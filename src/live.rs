use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Plan, RateWindow};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";

#[derive(Debug, Deserialize)]
struct AuthTokens {
    access_token: Option<String>,
    account_id: Option<String>,
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthFile {
    #[serde(default)]
    tokens: Option<AuthTokens>,
}

fn auth_path(codex_home: &Path) -> PathBuf {
    codex_home.join("auth.json")
}

fn read_auth(codex_home: &Path) -> anyhow::Result<AuthTokens> {
    let raw = fs::read_to_string(auth_path(codex_home)).context("read Codex auth.json")?;
    let auth: AuthFile = serde_json::from_str(&raw).context("parse Codex auth.json")?;
    auth.tokens
        .ok_or_else(|| anyhow!("auth.json has no tokens"))
}

fn label_window(seconds: u64) -> String {
    match seconds {
        s if s >= 2_419_200 => "monthly".into(),
        s if s >= 604_800 => "weekly".into(),
        s if s >= 86_400 => "daily".into(),
        s if s >= 18_000 => "5h".into(),
        s if s > 0 => format!("{}m", s / 60),
        _ => String::new(),
    }
}

pub fn humanize_until(epoch: Option<i64>) -> String {
    let Some(epoch) = epoch else {
        return "—".into();
    };
    let seconds = epoch - Utc::now().timestamp();
    if seconds <= 0 {
        return "0m".into();
    }
    let hours = seconds / 3600;
    if hours >= 24 {
        format!("{}d {}h", hours / 24, hours % 24)
    } else if hours >= 1 {
        format!("{hours}h")
    } else {
        format!("{}m", (seconds + 59) / 60)
    }
}

fn normalize_window(value: Option<&Value>) -> Option<RateWindow> {
    let value = value?;
    let used = value.get("used_percent")?.as_f64()?.clamp(0.0, 100.0) as f32;
    let seconds = value
        .get("limit_window_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let resets_at = value.get("reset_at").and_then(Value::as_i64);
    Some(RateWindow {
        used_percent: (used * 10.0).round() / 10.0,
        remaining_pct: (100.0 - used).round().clamp(0.0, 100.0) as u8,
        window_minutes: (seconds > 0).then_some(seconds / 60),
        window_label: label_window(seconds),
        resets_at,
        reset_label: humanize_until(resets_at),
    })
}

fn reset_credits_from_usage(value: &Value) -> Option<u32> {
    value
        .get("rate_limit_reset_credits")
        .and_then(|credits| {
            credits
                .get("applicable_available_count")
                .or_else(|| credits.get("available_count"))
        })
        .and_then(Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
}

fn fetch_reset_credits(
    client: &reqwest::blocking::Client,
    access_token: &str,
    account_id: Option<&str>,
) -> Option<u32> {
    let mut request = client
        .get(RESET_CREDITS_URL)
        .bearer_auth(access_token)
        .header("Accept", "application/json");
    if let Some(account_id) = account_id {
        request = request.header("chatgpt-account-id", account_id);
    }
    let response = request.send().ok()?.error_for_status().ok()?;
    let value: Value = response.json().ok()?;
    value
        .get("available_count")
        .and_then(Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
}

pub fn fetch_live_usage(codex_home: &Path) -> anyhow::Result<Plan> {
    let tokens = read_auth(codex_home)?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| anyhow!("no access_token in auth.json"))?;
    let mut client_builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("codex-calendar/0.1");

    if std::env::var_os("HTTPS_PROXY").is_none()
        && std::env::var_os("HTTP_PROXY").is_none()
        && let Ok(raw) = fs::read_to_string(codex_home.join(".env"))
        && let Some(proxy) = raw.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            matches!(
                key.trim().to_ascii_uppercase().as_str(),
                "HTTPS_PROXY" | "HTTP_PROXY"
            )
            .then(|| value.trim().trim_matches('"').to_owned())
        })
    {
        client_builder = client_builder.proxy(reqwest::Proxy::all(proxy)?);
    }

    let client = client_builder.build()?;
    let mut request = client
        .get(USAGE_URL)
        .bearer_auth(&access_token)
        .header("Accept", "application/json");
    if let Some(account_id) = tokens.account_id.as_deref() {
        request = request.header("chatgpt-account-id", account_id);
    }
    let response = request.send().context("request live Codex usage")?;
    if !response.status().is_success() {
        return Err(anyhow!("usage endpoint returned {}", response.status()));
    }
    let json: Value = response.json().context("decode live Codex usage")?;
    let reset_credits_available =
        fetch_reset_credits(&client, &access_token, tokens.account_id.as_deref())
            .or_else(|| reset_credits_from_usage(&json));
    let limits = json.get("rate_limit").unwrap_or(&Value::Null);
    Ok(Plan {
        plan_type: json
            .get("plan_type")
            .and_then(Value::as_str)
            .map(str::to_owned),
        limit_id: Some("codex".into()),
        primary: normalize_window(limits.get("primary_window")),
        secondary: normalize_window(limits.get("secondary_window")),
        reset_credits_available,
        snapshot_at: Some(Utc::now().to_rfc3339()),
        source: "live".into(),
        stale: false,
        expires_at: account_metadata(codex_home).1,
    })
}

pub fn account_metadata(codex_home: &Path) -> (Option<String>, Option<String>) {
    let Ok(tokens) = read_auth(codex_home) else {
        return (None, None);
    };
    let Some(jwt) = tokens.id_token else {
        return (None, None);
    };
    let Some(payload) = jwt.split('.').nth(1) else {
        return (None, None);
    };
    let Ok(decoded) = URL_SAFE_NO_PAD.decode(payload) else {
        return (None, None);
    };
    let Ok(claims) = serde_json::from_slice::<Value>(&decoded) else {
        return (None, None);
    };
    let nested = claims
        .get("https://api.openai.com/auth")
        .unwrap_or(&Value::Null);
    let plan = nested
        .get("chatgpt_plan_type")
        .or_else(|| claims.get("chatgpt_plan_type"))
        .or_else(|| nested.get("plan_type"))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_ascii_lowercase());
    let expiry = nested
        .get("chatgpt_subscription_active_until")
        .or_else(|| claims.get("chatgpt_subscription_active_until"))
        .and_then(|value| {
            value
                .as_str()
                .and_then(|raw| {
                    DateTime::parse_from_rfc3339(raw)
                        .ok()
                        .map(|d| d.with_timezone(&Utc).to_rfc3339())
                })
                .or_else(|| {
                    value.as_i64().and_then(|ts| {
                        DateTime::<Utc>::from_timestamp(ts, 0).map(|d| d.to_rfc3339())
                    })
                })
        });
    (plan, expiry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_labels_match_quota_periods() {
        assert_eq!(label_window(604_800), "weekly");
        assert_eq!(label_window(18_000), "5h");
    }

    #[test]
    fn embedded_reset_credit_count_prefers_applicable_count() {
        let value = serde_json::json!({
            "rate_limit_reset_credits": {
                "available_count": 3,
                "applicable_available_count": 1
            }
        });
        assert_eq!(reset_credits_from_usage(&value), Some(1));
    }
}
