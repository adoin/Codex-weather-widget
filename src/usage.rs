use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::Disks;
use walkdir::WalkDir;

use crate::{
    live::{account_metadata, humanize_until},
    model::{DailyUsage, Meter, Plan, ProjectUsage, RateWindow, TaskThroughput, UsageSnapshot},
    task_rates::TaskActivity,
};

const STATS_DAYS: u64 = 7;
const CACHE_RETENTION_DAYS: u64 = STATS_DAYS + 2;
const TAIL_INITIAL_BYTES: u64 = 1024 * 1024;
const CACHE_VERSION: u32 = 3;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DayBucket {
    used: u64,
    calls: u32,
    last_event_at: Option<DateTime<Utc>>,
    last_turn_tokens: u64,
    context_window: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ParsedSession {
    #[serde(default)]
    activity: TaskActivity,
    started_day: Option<NaiveDate>,
    provider: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    days: HashMap<NaiveDate, DayBucket>,
    rate_limits: Option<Value>,
    rate_limits_at: Option<DateTime<Utc>>,
    plan_type: Option<String>,
    #[serde(default)]
    previous_total: u64,
    #[serde(default)]
    subagent_created_at: Option<DateTime<Utc>>,
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

fn apply_record(session: &mut ParsedSession, record: &Value) {
    let payload = record.get("payload").unwrap_or(&Value::Null);
    let timestamp = parse_time(record.get("timestamp").or_else(|| payload.get("timestamp")));

    if record.get("type").and_then(Value::as_str) == Some("session_meta") {
        if let Some(id) = payload.get("id").and_then(Value::as_str) {
            session.activity.id = id.to_owned();
        }
        let is_subagent = payload.pointer("/source/subagent").is_some()
            || payload.get("thread_source").and_then(Value::as_str) == Some("subagent");
        if is_subagent && session.subagent_created_at.is_none() {
            session.subagent_created_at = timestamp;
        }
        if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
            session.cwd = Some(cwd.to_owned());
        }
        if let Some(provider) = payload.get("model_provider").and_then(Value::as_str) {
            session.provider = Some(provider.to_owned());
        }
        if let Some(model) = payload.get("model").and_then(Value::as_str) {
            session.model = Some(model.to_owned());
        }
        if let Some(timestamp) = timestamp {
            session.started_day = Some(local_day(timestamp));
        }
        return;
    }

    if record.get("type").and_then(Value::as_str) == Some("turn_context") {
        if let Some(model) = payload.get("model").and_then(Value::as_str) {
            session.model = Some(model.to_owned());
        }
    }
    if record.get("type").and_then(Value::as_str) == Some("event_msg")
        && let Some(at) = timestamp
        && session
            .subagent_created_at
            .is_none_or(|created| at > created + chrono::Duration::seconds(2))
    {
        session.activity.observe(payload, at);
    }
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return;
    }
    let info = payload.get("info").unwrap_or(&Value::Null);
    let total = info
        .pointer("/total_token_usage/total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let delta = if total >= session.previous_total {
        total - session.previous_total
    } else {
        // A compacted/restarted counter begins a new cumulative segment.
        total
    };
    session.previous_total = total;
    let is_replayed_subagent_prefix = timestamp.is_some_and(|event_at| {
        session
            .subagent_created_at
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

fn parse_session_incremental(
    path: &Path,
    mut session: ParsedSession,
    start_offset: u64,
) -> Option<(ParsedSession, u64)> {
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(start_offset)).ok()?;
    let mut reader = BufReader::new(file);
    let mut offset = start_offset;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            break;
        }
        let complete_line = line.ends_with('\n');
        let relevant = line.contains("\"event_msg\"")
            || line.contains("\"token_count\"")
            || line.contains("\"session_meta\"")
            || line.contains("\"turn_context\"");
        match relevant
            .then(|| serde_json::from_str::<Value>(&line))
            .transpose()
        {
            Ok(Some(record)) => {
                apply_record(&mut session, &record);
                offset = offset.saturating_add(bytes as u64);
            }
            Ok(None) => {
                offset = offset.saturating_add(bytes as u64);
            }
            Err(_) if complete_line => {
                offset = offset.saturating_add(bytes as u64);
            }
            Err(_) => break,
        }
    }
    Some((session, offset))
}

fn read_session_metadata(path: &Path) -> ParsedSession {
    let Ok(file) = File::open(path) else {
        return ParsedSession::default();
    };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok()
        && line.contains("\"session_meta\"")
        && let Ok(record) = serde_json::from_str::<Value>(&line)
    {
        let mut session = ParsedSession::default();
        apply_record(&mut session, &record);
        return session;
    }
    ParsedSession::default()
}

/// Find the most recent token counter just before the rolling stats window.
/// The search starts at the tail and grows exponentially, so reopening a large
/// dormant session does not require parsing its complete history.
fn recovery_start_offset(path: &Path, stats_start: NaiveDate) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    if file_len == 0 {
        return Some(0);
    }

    let mut window = TAIL_INITIAL_BYTES.min(file_len);
    loop {
        let start = file_len.saturating_sub(window);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut reader = BufReader::new(&mut file);
        let mut absolute_offset = start;

        if start > 0 {
            let mut partial = String::new();
            let skipped = reader.read_line(&mut partial).ok()?;
            absolute_offset = absolute_offset.saturating_add(skipped as u64);
        }

        let mut predecessor = None;
        let mut saw_recent = false;
        loop {
            let line_offset = absolute_offset;
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).ok()?;
            if bytes == 0 {
                break;
            }
            absolute_offset = absolute_offset.saturating_add(bytes as u64);
            if !line.ends_with('\n') || !line.contains("\"token_count\"") {
                continue;
            }
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let payload = record.get("payload").unwrap_or(&Value::Null);
            let Some(timestamp) =
                parse_time(record.get("timestamp").or_else(|| payload.get("timestamp")))
            else {
                continue;
            };
            if local_day(timestamp) < stats_start {
                predecessor = Some(line_offset);
            } else {
                saw_recent = true;
            }
        }

        if let Some(offset) = predecessor {
            return Some(offset);
        }
        if start == 0 {
            return Some(if saw_recent { 0 } else { file_len });
        }
        window = window.saturating_mul(2).min(file_len);
    }
}

fn parse_session_tail(path: &Path, stats_start: NaiveDate) -> Option<(ParsedSession, u64)> {
    let session = read_session_metadata(path);
    let offset = recovery_start_offset(path, stats_start)?;
    parse_session_incremental(path, session, offset)
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

fn recent_codex_jsonl(codex_home: &Path, days: u64) -> Vec<PathBuf> {
    let mut paths = recent_jsonl(&codex_home.join("sessions"), days);
    paths.extend(recent_jsonl(&codex_home.join("archived_sessions"), days));
    paths
}

fn stats_start_day() -> NaiveDate {
    let today = Local::now().date_naive();
    today
        .checked_sub_days(Days::new(STATS_DAYS.saturating_sub(1)))
        .unwrap_or(today)
}

fn session_overlaps_stats_window(session: &ParsedSession) -> bool {
    let today = Local::now().date_naive();
    let start = stats_start_day();
    session
        .days
        .keys()
        .any(|day| *day >= start && *day <= today)
}

fn prune_session_window(session: &mut ParsedSession) -> bool {
    let today = Local::now().date_naive();
    let start = stats_start_day();
    let previous_len = session.days.len();
    session.days.retain(|day, _| *day >= start && *day <= today);
    previous_len != session.days.len()
}

fn modified_millis(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn session_key(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| path.as_os_str().to_str().unwrap_or_default())
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedSession {
    path: String,
    offset: u64,
    file_len: u64,
    modified_millis: u64,
    session: ParsedSession,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedUsageCache {
    version: u32,
    codex_home: String,
    entries: HashMap<String, CachedSession>,
}

pub struct UsageCollector {
    codex_home: PathBuf,
    cache_path: PathBuf,
    cache: PersistedUsageCache,
}

impl UsageCollector {
    pub fn new(codex_home: PathBuf) -> Self {
        let cache_dir = dirs::data_local_dir()
            .or_else(dirs::cache_dir)
            .unwrap_or_else(|| codex_home.clone())
            .join("CodexCalendar");
        let cache_path = cache_dir.join("usage-cache-v3.json");
        Self::with_cache_path(codex_home, cache_path)
    }

    fn with_cache_path(codex_home: PathBuf, cache_path: PathBuf) -> Self {
        let home_key = fs::canonicalize(&codex_home)
            .unwrap_or_else(|_| codex_home.clone())
            .to_string_lossy()
            .into_owned();
        let cache = fs::read_to_string(&cache_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<PersistedUsageCache>(&raw).ok())
            .filter(|cache| cache.version == CACHE_VERSION && cache.codex_home == home_key)
            .unwrap_or_else(|| PersistedUsageCache {
                version: CACHE_VERSION,
                codex_home: home_key,
                entries: HashMap::new(),
            });
        Self {
            codex_home,
            cache_path,
            cache,
        }
    }

    fn save_cache(&self) {
        let Some(parent) = self.cache_path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_vec(&self.cache) {
            let _ = fs::write(&self.cache_path, json);
        }
    }

    fn sessions(&mut self) -> Vec<ParsedSession> {
        let paths = recent_codex_jsonl(&self.codex_home, CACHE_RETENTION_DAYS);
        let mut candidates: HashMap<String, (PathBuf, u64, u64)> = HashMap::new();
        for path in paths {
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            let key = session_key(&path);
            let file_len = metadata.len();
            let modified = modified_millis(&metadata);
            let replace = candidates
                .get(&key)
                .is_none_or(|(_, old_len, old_modified)| {
                    modified > *old_modified || (modified == *old_modified && file_len > *old_len)
                });
            if replace {
                candidates.insert(key, (path, file_len, modified));
            }
        }

        let discovered: HashSet<String> = candidates.keys().cloned().collect();
        let mut changed = false;
        let stats_start = stats_start_day();

        for (key, (path, file_len, modified_millis)) in candidates {
            let path_key = path.to_string_lossy().into_owned();
            let existing = self.cache.entries.get(&key).cloned();

            if let Some(cached) = existing.as_ref()
                && file_len == cached.file_len
                && modified_millis == cached.modified_millis
            {
                if cached.path != path_key
                    && let Some(entry) = self.cache.entries.get_mut(&key)
                {
                    entry.path = path_key;
                    changed = true;
                }
                continue;
            }

            let parsed = match existing {
                Some(cached) if file_len > cached.file_len && cached.offset <= file_len => {
                    parse_session_incremental(&path, cached.session, cached.offset)
                }
                _ => parse_session_tail(&path, stats_start),
            };
            if let Some((mut session, offset)) = parsed {
                prune_session_window(&mut session);
                self.cache.entries.insert(
                    key,
                    CachedSession {
                        path: path_key,
                        offset,
                        file_len,
                        modified_millis,
                        session,
                    },
                );
                changed = true;
            }
        }

        for cached in self.cache.entries.values_mut() {
            changed |= prune_session_window(&mut cached.session);
        }

        let retention_cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(CACHE_RETENTION_DAYS * 86_400))
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        let previous_count = self.cache.entries.len();
        self.cache.entries.retain(|key, cached| {
            discovered.contains(key)
                || cached.modified_millis >= retention_cutoff
                || session_overlaps_stats_window(&cached.session)
        });
        changed |= self.cache.entries.len() != previous_count;
        if changed {
            self.save_cache();
        }
        self.cache
            .entries
            .values()
            .map(|cached| cached.session.clone())
            .collect()
    }

    pub fn collect(&mut self, daily_budget: u64, live_plan: Option<Plan>) -> UsageSnapshot {
        let sessions = self.sessions();
        collect_usage_from_sessions(&self.codex_home, daily_budget, live_plan, sessions)
    }
}

fn task_throughput(
    codex_home: &Path,
    sessions: &[ParsedSession],
    now: DateTime<Utc>,
) -> Vec<TaskThroughput> {
    let mut titles = HashMap::new();
    if let Ok(file) = File::open(codex_home.join("session_index.jsonl")) {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && let (Some(id), Some(title)) = (
                    value.get("id").and_then(Value::as_str),
                    value.get("thread_name").and_then(Value::as_str),
                )
            {
                titles.insert(id.to_owned(), title.to_owned());
            }
        }
    }
    let mut tasks: Vec<_> = sessions
        .iter()
        .filter_map(|session| {
            let activity = &session.activity;
            let updated = activity.updated_at?;
            if activity.id.is_empty() || local_day(updated) != local_day(now) {
                return None;
            }
            let running = activity.running && (now - updated).num_seconds() < 15 * 60;
            let title = titles.get(&activity.id).cloned().unwrap_or_else(|| {
                let project = session
                    .cwd
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .and_then(|p| p.to_str())
                    .unwrap_or("Codex");
                format!(
                    "{project} · {}",
                    activity
                        .id
                        .chars()
                        .rev()
                        .take(8)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                )
            });
            Some(TaskThroughput {
                id: activity.id.clone(),
                title,
                model: session.model.clone(),
                cwd: session.cwd.clone(),
                running,
                output_tokens: activity.output_tokens,
                tokens_per_second: if running { activity.tps(now) } else { None },
                updated_at: updated.to_rfc3339(),
            })
        })
        .collect();
    tasks.sort_by(|a, b| {
        b.running
            .cmp(&a.running)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    tasks
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
    let stats_start = stats_start_day();
    let sessions = recent_codex_jsonl(codex_home, STATS_DAYS)
        .iter()
        .filter_map(|path| parse_session_tail(path, stats_start).map(|(session, _)| session))
        .collect();
    collect_usage_from_sessions(codex_home, daily_budget, live_plan, sessions)
}

fn collect_usage_from_sessions(
    codex_home: &Path,
    daily_budget: u64,
    live_plan: Option<Plan>,
    mut sessions: Vec<ParsedSession>,
) -> UsageSnapshot {
    let now = Local::now();
    let today = now.date_naive();
    let sessions_dir = codex_home.join("sessions");
    let (account_plan, account_expiry) = account_metadata(codex_home);
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
        task_throughput: task_throughput(codex_home, &sessions, Utc::now()),
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
    fn task_rows_keep_sessions_distinct_and_load_names_from_index() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("session_index.jsonl"), "{\"id\":\"a\",\"thread_name\":\"First task\"}\n{\"id\":\"b\",\"thread_name\":\"Second task\"}\n").unwrap();
        let now = Utc::now();
        let mut sessions = Vec::new();
        for (id, output) in [("a", 100), ("b", 40)] {
            let mut session = ParsedSession::default();
            apply_record(
                &mut session,
                &serde_json::json!({"type":"session_meta","timestamp":(now-chrono::Duration::seconds(5)).to_rfc3339(),"payload":{"id":id,"cwd":"same-project"}}),
            );
            apply_record(
                &mut session,
                &serde_json::json!({"type":"event_msg","timestamp":(now-chrono::Duration::seconds(4)).to_rfc3339(),"payload":{"type":"task_started"}}),
            );
            apply_record(
                &mut session,
                &serde_json::json!({"type":"event_msg","timestamp":(now-chrono::Duration::seconds(2)).to_rfc3339(),"payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":output,"total_tokens":999999},"last_token_usage":{"output_tokens":output}}}}),
            );
            sessions.push(session);
        }
        let rows = task_throughput(temp.path(), &sessions, now);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "First task");
        assert_eq!(rows[0].tokens_per_second, Some(50.0));
        assert_eq!(rows[1].tokens_per_second, Some(20.0));
        sessions[0]
            .activity
            .observe(&serde_json::json!({"type":"task_complete"}), now);
        let rows = task_throughput(temp.path(), &sessions, now);
        assert_eq!(rows[0].id, "b");
        assert!(!rows[1].running);
        assert_eq!(rows[1].tokens_per_second, None);
    }

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
    fn persisted_offset_handles_an_old_session_reopened_today() {
        let temp = tempfile::tempdir().unwrap();
        let old_day = Local::now().date_naive() - chrono::Duration::days(10);
        let dir = temp
            .path()
            .join("sessions")
            .join(old_day.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&dir).unwrap();
        let session_path = dir.join("rollout-old-session.jsonl");
        let old_timestamp = Utc::now() - chrono::Duration::days(10);
        {
            let mut file = File::create(&session_path).unwrap();
            writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"old-project\"}}}}", old_timestamp.to_rfc3339()).unwrap();
            writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":100}},\"last_token_usage\":{{\"total_tokens\":100}},\"model_context_window\":1000}}}}}}", old_timestamp.to_rfc3339()).unwrap();
        }
        let cache_path = temp.path().join("usage-cache.json");
        let mut first =
            UsageCollector::with_cache_path(temp.path().to_path_buf(), cache_path.clone());
        assert_eq!(first.collect(1_000, None).daily.used, 0);
        drop(first);

        {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&session_path)
                .unwrap();
            writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":250}},\"last_token_usage\":{{\"total_tokens\":150}},\"model_context_window\":1000}}}}}}", Utc::now().to_rfc3339()).unwrap();
        }

        let mut reopened = UsageCollector::with_cache_path(temp.path().to_path_buf(), cache_path);
        let snapshot = reopened.collect(1_000, None);
        assert_eq!(snapshot.daily.used, 150);
        assert_eq!(snapshot.calls_today, 1);
        assert_eq!(snapshot.project_usage[0].name, "old-project");
    }

    #[test]
    fn cold_tail_recovery_counts_only_the_recent_delta() {
        let temp = tempfile::tempdir().unwrap();
        let old_day = Local::now().date_naive() - chrono::Duration::days(30);
        let dir = temp
            .path()
            .join("sessions")
            .join(old_day.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&dir).unwrap();
        let session_path = dir.join("rollout-cold-session.jsonl");
        let old_timestamp = Utc::now() - chrono::Duration::days(30);
        let mut file = File::create(&session_path).unwrap();
        writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"cold-project\"}}}}", old_timestamp.to_rfc3339()).unwrap();
        writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":100}},\"last_token_usage\":{{\"total_tokens\":100}}}}}}}}", old_timestamp.to_rfc3339()).unwrap();
        let padding = "x".repeat((TAIL_INITIAL_BYTES + 4096) as usize);
        writeln!(
            file,
            "{{\"type\":\"response_item\",\"payload\":\"{padding}\"}}"
        )
        .unwrap();
        writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":250}},\"last_token_usage\":{{\"total_tokens\":150}}}}}}}}", Utc::now().to_rfc3339()).unwrap();
        drop(file);

        let mut collector = UsageCollector::with_cache_path(
            temp.path().to_path_buf(),
            temp.path().join("usage-cache.json"),
        );
        let snapshot = collector.collect(1_000, None);
        assert_eq!(snapshot.daily.used, 150);
        assert_eq!(snapshot.calls_today, 1);
        assert_eq!(snapshot.project_usage[0].name, "cold-project");
    }

    #[test]
    fn moving_a_session_to_the_archive_does_not_duplicate_it() {
        let temp = tempfile::tempdir().unwrap();
        let today = Local::now();
        let active_dir = temp
            .path()
            .join("sessions")
            .join(today.format("%Y/%m/%d").to_string());
        let archive_dir = temp.path().join("archived_sessions");
        fs::create_dir_all(&active_dir).unwrap();
        fs::create_dir_all(&archive_dir).unwrap();
        let active_path = active_dir.join("rollout-stable-id.jsonl");
        let archived_path = archive_dir.join("rollout-stable-id.jsonl");
        let mut file = File::create(&active_path).unwrap();
        writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"moved-project\"}}}}", Utc::now().to_rfc3339()).unwrap();
        writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":300}},\"last_token_usage\":{{\"total_tokens\":300}}}}}}}}", Utc::now().to_rfc3339()).unwrap();
        drop(file);

        let cache_path = temp.path().join("usage-cache.json");
        let mut collector = UsageCollector::with_cache_path(temp.path().to_path_buf(), cache_path);
        assert_eq!(collector.collect(1_000, None).daily.used, 300);

        fs::rename(&active_path, &archived_path).unwrap();
        let snapshot = collector.collect(1_000, None);
        assert_eq!(snapshot.daily.used, 300);
        assert_eq!(collector.cache.entries.len(), 1);
        assert!(
            collector
                .cache
                .entries
                .values()
                .next()
                .unwrap()
                .path
                .contains("archived_sessions")
        );
    }

    #[test]
    fn inactive_cursor_without_recent_usage_is_evicted() {
        let temp = tempfile::tempdir().unwrap();
        let mut collector = UsageCollector::with_cache_path(
            temp.path().to_path_buf(),
            temp.path().join("usage-cache.json"),
        );
        collector.cache.entries.insert(
            "rollout-stale.jsonl".into(),
            CachedSession {
                path: temp
                    .path()
                    .join("missing.jsonl")
                    .to_string_lossy()
                    .into_owned(),
                offset: 10,
                file_len: 10,
                modified_millis: 0,
                session: ParsedSession::default(),
            },
        );

        assert!(collector.sessions().is_empty());
        assert!(collector.cache.entries.is_empty());
    }

    #[test]
    fn archived_file_keeps_recent_usage_until_it_leaves_the_window() {
        let temp = tempfile::tempdir().unwrap();
        let today = Local::now();
        let dir = temp
            .path()
            .join("sessions")
            .join(today.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&dir).unwrap();
        let session_path = dir.join("rollout-archived.jsonl");
        {
            let mut file = File::create(&session_path).unwrap();
            writeln!(file, "{{\"type\":\"session_meta\",\"timestamp\":\"{}\",\"payload\":{{\"cwd\":\"archived-project\"}}}}", Utc::now().to_rfc3339()).unwrap();
            writeln!(file, "{{\"timestamp\":\"{}\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":250}},\"last_token_usage\":{{\"total_tokens\":250}},\"model_context_window\":1000}}}}}}", Utc::now().to_rfc3339()).unwrap();
        }
        let cache_path = temp.path().join("usage-cache.json");
        let mut collector =
            UsageCollector::with_cache_path(temp.path().to_path_buf(), cache_path.clone());
        assert_eq!(collector.collect(1_000, None).daily.used, 250);
        drop(collector);

        fs::remove_file(&session_path).unwrap();
        let mut after_archive =
            UsageCollector::with_cache_path(temp.path().to_path_buf(), cache_path);
        let snapshot = after_archive.collect(1_000, None);
        assert_eq!(snapshot.daily.used, 250);
        assert_eq!(snapshot.project_usage[0].name, "archived-project");
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
