use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Utc};
use serde_json::Value;
use sysinfo::Disks;
use walkdir::WalkDir;

use crate::{
    live::{account_metadata, humanize_until},
    model::{DailyUsage, Meter, Plan, ProjectUsage, RateWindow, UsageSnapshot},
};

#[derive(Debug, Default)]
struct DayBucket {
    used: u64,
    calls: u32,
    last_event_at: Option<DateTime<Utc>>,
    last_turn_tokens: u64,
    context_window: u64,
}

#[derive(Debug, Default)]
struct ParsedSession {
    started_day: Option<NaiveDate>,
    provider: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    days: HashMap<NaiveDate, DayBucket>,
    rate_limits: Option<Value>,
    rate_limits_at: Option<DateTime<Utc>>,
    plan_type: Option<String>,
}

fn looks_like_codex_home(path: &Path) -> bool {
    path.join("sessions").is_dir() || path.join("auth.json").is_file()
}

fn newest_session_mtime(path: &Path) -> Option<SystemTime> {
    WalkDir::new(path.join("sessions"))
        .min_depth(1)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "jsonl")
        })
        .filter_map(|entry| fs::metadata(entry.path()).and_then(|m| m.modified()).ok())
        .max()
}

/// Resolve the Codex data directory without assuming it lives under the user profile.
/// Explicit configuration and `CODEX_HOME` win; otherwise choose the candidate with
/// the most recently modified session file.
pub fn discover_codex_home(configured: Option<&str>) -> PathBuf {
    if let Some(path) = configured.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if looks_like_codex_home(&path) {
            return path;
        }
    }
    if let Some(path) = std::env::var_os("CODEX_HOME").map(PathBuf::from)
        && looks_like_codex_home(&path)
    {
        return path;
    }

    let fallback = dirs::home_dir()
        .map(|home| home.join(".codex"))
        .unwrap_or_else(|| PathBuf::from(".codex"));
    let mut candidates = vec![fallback.clone()];
    if cfg!(target_os = "windows") {
        for disk in Disks::new_with_refreshed_list().list() {
            candidates.push(disk.mount_point().join("codex_home"));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
        .into_iter()
        .filter(|path| looks_like_codex_home(path))
        .max_by_key(|path| newest_session_mtime(path).unwrap_or(SystemTime::UNIX_EPOCH))
        .unwrap_or(fallback)
}

fn parse_time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn local_day(timestamp: DateTime<Utc>) -> NaiveDate {
    timestamp.with_timezone(&Local).date_naive()
}

fn parse_session(path: &Path) -> Option<ParsedSession> {
    let file = File::open(path).ok()?;
    let mut session = ParsedSession::default();
    let mut previous_total = 0_u64;
    let mut subagent_created_at = None;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = record.get("payload").unwrap_or(&Value::Null);
        let timestamp = parse_time(record.get("timestamp").or_else(|| payload.get("timestamp")));

        if record.get("type").and_then(Value::as_str) == Some("session_meta") {
            let is_subagent = payload.pointer("/source/subagent").is_some()
                || payload.get("thread_source").and_then(Value::as_str) == Some("subagent");
            if is_subagent && subagent_created_at.is_none() {
                subagent_created_at = timestamp;
            }
            session.cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(session.cwd);
            session.provider = payload
                .get("model_provider")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(session.provider);
            session.model = payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(session.model);
            session.started_day = timestamp.map(local_day).or(session.started_day);
            continue;
        }

        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let info = payload.get("info").unwrap_or(&Value::Null);
        let total = info
            .pointer("/total_token_usage/total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let delta = total.saturating_sub(previous_total);
        previous_total = previous_total.max(total);
        let is_replayed_subagent_prefix = timestamp.is_some_and(|event_at| {
            subagent_created_at
                .is_some_and(|created_at| event_at <= created_at + chrono::Duration::seconds(2))
        });
        let last_turn = info
            .pointer("/last_token_usage/total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let context_window = info
            .get("model_context_window")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        if let Some(timestamp) = timestamp
            && !is_replayed_subagent_prefix
        {
            let bucket = session.days.entry(local_day(timestamp)).or_default();
            bucket.used = bucket.used.saturating_add(delta);
            if last_turn > 0 {
                bucket.calls += 1;
            }
            if bucket.last_event_at.is_none_or(|last| timestamp >= last) {
                bucket.last_event_at = Some(timestamp);
                bucket.last_turn_tokens = last_turn;
                bucket.context_window = context_window;
            }
        }

        if let Some(limits) = payload.get("rate_limits").filter(|v| !v.is_null()) {
            let has_limits = limits.get("primary").is_some() || limits.get("secondary").is_some();
            if has_limits {
                session.rate_limits = Some(limits.clone());
                session.rate_limits_at = timestamp;
            }
            if let Some(plan) = limits.get("plan_type").and_then(Value::as_str) {
                session.plan_type = Some(plan.to_owned());
            }
        }
    }
    Some(session)
}

fn recent_jsonl(sessions_dir: &Path, days: u64) -> Vec<PathBuf> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(days * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    WalkDir::new(sessions_dir)
        .min_depth(1)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "jsonl")
        })
        .filter(|entry| {
            fs::metadata(entry.path())
                .and_then(|m| m.modified())
                .is_ok_and(|mtime| mtime >= cutoff)
        })
        .map(|entry| entry.into_path())
        .collect()
}

fn window_label(minutes: Option<u64>) -> String {
    match minutes.unwrap_or(0) {
        m if m >= 40_320 => "monthly".into(),
        m if m >= 10_080 => "weekly".into(),
        m if m >= 1_440 => "daily".into(),
        m if m >= 300 => "5h".into(),
        m if m > 0 => format!("{m}m"),
        _ => String::new(),
    }
}

fn normalize_session_window(value: Option<&Value>) -> Option<RateWindow> {
    let value = value?;
    let used = value
        .get("used_percent")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 100.0) as f32;
    let minutes = value.get("window_minutes").and_then(Value::as_u64);
    let resets_at = value.get("resets_at").and_then(Value::as_i64);
    Some(RateWindow {
        used_percent: used,
        remaining_pct: (100.0 - used).round() as u8,
        window_minutes: minutes,
        window_label: window_label(minutes),
        resets_at,
        reset_label: humanize_until(resets_at),
    })
}

fn session_plan(
    session: &ParsedSession,
    account_plan: Option<String>,
    expires_at: Option<String>,
) -> Option<Plan> {
    let limits = session.rate_limits.as_ref()?;
    Some(Plan {
        plan_type: limits
            .get("plan_type")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| session.plan_type.clone())
            .or(account_plan),
        limit_id: limits
            .get("limit_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        primary: normalize_session_window(limits.get("primary")),
        secondary: normalize_session_window(limits.get("secondary")),
        reset_credits_available: limits
            .get("rate_limit_reset_credits")
            .and_then(|credits| {
                credits
                    .get("applicable_available_count")
                    .or_else(|| credits.get("available_count"))
            })
            .and_then(Value::as_u64)
            .and_then(|count| u32::try_from(count).ok()),
        snapshot_at: session.rate_limits_at.map(|t| t.to_rfc3339()),
        source: "session".into(),
        stale: false,
        expires_at,
    })
}

pub fn collect_usage(daily_budget: u64, live_plan: Option<Plan>) -> UsageSnapshot {
    collect_usage_from(&discover_codex_home(None), daily_budget, live_plan)
}

pub fn collect_usage_from(
    codex_home: &Path,
    daily_budget: u64,
    live_plan: Option<Plan>,
) -> UsageSnapshot {
    let now = Local::now();
    let today = now.date_naive();
    let sessions_dir = codex_home.join("sessions");
    let (account_plan, account_expiry) = account_metadata(codex_home);
    let mut sessions: Vec<_> = recent_jsonl(&sessions_dir, 15)
        .iter()
        .filter_map(|path| parse_session(path))
        .collect();
    sessions.sort_by_key(|session| session.days.values().filter_map(|b| b.last_event_at).max());

    let mut used_today = 0_u64;
    let mut calls_today = 0_u32;
    let mut sessions_today = 0_u32;
    let mut latest: Option<&ParsedSession> = None;
    let mut latest_at: Option<DateTime<Utc>> = None;
    let stats_start = today.checked_sub_days(Days::new(6)).unwrap_or(today);
    let mut daily_totals: HashMap<NaiveDate, u64> = HashMap::new();
    let mut project_totals: HashMap<String, (String, u64)> = HashMap::new();
    for session in &sessions {
        let bucket = session.days.get(&today);
        if bucket.is_some() || session.started_day == Some(today) {
            sessions_today += 1;
        }
        if let Some(bucket) = bucket {
            used_today = used_today.saturating_add(bucket.used);
            calls_today = calls_today.saturating_add(bucket.calls);
            if bucket
                .last_event_at
                .is_some_and(|at| latest_at.is_none_or(|current| at > current))
            {
                latest_at = bucket.last_event_at;
                latest = Some(session);
            }
        }

        let recent_tokens = session
            .days
            .iter()
            .filter(|(day, _)| **day >= stats_start && **day <= today)
            .fold(0_u64, |total, (day, bucket)| {
                daily_totals
                    .entry(*day)
                    .and_modify(|tokens| *tokens = tokens.saturating_add(bucket.used))
                    .or_insert(bucket.used);
                total.saturating_add(bucket.used)
            });
        if recent_tokens > 0
            && let Some(cwd) = session.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty())
        {
            let project_name = Path::new(cwd)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(cwd)
                .to_owned();
            project_totals
                .entry(cwd.to_owned())
                .and_modify(|(_, tokens)| *tokens = tokens.saturating_add(recent_tokens))
                .or_insert((project_name, recent_tokens));
        }
    }

    let mut project_usage: Vec<ProjectUsage> = project_totals
        .into_iter()
        .map(|(cwd, (name, tokens))| ProjectUsage { name, cwd, tokens })
        .collect();
    project_usage.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut recent_daily_usage = Vec::with_capacity(7);
    let mut day = stats_start;
    loop {
        recent_daily_usage.push(DailyUsage {
            date: day.format("%Y-%m-%d").to_string(),
            tokens: daily_totals.get(&day).copied().unwrap_or(0),
        });
        if day >= today {
            break;
        }
        day = day.succ_opt().unwrap_or(today);
    }

    let latest_bucket = latest.and_then(|s| s.days.get(&today));
    let context_used = latest_bucket.map_or(0, |b| b.last_turn_tokens);
    let context_limit = latest_bucket.map_or(0, |b| b.context_window);
    let fallback_plan = sessions
        .iter()
        .filter(|s| s.rate_limits.is_some())
        .max_by_key(|s| s.rate_limits_at)
        .and_then(|s| session_plan(s, account_plan.clone(), account_expiry.clone()));
    let plan = live_plan.or(fallback_plan).or_else(|| {
        (account_plan.is_some() || account_expiry.is_some()).then(|| Plan {
            plan_type: account_plan,
            expires_at: account_expiry,
            source: "account".into(),
            ..Plan::default()
        })
    });

    let tomorrow = now.date_naive().succ_opt().unwrap_or(today);
    let reset = Local
        .with_ymd_and_hms(tomorrow.year(), tomorrow.month(), tomorrow.day(), 0, 0, 0)
        .single();
    let minutes_to_reset = reset.map_or(0, |r| (r - now).num_minutes().max(0));
    let daily_remaining = if daily_budget == 0 {
        100
    } else {
        ((daily_budget.saturating_sub(used_today) as f64 / daily_budget as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8
    };
    let context_remaining = if context_limit == 0 {
        100
    } else {
        ((context_limit.saturating_sub(context_used) as f64 / context_limit as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8
    };

    UsageSnapshot {
        ok: true,
        updated_at: Utc::now().to_rfc3339(),
        provider: latest.and_then(|s| s.provider.clone()),
        model: latest.and_then(|s| s.model.clone()),
        active_cwd: latest.and_then(|s| s.cwd.clone()),
        plan,
        daily: Meter {
            used: used_today,
            limit: daily_budget,
            remaining_pct: daily_remaining,
        },
        context: Meter {
            used: context_used,
            limit: context_limit,
            remaining_pct: context_remaining,
        },
        calls_today,
        sessions_today,
        project_usage,
        recent_daily_usage,
        reset_in_hours: (minutes_to_reset / 60) as u32,
        reset_label: if minutes_to_reset >= 60 {
            format!("{}h", minutes_to_reset / 60)
        } else {
            format!("{minutes_to_reset}m")
        },
        status_message: if fs::metadata(&sessions_dir).is_ok() {
            None
        } else {
            Some("还没有找到 Codex 会话记录".into())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn aggregates_token_deltas_instead_of_cumulative_totals() {
        let temp = tempfile::tempdir().unwrap();
        let today = Local::now();
        let dir = temp
            .path()
            .join("sessions")
            .join(today.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&dir).unwrap();
        let mut file = File::create(dir.join("rollout-test.jsonl")).unwrap();
        writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"demo\",\"model_provider\":\"openai\"}}}}", Utc::now().to_rfc3339()).unwrap();
        for total in [100_u64, 250] {
            writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{total}}},\"last_token_usage\":{{\"total_tokens\":50}},\"model_context_window\":1000}}}}}}", Utc::now().to_rfc3339()).unwrap();
        }
        let snapshot = collect_usage_from(temp.path(), 1_000, None);
        assert_eq!(snapshot.daily.used, 250);
        assert_eq!(snapshot.calls_today, 2);
        assert_eq!(snapshot.context.used, 50);
        assert_eq!(snapshot.project_usage.len(), 1);
        assert_eq!(snapshot.project_usage[0].name, "demo");
        assert_eq!(snapshot.project_usage[0].tokens, 250);
        assert_eq!(snapshot.recent_daily_usage.len(), 7);
        assert_eq!(snapshot.recent_daily_usage.last().unwrap().tokens, 250);
    }

    #[test]
    fn subagent_replay_is_used_as_a_baseline_not_new_usage() {
        let temp = tempfile::tempdir().unwrap();
        let today = Local::now();
        let dir = temp
            .path()
            .join("sessions")
            .join(today.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&dir).unwrap();
        let mut file = File::create(dir.join("rollout-subagent.jsonl")).unwrap();
        let created_at = Utc::now() - chrono::Duration::minutes(5);
        writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"demo\",\"thread_source\":\"subagent\",\"source\":{{\"subagent\":{{\"thread_spawn\":{{\"parent_thread_id\":\"parent\"}}}}}}}}}}", created_at.to_rfc3339()).unwrap();
        for (offset_ms, total) in [(100_i64, 100_u64), (300, 300)] {
            let timestamp = created_at + chrono::Duration::milliseconds(offset_ms);
            writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{total}}},\"last_token_usage\":{{\"total_tokens\":100}},\"model_context_window\":1000}}}}}}", timestamp.to_rfc3339()).unwrap();
        }
        let live_at = created_at + chrono::Duration::minutes(1);
        writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":450}},\"last_token_usage\":{{\"total_tokens\":150}},\"model_context_window\":1000}}}}}}", live_at.to_rfc3339()).unwrap();

        let snapshot = collect_usage_from(temp.path(), 1_000, None);
        assert_eq!(snapshot.daily.used, 150);
        assert_eq!(snapshot.calls_today, 1);
        assert_eq!(snapshot.project_usage[0].tokens, 150);
    }

    #[test]
    fn explicit_codex_home_is_respected() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("sessions")).unwrap();
        assert_eq!(
            discover_codex_home(temp.path().to_str()),
            temp.path().to_path_buf()
        );
    }
}
