use std::{fs, path::PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WeatherProvider {
    #[default]
    Auto,
    OpenMeteo,
    QWeather,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WeatherIconTheme {
    #[default]
    Crystal,
    Duotone,
    Neon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub codex_home: Option<String>,
    pub port: u16,
    pub refresh_ms: u64,
    pub live_usage_ms: u64,
    pub daily_budget_tokens: u64,
    pub always_on_top: bool,
    pub follow_codex: bool,
    pub watch_processes: Vec<String>,
    pub watch_interval_ms: u64,
    pub reduced_motion: bool,
    pub auto_scene: bool,
    pub scene: usize,
    pub background: usize,
    pub background_opacity: f32,
    pub desktop_glass: bool,
    pub weather_provider: WeatherProvider,
    pub weather_icon_theme: WeatherIconTheme,
    pub qweather_api_host: String,
    pub weather_location: String,
    pub weather_display_name: Option<String>,
    pub weather_latitude: Option<f64>,
    pub weather_longitude: Option<f64>,
    pub weather_refresh_minutes: u64,
    pub desktop_attached: bool,
    pub position_locked: bool,
    pub auto_start: bool,
    pub language: Option<String>,
    pub window_x: Option<f32>,
    pub window_y: Option<f32>,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            codex_home: None,
            port: 8787,
            refresh_ms: 4_000,
            live_usage_ms: 60_000,
            daily_budget_tokens: 20_000_000,
            always_on_top: true,
            follow_codex: true,
            watch_processes: vec!["Codex".into(), "ChatGPT".into()],
            watch_interval_ms: 4_000,
            reduced_motion: false,
            auto_scene: true,
            scene: 3,
            background: 0,
            background_opacity: 1.0,
            desktop_glass: cfg!(target_os = "windows"),
            weather_provider: WeatherProvider::Auto,
            weather_icon_theme: WeatherIconTheme::Crystal,
            qweather_api_host: String::new(),
            weather_location: String::new(),
            weather_display_name: None,
            weather_latitude: None,
            weather_longitude: None,
            weather_refresh_minutes: 30,
            desktop_attached: cfg!(target_os = "windows"),
            position_locked: false,
            auto_start: false,
            language: None,
            window_x: None,
            window_y: None,
            window_width: 392.0,
            window_height: 466.0,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("CodexCalendar")
            .join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let (mut config, mut needs_save) = match fs::read_to_string(&path) {
            Ok(raw) => (serde_json::from_str(&raw).unwrap_or_default(), false),
            Err(_) => (Self::default(), true),
        };
        if (config.window_height - 548.0).abs() < f32::EPSILON
            || (config.window_height - 500.0).abs() < f32::EPSILON
            || (config.window_height - 450.0).abs() < f32::EPSILON
        {
            config.window_height = 466.0;
            needs_save = true;
        }
        if config.language.is_none() {
            config.language = Some(crate::i18n::Language::system_default().code().to_owned());
            needs_save = true;
        }
        if !config.background_opacity.is_finite()
            || !(0.0..=1.0).contains(&config.background_opacity)
        {
            config.background_opacity = 1.0;
            needs_save = true;
        }
        if needs_save {
            let _ = config.save();
        }
        config
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("write {}", path.display()))
    }
}
