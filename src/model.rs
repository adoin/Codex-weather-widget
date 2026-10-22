use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    pub used_percent: f32,
    pub remaining_pct: u8,
    pub window_minutes: Option<u64>,
    pub window_label: String,
    pub resets_at: Option<i64>,
    pub reset_label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_type: Option<String>,
    pub limit_id: Option<String>,
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
    #[serde(default)]
    pub reset_credits_available: Option<u32>,
    pub snapshot_at: Option<String>,
    pub source: String,
    pub stale: bool,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    pub used: u64,
    pub limit: u64,
    pub remaining_pct: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    pub name: String,
    pub cwd: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskThroughput {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub running: bool,
    pub output_tokens: u64,
    pub tokens_per_second: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub ok: bool,
    pub updated_at: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub active_cwd: Option<String>,
    pub plan: Option<Plan>,
    pub daily: Meter,
    pub context: Meter,
    pub calls_today: u32,
    pub sessions_today: u32,
    #[serde(default)]
    pub project_usage: Vec<ProjectUsage>,
    #[serde(default)]
    pub recent_daily_usage: Vec<DailyUsage>,
    #[serde(default)]
    pub task_throughput: Vec<TaskThroughput>,
    pub reset_in_hours: u32,
    pub reset_label: String,
    pub status_message: Option<String>,
}

impl Default for UsageSnapshot {
    fn default() -> Self {
        Self {
            ok: true,
            updated_at: chrono::Utc::now().to_rfc3339(),
            provider: None,
            model: None,
            active_cwd: None,
            plan: None,
            daily: Meter {
                used: 0,
                limit: 20_000_000,
                remaining_pct: 100,
            },
            context: Meter {
                used: 0,
                limit: 0,
                remaining_pct: 100,
            },
            calls_today: 0,
            sessions_today: 0,
            project_usage: Vec::new(),
            recent_daily_usage: Vec::new(),
            task_throughput: Vec::new(),
            reset_in_hours: 0,
            reset_label: "—".into(),
            status_message: Some("正在读取 Codex 记录".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    Show,
    ShowLanguageMenu,
    ShowStatistics,
    ShowQuota,
    Hide,
    Toggle,
    Quit,
}
