use std::{collections::HashSet, time::Duration};

use anyhow::{Context, anyhow};
use chrono::{Local, Utc};
use serde::Deserialize;

use crate::{config::WeatherProvider, i18n::Language};

const PAST_DAYS: usize = 6;
const OPEN_METEO_GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";
const OPEN_METEO_FORECAST: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Debug, Clone)]
pub struct WeatherServices {
    pub provider: WeatherProvider,
    pub qweather_api_host: String,
    pub qweather_api_key: Option<String>,
}

impl WeatherServices {
    pub fn new(
        provider: WeatherProvider,
        qweather_api_host: impl Into<String>,
        qweather_api_key: Option<String>,
    ) -> Self {
        Self {
            provider,
            qweather_api_host: qweather_api_host.into(),
            qweather_api_key: qweather_api_key.filter(|key| !key.trim().is_empty()),
        }
    }

    fn qweather_credentials(&self, language: Language) -> anyhow::Result<(&str, &str)> {
        let host = self.qweather_api_host.trim();
        let key = self.qweather_api_key.as_deref().unwrap_or_default().trim();
        if host.is_empty() || key.is_empty() {
            return Err(anyhow!(message(
                language,
                "请先填写和风天气 API Host 并保存 API Key",
                "Enter the QWeather API Host and save an API key first"
            )));
        }
        Ok((host, key))
    }

    fn has_qweather_credentials(&self) -> bool {
        !self.qweather_api_host.trim().is_empty()
            && self
                .qweather_api_key
                .as_deref()
                .is_some_and(|key| !key.trim().is_empty())
    }
}

impl Default for WeatherServices {
    fn default() -> Self {
        Self::new(WeatherProvider::OpenMeteo, "", None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherDataSource {
    OpenMeteo,
    QWeather,
}

impl WeatherDataSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenMeteo => "Open-Meteo",
            Self::QWeather => "QWeather",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocationCandidate {
    pub name: String,
    pub full_name: Option<String>,
    pub admin1: Option<String>,
    pub admin2: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

impl LocationCandidate {
    pub fn display_name(&self) -> String {
        if let Some(full_name) = self.full_name.as_deref() {
            return full_name.replace(", ", " · ");
        }
        let mut parts = vec![self.name.clone()];
        for part in [&self.admin2, &self.admin1, &self.country]
            .into_iter()
            .flatten()
        {
            if !parts.contains(part) {
                parts.push(part.clone());
            }
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone)]
pub struct WeatherDay {
    pub date: String,
    pub weather_code: u16,
    pub qweather_icon: Option<String>,
    pub temperature_max: Option<f32>,
    pub temperature_min: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirQuality {
    pub aqi_display: String,
    pub category: String,
    pub color: Option<[u8; 3]>,
    pub primary_pollutant: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WeatherState {
    pub requested_location: String,
    pub resolved_location: Option<String>,
    pub weather_code: Option<u16>,
    pub qweather_icon: Option<String>,
    pub temperature_max: Option<f32>,
    pub temperature_min: Option<f32>,
    pub scene_index: Option<usize>,
    pub days: Vec<WeatherDay>,
    pub air_quality: Option<AirQuality>,
    pub updated_at: Option<String>,
    pub error: Option<String>,
    pub source: Option<WeatherDataSource>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
    admin1: Option<String>,
    admin2: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForecastResponse {
    daily: DailyForecast,
}

#[derive(Debug, Deserialize)]
struct DailyForecast {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    weather_code: Vec<u16>,
    #[serde(default)]
    temperature_2m_max: Vec<f32>,
    #[serde(default)]
    temperature_2m_min: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct QWeatherLocationResponse {
    code: String,
    #[serde(default)]
    location: Vec<QWeatherLocation>,
}

#[derive(Debug, Deserialize)]
struct QWeatherLocation {
    name: String,
    lat: String,
    lon: String,
    #[serde(default)]
    adm1: String,
    #[serde(default)]
    adm2: String,
    #[serde(default)]
    country: String,
}

#[derive(Debug, Deserialize)]
struct QWeatherForecastResponse {
    code: String,
    #[serde(default)]
    daily: Vec<QWeatherDaily>,
}

#[derive(Debug, Deserialize)]
struct QWeatherAirQualityResponse {
    #[serde(default)]
    indexes: Vec<QWeatherAirQualityIndex>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QWeatherAirQualityIndex {
    code: String,
    aqi_display: String,
    category: String,
    color: Option<QWeatherAirQualityColor>,
    primary_pollutant: Option<QWeatherPrimaryPollutant>,
}

#[derive(Debug, Deserialize)]
struct QWeatherAirQualityColor {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Debug, Deserialize)]
struct QWeatherPrimaryPollutant {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QWeatherDaily {
    fx_date: String,
    temp_max: String,
    temp_min: String,
    icon_day: String,
}

pub fn search_locations_with_services(
    location: &str,
    language: Language,
    services: &WeatherServices,
) -> anyhow::Result<Vec<LocationCandidate>> {
    let location = location.trim();
    if location.chars().count() < 2 {
        return Err(anyhow!(message(
            language,
            "请至少输入两个字符的城市或地区",
            "Enter at least two characters for a city or region"
        )));
    }
    let client = weather_client()?;
    let candidates = match services.provider {
        WeatherProvider::OpenMeteo => search_open_meteo_variants(&client, location, language)?,
        WeatherProvider::QWeather => {
            let (host, key) = services.qweather_credentials(language)?;
            search_qweather(&client, host, key, location, language)?
        }
        WeatherProvider::Auto => {
            let qweather = if services.has_qweather_credentials() {
                let (host, key) = services.qweather_credentials(language)?;
                search_qweather(&client, host, key, location, language).unwrap_or_default()
            } else {
                Vec::new()
            };
            if qweather.is_empty() {
                search_open_meteo_variants(&client, location, language)?
            } else {
                qweather
            }
        }
    };
    if candidates.is_empty() {
        Err(anyhow!(message(
            language,
            "没有找到匹配地区，请尝试输入上级城市",
            "No matching location; try a nearby city"
        )))
    } else {
        Ok(candidates)
    }
}

pub fn fetch_coordinates_with_services(
    requested_location: &str,
    resolved_location: &str,
    latitude: f64,
    longitude: f64,
    language: Language,
    services: &WeatherServices,
) -> WeatherState {
    let result = match services.provider {
        WeatherProvider::OpenMeteo => fetch_open_meteo(
            requested_location,
            resolved_location.to_owned(),
            latitude,
            longitude,
            language,
        ),
        WeatherProvider::QWeather => services
            .qweather_credentials(language)
            .and_then(|(host, key)| {
                fetch_qweather(
                    requested_location,
                    resolved_location.to_owned(),
                    latitude,
                    longitude,
                    language,
                    host,
                    key,
                )
            })
            .map(|state| merge_open_meteo_history(state, latitude, longitude, language)),
        WeatherProvider::Auto => {
            let qweather = if services.has_qweather_credentials() {
                services
                    .qweather_credentials(language)
                    .and_then(|(host, key)| {
                        fetch_qweather(
                            requested_location,
                            resolved_location.to_owned(),
                            latitude,
                            longitude,
                            language,
                            host,
                            key,
                        )
                    })
                    .ok()
            } else {
                None
            };
            match qweather {
                Some(state) => Ok(merge_open_meteo_history(
                    state, latitude, longitude, language,
                )),
                None => fetch_open_meteo(
                    requested_location,
                    resolved_location.to_owned(),
                    latitude,
                    longitude,
                    language,
                ),
            }
        }
    };
    weather_state_from_result(requested_location, result)
}

fn weather_state_from_result(location: &str, result: anyhow::Result<WeatherState>) -> WeatherState {
    match result {
        Ok(state) => state,
        Err(error) => WeatherState {
            requested_location: location.to_owned(),
            error: Some(error.to_string()),
            updated_at: Some(Utc::now().to_rfc3339()),
            ..WeatherState::default()
        },
    }
}

fn search_open_meteo_variants(
    client: &reqwest::blocking::Client,
    location: &str,
    language: Language,
) -> anyhow::Result<Vec<LocationCandidate>> {
    for query in location_query_variants(location) {
        let candidates = search_open_meteo(client, &query, language)?;
        if !candidates.is_empty() {
            return Ok(candidates);
        }
    }
    Ok(Vec::new())
}

fn search_open_meteo(
    client: &reqwest::blocking::Client,
    query: &str,
    language: Language,
) -> anyhow::Result<Vec<LocationCandidate>> {
    let api_language = match language {
        Language::ZhCn => "zh",
        Language::EnUs => "en",
    };
    let geocoding: GeocodingResponse = client
        .get(OPEN_METEO_GEOCODING)
        .query(&[
            ("name", query),
            ("count", "10"),
            ("language", api_language),
            ("format", "json"),
        ])
        .send()
        .with_context(|| message(language, "查询地址", "Query location"))?
        .error_for_status()
        .with_context(|| message(language, "地址服务返回错误", "Location service error"))?
        .json()
        .with_context(|| message(language, "解析地址结果", "Parse location result"))?;
    Ok(geocoding
        .results
        .into_iter()
        .map(|result| LocationCandidate {
            name: result.name,
            full_name: None,
            admin1: result.admin1,
            admin2: result.admin2,
            country: result.country,
            latitude: result.latitude,
            longitude: result.longitude,
        })
        .collect())
}

fn search_qweather(
    client: &reqwest::blocking::Client,
    host: &str,
    key: &str,
    query: &str,
    language: Language,
) -> anyhow::Result<Vec<LocationCandidate>> {
    let response: QWeatherLocationResponse = client
        .get(format!(
            "{}/geo/v2/city/lookup",
            qweather_base_url(host, language)?
        ))
        .header("X-QW-Api-Key", key)
        .query(&[
            ("location", query),
            ("number", "10"),
            (
                "lang",
                match language {
                    Language::ZhCn => "zh",
                    Language::EnUs => "en",
                },
            ),
        ])
        .send()
        .with_context(|| message(language, "查询和风天气地址", "Query QWeather location"))?
        .error_for_status()
        .with_context(|| message(language, "和风地址服务返回错误", "QWeather location error"))?
        .json()
        .with_context(|| message(language, "解析和风地址结果", "Parse QWeather location"))?;
    ensure_qweather_success(&response.code, language)?;
    response
        .location
        .into_iter()
        .map(|location| {
            Ok(LocationCandidate {
                name: location.name,
                full_name: None,
                admin1: non_empty(location.adm1),
                admin2: non_empty(location.adm2),
                country: non_empty(location.country),
                latitude: location
                    .lat
                    .parse()
                    .with_context(|| message(language, "解析地区纬度", "Parse latitude"))?,
                longitude: location
                    .lon
                    .parse()
                    .with_context(|| message(language, "解析地区经度", "Parse longitude"))?,
            })
        })
        .collect()
}

fn weather_client() -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("codex-calendar/1 (+https://github.com/adoin/Codex-weather-widget)")
        .build()?)
}

fn fetch_open_meteo(
    requested_location: &str,
    resolved_location: String,
    latitude: f64,
    longitude: f64,
    language: Language,
) -> anyhow::Result<WeatherState> {
    let forecast: ForecastResponse = weather_client()?
        .get(OPEN_METEO_FORECAST)
        .query(&[
            ("latitude", latitude.to_string()),
            ("longitude", longitude.to_string()),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min".into(),
            ),
            ("timezone", "auto".into()),
            ("forecast_days", "7".into()),
            ("past_days", PAST_DAYS.to_string()),
        ])
        .send()
        .with_context(|| message(language, "获取天气预报", "Fetch weather forecast"))?
        .error_for_status()
        .with_context(|| message(language, "天气服务返回错误", "Weather service error"))?
        .json()
        .with_context(|| message(language, "解析天气预报", "Parse weather forecast"))?;
    weather_state_for_today(
        requested_location,
        resolved_location,
        open_meteo_days(&forecast),
        WeatherDataSource::OpenMeteo,
        language,
    )
}

fn fetch_qweather(
    requested_location: &str,
    resolved_location: String,
    latitude: f64,
    longitude: f64,
    language: Language,
    host: &str,
    key: &str,
) -> anyhow::Result<WeatherState> {
    let client = weather_client()?;
    let response: QWeatherForecastResponse = client
        .get(format!(
            "{}/v7/weather/7d",
            qweather_base_url(host, language)?
        ))
        .header("X-QW-Api-Key", key)
        .query(&[("location", format!("{longitude:.4},{latitude:.4}"))])
        .send()
        .with_context(|| message(language, "获取和风天气预报", "Fetch QWeather forecast"))?
        .error_for_status()
        .with_context(|| message(language, "和风天气服务返回错误", "QWeather service error"))?
        .json()
        .with_context(|| message(language, "解析和风天气预报", "Parse QWeather forecast"))?;
    ensure_qweather_success(&response.code, language)?;
    let days = response
        .daily
        .into_iter()
        .map(|day| {
            Ok(WeatherDay {
                date: day.fx_date,
                weather_code: qweather_icon_to_wmo(&day.icon_day),
                qweather_icon: Some(day.icon_day),
                temperature_max: Some(day.temp_max.parse().with_context(|| {
                    message(language, "解析最高温度", "Parse maximum temperature")
                })?),
                temperature_min: Some(day.temp_min.parse().with_context(|| {
                    message(language, "解析最低温度", "Parse minimum temperature")
                })?),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut state = weather_state_for_today(
        requested_location,
        resolved_location,
        days,
        WeatherDataSource::QWeather,
        language,
    )?;
    state.air_quality =
        fetch_qweather_air_quality(&client, latitude, longitude, language, host, key).ok();
    Ok(state)
}

fn fetch_qweather_air_quality(
    client: &reqwest::blocking::Client,
    latitude: f64,
    longitude: f64,
    language: Language,
    host: &str,
    key: &str,
) -> anyhow::Result<AirQuality> {
    let response: QWeatherAirQualityResponse = client
        .get(format!(
            "{}/airquality/v1/current/{latitude:.2}/{longitude:.2}",
            qweather_base_url(host, language)?
        ))
        .header("X-QW-Api-Key", key)
        .query(&[(
            "lang",
            match language {
                Language::ZhCn => "zh",
                Language::EnUs => "en",
            },
        )])
        .send()
        .with_context(|| message(language, "获取实时空气质量", "Fetch current air quality"))?
        .error_for_status()
        .with_context(|| {
            message(
                language,
                "空气质量服务返回错误",
                "Air quality service error",
            )
        })?
        .json()
        .with_context(|| message(language, "解析实时空气质量", "Parse current air quality"))?;
    select_qweather_air_quality(response.indexes).ok_or_else(|| {
        anyhow!(message(
            language,
            "空气质量服务没有返回可用指数",
            "Air quality service returned no usable index"
        ))
    })
}

fn select_qweather_air_quality(indexes: Vec<QWeatherAirQualityIndex>) -> Option<AirQuality> {
    let selected = indexes
        .iter()
        .position(|index| index.code == "cn-mee")
        .or_else(|| {
            indexes
                .iter()
                .position(|index| index.code != "qaqi" && !index.code.ends_with("-1h"))
        })
        .or_else(|| indexes.iter().position(|index| index.code != "qaqi"))
        .or_else(|| (!indexes.is_empty()).then_some(0))?;
    let index = indexes.into_iter().nth(selected)?;
    Some(AirQuality {
        aqi_display: index.aqi_display,
        category: index.category,
        color: index
            .color
            .map(|color| [color.red, color.green, color.blue]),
        primary_pollutant: index.primary_pollutant.map(|pollutant| pollutant.name),
    })
}

fn weather_state_for_today(
    requested_location: &str,
    resolved_location: String,
    days: Vec<WeatherDay>,
    source: WeatherDataSource,
    language: Language,
) -> anyhow::Result<WeatherState> {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let current = days
        .iter()
        .find(|day| day.date == today)
        .or_else(|| days.first())
        .ok_or_else(|| {
            anyhow!(message(
                language,
                "天气预报没有今日数据",
                "Forecast has no data for today"
            ))
        })?;
    Ok(WeatherState {
        requested_location: requested_location.to_owned(),
        resolved_location: Some(resolved_location),
        weather_code: Some(current.weather_code),
        qweather_icon: current.qweather_icon.clone(),
        temperature_max: current.temperature_max,
        temperature_min: current.temperature_min,
        scene_index: Some(scene_for_weather_code(current.weather_code)),
        days,
        air_quality: None,
        updated_at: Some(Utc::now().to_rfc3339()),
        error: None,
        source: Some(source),
    })
}

fn open_meteo_days(forecast: &ForecastResponse) -> Vec<WeatherDay> {
    forecast
        .daily
        .weather_code
        .iter()
        .enumerate()
        .filter_map(|(index, weather_code)| {
            Some(WeatherDay {
                date: forecast.daily.time.get(index)?.clone(),
                weather_code: *weather_code,
                qweather_icon: None,
                temperature_max: forecast.daily.temperature_2m_max.get(index).copied(),
                temperature_min: forecast.daily.temperature_2m_min.get(index).copied(),
            })
        })
        .collect()
}

fn merge_open_meteo_history(
    mut primary: WeatherState,
    latitude: f64,
    longitude: f64,
    language: Language,
) -> WeatherState {
    let Ok(history) = fetch_open_meteo(
        &primary.requested_location,
        primary.resolved_location.clone().unwrap_or_default(),
        latitude,
        longitude,
        language,
    ) else {
        return primary;
    };
    let mut known: HashSet<String> = primary.days.iter().map(|day| day.date.clone()).collect();
    for day in history.days {
        if known.insert(day.date.clone()) {
            primary.days.push(day);
        }
    }
    primary
        .days
        .sort_by(|left, right| left.date.cmp(&right.date));
    primary
}

fn qweather_base_url(host: &str, language: Language) -> anyhow::Result<String> {
    let trimmed = host.trim().trim_end_matches('/');
    let candidate = if trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else if trimmed.contains("://") {
        return Err(anyhow!(message(
            language,
            "和风 API Host 必须使用 HTTPS",
            "QWeather API Host must use HTTPS"
        )));
    } else {
        format!("https://{trimmed}")
    };
    let url = reqwest::Url::parse(&candidate).with_context(|| {
        message(
            language,
            "和风 API Host 格式不正确",
            "Invalid QWeather API Host",
        )
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || (url.path() != "" && url.path() != "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!(message(
            language,
            "和风 API Host 只填写专属域名，不要包含接口路径",
            "Enter only the QWeather API host, without an API path"
        )));
    }
    Ok(candidate)
}

fn ensure_qweather_success(code: &str, language: Language) -> anyhow::Result<()> {
    if code == "200" {
        Ok(())
    } else {
        Err(anyhow!(format!(
            "{} ({code})",
            message(
                language,
                "和风天气请求失败，请检查 Host、Key 和订阅状态",
                "QWeather request failed; check the Host, key and subscription"
            )
        )))
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn location_query_variants(location: &str) -> Vec<String> {
    let mut variants = vec![location.to_owned()];
    let after_province = location
        .split_once('省')
        .map_or(location, |(_, remainder)| remainder);
    if let Some(city_end) = after_province.find('市') {
        let city = after_province[..city_end].trim();
        if city.chars().count() >= 2 && !variants.iter().any(|item| item == city) {
            variants.push(city.to_owned());
        }
    }
    variants
}

fn qweather_icon_to_wmo(icon: &str) -> u16 {
    match icon.parse::<u16>().unwrap_or_default() {
        100 | 150 => 0,
        101..=103 | 151..=153 => 2,
        104 | 154 => 3,
        302..=304 => 95,
        300..=399 => 63,
        400..=499 => 73,
        500..=515 => 45,
        _ => 2,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeatherEffects {
    pub rain: u8,
    pub snow: u8,
    pub lightning: bool,
}

pub fn weather_effects(qweather_icon: Option<&str>, wmo_code: Option<u16>) -> WeatherEffects {
    if let Some(code) = qweather_icon.and_then(|icon| icon.parse::<u16>().ok()) {
        return WeatherEffects {
            rain: match code {
                305 | 309 | 314 | 404 | 405 => 1,
                300 | 302 | 304 | 306 | 313 | 315 | 350 | 399 | 406 | 456 => 2,
                301 | 303 | 307 | 308 | 310 | 311 | 312 | 316 | 317 | 318 | 351 => 3,
                _ => 0,
            },
            snow: match code {
                400 | 404 | 405 | 408 => 1,
                401 | 406 | 407 | 409 | 456 | 457 | 499 => 2,
                402 | 403 | 410 => 3,
                _ => 0,
            },
            lightning: matches!(code, 302..=304),
        };
    }
    let code = wmo_code.unwrap_or_default();
    WeatherEffects {
        rain: match code {
            51 | 53 | 56 | 61 | 66 | 80 => 1,
            55 | 57 | 63 | 67 | 81 | 95 | 96 => 2,
            65 | 82 | 99 => 3,
            _ => 0,
        },
        snow: match code {
            71 | 77 | 85 => 1,
            73 => 2,
            75 | 86 => 3,
            _ => 0,
        },
        lightning: matches!(code, 95 | 96 | 99),
    }
}

pub fn scene_for_weather_code(code: u16) -> usize {
    match code {
        51..=67 | 80..=82 => 0,
        95..=99 => 1,
        71..=77 | 85 | 86 => 2,
        0 => 3,
        1 | 2 => 4,
        3 => 5,
        45 | 48 => 6,
        _ => 4,
    }
}

pub fn condition_label(code: Option<u16>, language: Language) -> &'static str {
    match (language, code) {
        (Language::ZhCn, Some(0)) => "晴",
        (Language::ZhCn, Some(1 | 2)) => "多云",
        (Language::ZhCn, Some(3)) => "阴",
        (Language::ZhCn, Some(45 | 48)) => "雾",
        (Language::ZhCn, Some(51..=67 | 80..=82)) => "雨",
        (Language::ZhCn, Some(71..=77 | 85 | 86)) => "雪",
        (Language::ZhCn, Some(95..=99)) => "雷暴",
        (Language::EnUs, Some(0)) => "Clear",
        (Language::EnUs, Some(1 | 2)) => "Cloudy",
        (Language::EnUs, Some(3)) => "Overcast",
        (Language::EnUs, Some(45 | 48)) => "Fog",
        (Language::EnUs, Some(51..=67 | 80..=82)) => "Rain",
        (Language::EnUs, Some(71..=77 | 85 | 86)) => "Snow",
        (Language::EnUs, Some(95..=99)) => "Thunderstorm",
        (Language::ZhCn, _) => "天气",
        (Language::EnUs, _) => "Weather",
    }
}

const fn message(language: Language, chinese: &'static str, english: &'static str) -> &'static str {
    match language {
        Language::ZhCn => chinese,
        Language::EnUs => english,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wmo_codes_to_expected_scenes() {
        assert_eq!(scene_for_weather_code(0), 3);
        assert_eq!(scene_for_weather_code(2), 4);
        assert_eq!(scene_for_weather_code(3), 5);
        assert_eq!(scene_for_weather_code(45), 6);
        assert_eq!(scene_for_weather_code(63), 0);
        assert_eq!(scene_for_weather_code(73), 2);
        assert_eq!(scene_for_weather_code(95), 1);
    }

    #[test]
    fn maps_qweather_icons_to_existing_weather_categories() {
        assert_eq!(qweather_icon_to_wmo("100"), 0);
        assert_eq!(qweather_icon_to_wmo("104"), 3);
        assert_eq!(qweather_icon_to_wmo("305"), 63);
        assert_eq!(qweather_icon_to_wmo("303"), 95);
        assert_eq!(qweather_icon_to_wmo("401"), 73);
        assert_eq!(qweather_icon_to_wmo("501"), 45);
    }

    #[test]
    fn maps_weather_effects_without_treating_every_precipitation_as_rain() {
        assert_eq!(
            weather_effects(Some("303"), None),
            WeatherEffects {
                rain: 3,
                snow: 0,
                lightning: true,
            }
        );
        assert_eq!(
            weather_effects(Some("404"), None),
            WeatherEffects {
                rain: 1,
                snow: 1,
                lightning: false,
            }
        );
        assert_eq!(weather_effects(Some("403"), None).snow, 3);
        assert_eq!(
            weather_effects(Some("501"), None),
            WeatherEffects::default()
        );
        assert!(weather_effects(None, Some(99)).lightning);
    }

    #[test]
    fn selects_local_daily_air_quality_index_and_preserves_display_data() {
        let response: QWeatherAirQualityResponse = serde_json::from_str(
            r#"{
                "indexes": [
                    {"code":"qaqi","aqiDisplay":"1.2","category":"Excellent"},
                    {"code":"cn-mee-1h","aqiDisplay":"31","category":"优"},
                    {
                        "code":"cn-mee",
                        "aqiDisplay":"42",
                        "category":"优",
                        "color":{"red":0,"green":228,"blue":0,"alpha":1},
                        "primaryPollutant":{"code":"pm2p5","name":"PM2.5","fullName":"细颗粒物"}
                    }
                ]
            }"#,
        )
        .unwrap();

        let air_quality = select_qweather_air_quality(response.indexes).unwrap();
        assert_eq!(air_quality.aqi_display, "42");
        assert_eq!(air_quality.category, "优");
        assert_eq!(air_quality.color, Some([0, 228, 0]));
        assert_eq!(air_quality.primary_pollutant.as_deref(), Some("PM2.5"));
    }

    #[test]
    fn validates_qweather_hosts() {
        assert_eq!(
            qweather_base_url("abc.qweatherapi.com/", Language::EnUs).unwrap(),
            "https://abc.qweatherapi.com"
        );
        assert!(qweather_base_url("http://example.com", Language::EnUs).is_err());
        assert!(qweather_base_url("example.com/v7", Language::EnUs).is_err());
    }

    #[test]
    fn falls_back_from_chinese_district_to_parent_city() {
        assert_eq!(
            location_query_variants("福建省福州市晋安区"),
            vec!["福建省福州市晋安区", "福州"]
        );
        assert_eq!(
            location_query_variants("上海市浦东新区"),
            vec!["上海市浦东新区", "上海"]
        );
    }
}
