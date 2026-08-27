use std::time::Duration;

use anyhow::{Context, anyhow};
use chrono::Utc;
use serde::Deserialize;

use crate::i18n::Language;

const PAST_DAYS: usize = 6;

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
    pub temperature_max: Option<f32>,
    pub temperature_min: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct WeatherState {
    pub requested_location: String,
    pub resolved_location: Option<String>,
    pub weather_code: Option<u16>,
    pub temperature_max: Option<f32>,
    pub temperature_min: Option<f32>,
    pub scene_index: Option<usize>,
    pub days: Vec<WeatherDay>,
    pub updated_at: Option<String>,
    pub error: Option<String>,
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
struct NominatimResult {
    display_name: String,
    lat: String,
    lon: String,
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

pub fn fetch(location: &str, language: Language) -> WeatherState {
    let result = search_locations(location, language).and_then(|candidates| {
        let candidate = candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!(message(language, "没有找到这个地址", "Location not found")))?;
        fetch_coordinates_inner(
            location,
            candidate.display_name(),
            candidate.latitude,
            candidate.longitude,
            language,
        )
    });
    weather_state_from_result(location, result)
}

pub fn fetch_coordinates(
    requested_location: &str,
    resolved_location: &str,
    latitude: f64,
    longitude: f64,
    language: Language,
) -> WeatherState {
    weather_state_from_result(
        requested_location,
        fetch_coordinates_inner(
            requested_location,
            resolved_location.to_owned(),
            latitude,
            longitude,
            language,
        ),
    )
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

pub fn search_locations(
    location: &str,
    language: Language,
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
    let variants = location_query_variants(location);
    let mut candidates = search_open_meteo(&client, location, language)?;
    if candidates.is_empty() {
        candidates = search_nominatim(&client, location, language)?;
    }
    if candidates.is_empty() {
        for query in variants.iter().skip(1) {
            candidates = search_open_meteo(&client, query, language)?;
            if !candidates.is_empty() {
                break;
            }
        }
    }
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
        .get("https://geocoding-api.open-meteo.com/v1/search")
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

fn search_nominatim(
    client: &reqwest::blocking::Client,
    query: &str,
    language: Language,
) -> anyhow::Result<Vec<LocationCandidate>> {
    let accept_language = match language {
        Language::ZhCn => "zh-CN,zh;q=0.9",
        Language::EnUs => "en-US,en;q=0.9",
    };
    let results: Vec<NominatimResult> = client
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[
            ("q", query),
            ("format", "jsonv2"),
            ("addressdetails", "1"),
            ("limit", "8"),
        ])
        .header("Accept-Language", accept_language)
        .send()
        .with_context(|| message(language, "查询区县地址", "Query district location"))?
        .error_for_status()
        .with_context(|| {
            message(
                language,
                "区县地址服务返回错误",
                "District location service error",
            )
        })?
        .json()
        .with_context(|| {
            message(
                language,
                "解析区县地址结果",
                "Parse district location result",
            )
        })?;
    results
        .into_iter()
        .map(|result| {
            Ok(LocationCandidate {
                name: result
                    .display_name
                    .split(',')
                    .next()
                    .unwrap_or(message(language, "未知地区", "Unknown location"))
                    .trim()
                    .to_owned(),
                full_name: Some(result.display_name),
                admin1: None,
                admin2: None,
                country: None,
                latitude: result
                    .lat
                    .parse()
                    .with_context(|| message(language, "解析地区纬度", "Parse latitude"))?,
                longitude: result
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
        .user_agent("codex-calendar/0.1 (+https://github.com/adoin/Codex-weather-widget)")
        .build()?)
}

fn fetch_coordinates_inner(
    requested_location: &str,
    resolved_location: String,
    latitude: f64,
    longitude: f64,
    language: Language,
) -> anyhow::Result<WeatherState> {
    let forecast: ForecastResponse = weather_client()?
        .get("https://api.open-meteo.com/v1/forecast")
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
    let code = forecast
        .daily
        .weather_code
        .get(PAST_DAYS)
        .copied()
        .ok_or_else(|| {
            anyhow!(message(
                language,
                "天气预报没有今日数据",
                "Forecast has no data for today"
            ))
        })?;
    let days = forecast
        .daily
        .weather_code
        .iter()
        .enumerate()
        .filter_map(|(index, weather_code)| {
            Some(WeatherDay {
                date: forecast.daily.time.get(index)?.clone(),
                weather_code: *weather_code,
                temperature_max: forecast.daily.temperature_2m_max.get(index).copied(),
                temperature_min: forecast.daily.temperature_2m_min.get(index).copied(),
            })
        })
        .collect();
    Ok(WeatherState {
        requested_location: requested_location.to_owned(),
        resolved_location: Some(resolved_location),
        weather_code: Some(code),
        temperature_max: forecast.daily.temperature_2m_max.get(PAST_DAYS).copied(),
        temperature_min: forecast.daily.temperature_2m_min.get(PAST_DAYS).copied(),
        scene_index: Some(scene_for_weather_code(code)),
        days,
        updated_at: Some(Utc::now().to_rfc3339()),
        error: None,
    })
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

/// Map WMO weather interpretation codes to the seven bundled visual themes:
/// rain, thunderstorm, snow, clear, cloudy, overcast and fog.
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

    #[test]
    fn today_follows_the_six_requested_past_days() {
        let returned_days: Vec<u16> = (0..13).collect();
        assert_eq!(returned_days.get(PAST_DAYS), Some(&6));
        assert_eq!(returned_days[..PAST_DAYS].len(), 6);
        assert_eq!(returned_days[PAST_DAYS..].len(), 7);
    }
}
