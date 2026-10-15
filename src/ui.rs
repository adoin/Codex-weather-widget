use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use chrono::{Datelike, Local, NaiveDate, Utc};
use eframe::egui::epaint::{RectShape, Shadow};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2,
    Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextureHandle, Vec2, ViewportCommand,
    WindowLevel,
};
use parking_lot::RwLock;

use crate::{
    config::{Config, WeatherIconTheme, WeatherProvider},
    credentials,
    i18n::{Language, Text, month_name, tr, weekday_name, weekday_short},
    model::{AppCommand, DailyUsage, RateWindow, UsageSnapshot},
    weather::{self, LocationCandidate, WeatherState, condition_label},
};

type LocationSearchResult = Result<Vec<LocationCandidate>, String>;
type DesktopGlassResult = Result<Option<image::RgbaImage>, String>;

struct LocationSearchRequest {
    query: String,
    language: Language,
    services: weather::WeatherServices,
}

const INK: Color32 = Color32::from_rgb(39, 48, 43);
const PAPER: Color32 = Color32::from_rgb(246, 243, 234);
const TERRACOTTA: Color32 = Color32::from_rgb(184, 101, 72);

const RAIN_SCENE: usize = 0;
const THUNDER_SCENE: usize = 1;
const SNOW_SCENE: usize = 2;
const CLEAR_SCENE: usize = 3;
const CLOUDY_SCENE: usize = 4;
const OVERCAST_SCENE: usize = 5;
const FOG_SCENE: usize = 6;
const WINDOW_CORNER_RADIUS: u8 = 24;

const BACKGROUND_BYTES: [[&[u8]; 3]; 7] = [
    [
        include_bytes!("../assets/backgrounds/rain-0.jpg"),
        include_bytes!("../assets/backgrounds/rain-1.jpg"),
        include_bytes!("../assets/backgrounds/rain-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/rain-0.jpg"),
        include_bytes!("../assets/backgrounds/rain-1.jpg"),
        include_bytes!("../assets/backgrounds/rain-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/snow-0.jpg"),
        include_bytes!("../assets/backgrounds/snow-1.jpg"),
        include_bytes!("../assets/backgrounds/snow-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/clear-0.jpg"),
        include_bytes!("../assets/backgrounds/clear-1.jpg"),
        include_bytes!("../assets/backgrounds/clear-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/cloudy-0.jpg"),
        include_bytes!("../assets/backgrounds/cloudy-1.jpg"),
        include_bytes!("../assets/backgrounds/cloudy-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/overcast-0.jpg"),
        include_bytes!("../assets/backgrounds/overcast-1.jpg"),
        include_bytes!("../assets/backgrounds/overcast-2.jpg"),
    ],
    [
        include_bytes!("../assets/backgrounds/fog-0.jpg"),
        include_bytes!("../assets/backgrounds/fog-1.jpg"),
        include_bytes!("../assets/backgrounds/fog-2.jpg"),
    ],
];

#[derive(Clone, Copy)]
struct Scene {
    sky: Color32,
}

#[derive(Clone, Copy, Debug)]
struct Hsl {
    hue: f32,
    saturation: f32,
    lightness: f32,
}

#[derive(Clone, Copy, Debug)]
struct TextPalette {
    photo_primary: Color32,
    photo_muted: Color32,
    photo_accent: Color32,
    card_primary: Color32,
    card_muted: Color32,
    card_accent: Color32,
    glass: GlassPalette,
}

#[derive(Clone, Copy, Debug)]
struct GlassPalette {
    tint: Color32,
    tint_alpha: u8,
    composite: Color32,
}

#[derive(Clone, Copy)]
struct FrostedContext<'a> {
    texture: Option<&'a TextureHandle>,
    scene_rect: Rect,
    texture_opacity: f32,
}

#[derive(Clone, Copy)]
enum TextSurface {
    Photo,
    Glass,
}

const SCENES: [Scene; 7] = [
    Scene {
        sky: Color32::from_rgb(194, 211, 218),
    },
    Scene {
        sky: Color32::from_rgb(80, 97, 112),
    },
    Scene {
        sky: Color32::from_rgb(219, 226, 227),
    },
    Scene {
        sky: Color32::from_rgb(166, 218, 239),
    },
    Scene {
        sky: Color32::from_rgb(183, 205, 216),
    },
    Scene {
        sky: Color32::from_rgb(126, 142, 151),
    },
    Scene {
        sky: Color32::from_rgb(190, 198, 195),
    },
];

pub fn install_fonts(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    let candidates = if cfg!(target_os = "windows") {
        vec![
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\segoeui.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/SFNS.ttf",
        ]
    } else {
        vec![
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
    };
    for path in candidates {
        if let Ok(bytes) = std::fs::read(Path::new(path)) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "calendar-ui".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "calendar-ui".into());
            if cfg!(target_os = "windows")
                && let Ok(symbols) = std::fs::read(r"C:\Windows\Fonts\seguisym.ttf")
            {
                fonts.font_data.insert(
                    "weather-symbols".into(),
                    egui::FontData::from_owned(symbols).into(),
                );
                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .push("weather-symbols".into());
            }
            ctx.set_fonts(fonts);
            break;
        }
    }
}

fn should_show_text_hint(value: &str, has_focus: bool) -> bool {
    value.is_empty() && !has_focus
}

fn should_show_qweather_help(provider: WeatherProvider) -> bool {
    provider != WeatherProvider::OpenMeteo
}

fn qweather_help_url(language: Language) -> &'static str {
    match language {
        Language::ZhCn => "https://dev.qweather.com/docs/configuration/project-and-key/",
        Language::EnUs => "https://dev.qweather.com/en/docs/configuration/project-and-key/",
    }
}

pub struct CalendarApp {
    config: Arc<RwLock<Config>>,
    snapshot: Arc<RwLock<UsageSnapshot>>,
    weather: Arc<RwLock<WeatherState>>,
    commands: Receiver<AppCommand>,
    visible: Arc<AtomicBool>,
    settings_open: bool,
    stats_open: bool,
    language_popup_requested: bool,
    quitting: bool,
    initial_show_pending: bool,
    startup_warmup_frames: u8,
    backgrounds: Vec<Vec<Option<TextureHandle>>>,
    frosted_backgrounds: Vec<Vec<Option<TextureHandle>>>,
    background_palettes: Vec<Vec<Option<TextPalette>>>,
    background_dominants: Vec<Vec<Option<Color32>>>,
    desktop_glass_rx: Receiver<DesktopGlassResult>,
    desktop_glass_texture: Option<TextureHandle>,
    desktop_glass_dominant: Option<Color32>,
    desktop_glass_loading: bool,
    location_draft: String,
    qweather_api_key_draft: String,
    qweather_api_key_saved: bool,
    weather_credentials_status: Option<String>,
    location_search_tx: Sender<LocationSearchRequest>,
    location_search_rx: Receiver<(String, LocationSearchResult)>,
    location_search_pending: Option<String>,
    location_candidates: Vec<LocationCandidate>,
    location_search_error: Option<String>,
    autostart_error: Option<String>,
    native_attached_state: Option<bool>,
    native_region_size: Option<(u32, u32)>,
    native_drag_origin: Option<crate::desktop::NativeDragOrigin>,
    last_native_position: Option<(i32, i32)>,
    position_save_due: Option<Instant>,
    diagnostic_state: String,
    diagnostic_heartbeat: Instant,
    diagnostic_attach_failure: Option<Instant>,
}

impl CalendarApp {
    pub fn new(
        config: Arc<RwLock<Config>>,
        snapshot: Arc<RwLock<UsageSnapshot>>,
        weather: Arc<RwLock<WeatherState>>,
        commands: Receiver<AppCommand>,
        visible: Arc<AtomicBool>,
    ) -> Self {
        let location_draft = config.read().weather_location.clone();
        let qweather_api_key_saved = credentials::load_qweather_api_key()
            .ok()
            .flatten()
            .is_some_and(|key| !key.trim().is_empty());
        let last_native_position = {
            let cfg = config.read();
            cfg.window_x
                .zip(cfg.window_y)
                .map(|(x, y)| (x.round() as i32, y.round() as i32))
        };
        let (location_search_tx, location_search_requests) =
            mpsc::channel::<LocationSearchRequest>();
        let (location_search_results, location_search_rx) =
            mpsc::channel::<(String, LocationSearchResult)>();
        let (desktop_glass_tx, desktop_glass_rx) = mpsc::channel::<DesktopGlassResult>();
        thread::Builder::new()
            .name("location-search".into())
            .spawn(move || {
                while let Ok(request) = location_search_requests.recv() {
                    let query = request.query;
                    let result = weather::search_locations_with_services(
                        &query,
                        request.language,
                        &request.services,
                    )
                    .map_err(|error| error.to_string());
                    if location_search_results.send((query, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn location search worker");
        let desktop_glass_loading = cfg!(target_os = "windows");
        thread::Builder::new()
            .name("widget-wallpaper-blur".into())
            .spawn(move || {
                let result = load_desktop_glass_image().map_err(|error| error.to_string());
                let _ = desktop_glass_tx.send(result);
            })
            .expect("spawn desktop glass worker");
        Self {
            config,
            snapshot,
            weather,
            commands,
            visible,
            settings_open: false,
            stats_open: false,
            language_popup_requested: false,
            quitting: false,
            initial_show_pending: true,
            startup_warmup_frames: 2,
            backgrounds: (0..SCENES.len())
                .map(|_| (0..3).map(|_| None).collect())
                .collect(),
            frosted_backgrounds: (0..SCENES.len())
                .map(|_| (0..3).map(|_| None).collect())
                .collect(),
            background_palettes: (0..SCENES.len())
                .map(|_| (0..3).map(|_| None).collect())
                .collect(),
            background_dominants: (0..SCENES.len())
                .map(|_| (0..3).map(|_| None).collect())
                .collect(),
            desktop_glass_rx,
            desktop_glass_texture: None,
            desktop_glass_dominant: None,
            desktop_glass_loading,
            location_draft,
            qweather_api_key_draft: String::new(),
            qweather_api_key_saved,
            weather_credentials_status: None,
            location_search_tx,
            location_search_rx,
            location_search_pending: None,
            location_candidates: Vec::new(),
            location_search_error: None,
            autostart_error: None,
            native_attached_state: None,
            native_region_size: None,
            native_drag_origin: None,
            last_native_position,
            position_save_due: None,
            diagnostic_state: String::new(),
            diagnostic_heartbeat: Instant::now(),
            diagnostic_attach_failure: None,
        }
    }

    fn handle_commands(&mut self, ctx: &egui::Context) {
        while let Ok(command) = self.commands.try_recv() {
            crate::diagnostics::event(format!("app_command {command:?}"));
            match command {
                AppCommand::Show => {
                    self.visible.store(true, Ordering::Relaxed);
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                AppCommand::ShowLanguageMenu => {
                    self.visible.store(true, Ordering::Relaxed);
                    self.settings_open = true;
                    self.stats_open = false;
                    self.language_popup_requested = true;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                AppCommand::ShowStatistics => {
                    self.visible.store(true, Ordering::Relaxed);
                    self.settings_open = false;
                    self.stats_open = true;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                AppCommand::ShowQuota => {
                    self.visible.store(true, Ordering::Relaxed);
                    self.settings_open = false;
                    self.stats_open = false;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                AppCommand::Hide => {
                    self.visible.store(false, Ordering::Relaxed);
                    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                }
                AppCommand::Toggle => {
                    let show = !self.visible.load(Ordering::Relaxed);
                    self.visible.store(show, Ordering::Relaxed);
                    ctx.send_viewport_cmd(ViewportCommand::Visible(show));
                    if show {
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                }
                AppCommand::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
    }

    fn scene_index(&self) -> usize {
        let cfg = self.config.read();
        if !cfg.auto_scene {
            return cfg.scene % SCENES.len();
        }
        self.weather.read().scene_index.unwrap_or(CLEAR_SCENE)
    }

    fn language(&self) -> Language {
        Language::from_code(self.config.read().language.as_deref())
    }

    fn ensure_backgrounds(&mut self, ctx: &egui::Context, scene_index: usize) {
        for (background_index, bytes) in BACKGROUND_BYTES[scene_index].iter().enumerate() {
            if self.backgrounds[scene_index][background_index].is_some() {
                continue;
            }
            let Ok(decoded) = image::load_from_memory(bytes) else {
                continue;
            };
            let rgba = decoded.to_rgba8();
            let dominant = dominant_color(&rgba);
            let palette = text_palette(dominant);
            let frosted = image::imageops::blur(&rgba, 10.0);
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let texture = ctx.load_texture(
                format!("scene-{scene_index}-{background_index}"),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            let frosted_image = egui::ColorImage::from_rgba_unmultiplied(size, frosted.as_raw());
            let frosted_texture = ctx.load_texture(
                format!("scene-frosted-{scene_index}-{background_index}"),
                frosted_image,
                egui::TextureOptions::LINEAR,
            );
            self.backgrounds[scene_index][background_index] = Some(texture);
            self.frosted_backgrounds[scene_index][background_index] = Some(frosted_texture);
            self.background_palettes[scene_index][background_index] = Some(palette);
            self.background_dominants[scene_index][background_index] = Some(dominant);
        }
    }

    fn active_text_palette(&self, scene_index: usize) -> TextPalette {
        let cfg = self.config.read();
        let background_index = cfg.background % 3;
        let opacity = cfg.background_opacity.clamp(0.0, 1.0);
        let underlay = if cfg.desktop_glass {
            self.desktop_glass_dominant
                .unwrap_or(SCENES[scene_index].sky)
        } else {
            SCENES[scene_index].sky
        };
        self.background_dominants[scene_index][background_index]
            .map(|dominant| text_palette(blend_rgb(underlay, dominant, opacity)))
            .or(self.background_palettes[scene_index][background_index])
            .unwrap_or_else(|| text_palette(underlay))
    }

    fn active_frosted_background(&self, scene_index: usize) -> Option<TextureHandle> {
        let background_index = self.config.read().background % 3;
        self.frosted_backgrounds[scene_index][background_index].clone()
    }

    fn receive_desktop_glass(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.desktop_glass_rx.try_recv() {
            self.desktop_glass_loading = false;
            match result {
                Ok(Some(image)) => {
                    self.desktop_glass_dominant = Some(dominant_color(&image));
                    let size = [image.width() as usize, image.height() as usize];
                    crate::diagnostics::event(format!(
                        "wallpaper_texture_upload width={} height={}",
                        size[0], size[1]
                    ));
                    self.desktop_glass_texture = Some(ctx.load_texture(
                        "desktop-wallpaper-frosted",
                        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                        egui::TextureOptions::LINEAR,
                    ));
                    crate::diagnostics::event("wallpaper_texture_ready");
                    eprintln!(
                        "widget-wallpaper-backdrop status=ready texture={}x{} renderer=glow",
                        size[0], size[1]
                    );
                }
                Ok(None) => {
                    eprintln!("widget-wallpaper-backdrop status=unavailable reason=no-wallpaper");
                }
                Err(error) => {
                    eprintln!("widget-wallpaper-backdrop status=failed error={error}");
                }
            }
        }
    }

    fn paint_scene(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        scene_index: usize,
        weather: &WeatherState,
        frame: &eframe::Frame,
    ) {
        let painter = ui.painter();
        // Windows already applies an exact native HRGN to the whole swapchain.
        // Drawing another antialiased egui radius leaves the first pixel row at
        // partial coverage, exposing the much brighter scene fallback as a top
        // seam. Paint the complete surface on Windows and let the HRGN own all
        // four corners; other platforms still need the visual egui radius.
        let outer_radius = if cfg!(target_os = "windows") {
            CornerRadius::ZERO
        } else {
            CornerRadius::same(WINDOW_CORNER_RADIUS)
        };
        let cfg = self.config.read();
        let background_index = cfg.background % 3;
        let background_opacity = cfg.background_opacity.clamp(0.0, 1.0);
        let reduced = cfg.reduced_motion;
        let auto_scene = cfg.auto_scene;
        let desktop_glass = cfg.desktop_glass && cfg.desktop_attached;
        drop(cfg);
        if desktop_glass
            && self.native_drag_origin.is_none()
            && let Some(texture) = self.desktop_glass_texture.as_ref()
            && let Some(sample) = crate::desktop::desktop_sample(frame)
        {
            painter.image(
                texture.id(),
                rect,
                desktop_glass_uv(texture.size_vec2(), sample),
                Color32::WHITE,
            );
        }
        let texture = self.backgrounds[scene_index][background_index].as_ref();
        if let Some(texture) = texture {
            let uv = cover_uv(texture.size_vec2(), rect.size());
            let tint = Color32::from_white_alpha((background_opacity * 255.0).round() as u8);
            if cfg!(target_os = "windows") {
                // Painter::image uses an unrounded image mesh. Unlike a
                // RectShape, it does not generate an antialiased outer edge
                // that can leave the first swapchain row at zero coverage.
                painter.image(texture.id(), rect, uv, tint);
            } else {
                painter.add(
                    RectShape::filled(rect, outer_radius, tint).with_texture(texture.id(), uv),
                );
            }
        }
        painter.rect_filled(
            rect,
            outer_radius,
            Color32::from_black_alpha((18.0 * background_opacity).round() as u8),
        );
        let time = if reduced {
            0.0
        } else {
            ui.input(|i| i.time) as f32
        };
        let effects = if auto_scene {
            weather::weather_effects(weather.qweather_icon.as_deref(), weather.weather_code)
        } else {
            effects_for_manual_scene(scene_index)
        };
        paint_weather_particles(painter, rect, effects, time, reduced);
    }

    fn header(
        &mut self,
        ui: &mut egui::Ui,
        frame: &eframe::Frame,
        scene_index: usize,
        weather: &WeatherState,
        language: Language,
        controls_visibility: f32,
    ) -> Rect {
        let (position_locked, weather_icon_theme) = {
            let cfg = self.config.read();
            (cfg.position_locked, cfg.weather_icon_theme)
        };
        let (header_rect, drag) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 30.0),
            if position_locked {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            },
        );
        let desktop_attached =
            self.config.read().desktop_attached && crate::desktop::is_desktop_attached(frame);
        if !position_locked && drag.drag_started() {
            if desktop_attached {
                self.native_drag_origin = crate::desktop::begin_window_drag(frame);
            } else {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
        }
        if !position_locked
            && desktop_attached
            && drag.dragged()
            && let Some(origin) = self.native_drag_origin
            && let Some(position) = crate::desktop::drag_window_to(frame, origin)
        {
            self.last_native_position = Some(position);
            self.position_save_due = Some(Instant::now() + Duration::from_millis(350));
        }
        if drag.drag_stopped() {
            self.native_drag_origin = None;
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
            ui.set_min_height(header_rect.height());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if svg_icon_button(
                    ui,
                    egui::include_image!("../assets/ui-icons/close.svg"),
                    tr(language, Text::HideToTray),
                    controls_visibility,
                )
                .clicked()
                {
                    self.visible.store(false, Ordering::Relaxed);
                    ui.ctx().send_viewport_cmd(ViewportCommand::Visible(false));
                }
                if svg_icon_button(
                    ui,
                    egui::include_image!("../assets/ui-icons/settings.svg"),
                    tr(language, Text::SettingsTooltip),
                    controls_visibility,
                )
                .clicked()
                {
                    self.settings_open = !self.settings_open;
                    if self.settings_open {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Focus);
                        let _ = crate::desktop::request_keyboard_focus(frame);
                    }
                }
                let lock_icon = if position_locked {
                    egui::include_image!("../assets/ui-icons/lock.svg")
                } else {
                    egui::include_image!("../assets/ui-icons/unlock.svg")
                };
                if svg_icon_button(
                    ui,
                    lock_icon,
                    if position_locked {
                        tr(language, Text::LockedTooltip)
                    } else {
                        tr(language, Text::UnlockedTooltip)
                    },
                    controls_visibility,
                )
                .clicked()
                {
                    let mut cfg = self.config.write();
                    cfg.position_locked = !cfg.position_locked;
                    let _ = cfg.save();
                }
                let weather_text = match (weather.temperature_min, weather.temperature_max) {
                    (Some(low), Some(high)) => format!("{low:.0}°/{high:.0}°"),
                    _ => scene_label(scene_index, language).to_owned(),
                };
                let location = weather
                    .resolved_location
                    .as_deref()
                    .unwrap_or(tr(language, Text::WeatherUnsetTooltip));
                let mut weather_tooltip = match weather.source {
                    Some(source) => format!(
                        "{location}\n{}: {}",
                        tr(language, Text::WeatherDataBy),
                        source.label()
                    ),
                    None => location.to_owned(),
                };
                if let Some(air_quality) = weather.air_quality.as_ref() {
                    weather_tooltip.push_str(&format!(
                        "\n{}: AQI {} · {}",
                        tr(language, Text::AirQuality),
                        air_quality.aqi_display,
                        air_quality.category
                    ));
                    if let Some(pollutant) = air_quality.primary_pollutant.as_deref() {
                        weather_tooltip.push_str(&format!(
                            "\n{}: {pollutant}",
                            tr(language, Text::PrimaryPollutant)
                        ));
                    }
                }
                Frame::new()
                    .fill(Color32::from_black_alpha(108))
                    .corner_radius(5)
                    .inner_margin(Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add(
                                egui::Image::new(if weather.weather_code.is_some() {
                                    weather_icon_source(
                                        weather_icon_theme,
                                        weather.qweather_icon.as_deref(),
                                        weather.weather_code,
                                    )
                                } else {
                                    scene_icon_source(weather_icon_theme, scene_index)
                                })
                                .fit_to_exact_size(Vec2::splat(20.0)),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(weather_text)
                                        .size(10.5)
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .sense(Sense::hover()),
                            );
                            if let Some(air_quality) = weather.air_quality.as_ref() {
                                ui.label(
                                    RichText::new("·")
                                        .size(10.0)
                                        .color(Color32::from_white_alpha(150)),
                                );
                                let color = air_quality
                                    .color
                                    .map(|[red, green, blue]| Color32::from_rgb(red, green, blue))
                                    .unwrap_or(Color32::from_rgb(155, 225, 186));
                                ui.label(
                                    RichText::new(format!(
                                        "AQI {} {}",
                                        air_quality.aqi_display, air_quality.category
                                    ))
                                    .size(10.5)
                                    .color(color)
                                    .strong(),
                                );
                            }
                            if let Some(source) = weather.source {
                                ui.label(
                                    RichText::new(format!("· {}", source.label()))
                                        .size(9.5)
                                        .color(Color32::from_white_alpha(205)),
                                );
                            }
                        });
                    })
                    .response
                    .on_hover_text(weather_tooltip);
            });
        });
        header_rect
    }

    fn date_block(
        &self,
        ui: &mut egui::Ui,
        snapshot: &UsageSnapshot,
        palette: TextPalette,
        language: Language,
    ) -> Rect {
        let now = Local::now();
        ui.horizontal(|ui| {
            shadowed_label(
                ui,
                RichText::new(format!("{:02}", now.day()))
                    .size(68.0)
                    .strong(),
                palette.photo_primary,
                TextSurface::Photo,
            );
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                shadowed_label(
                    ui,
                    RichText::new(month_name(language, now.month()))
                        .size(19.0)
                        .strong(),
                    palette.photo_primary,
                    TextSurface::Photo,
                );
                shadowed_label(
                    ui,
                    RichText::new(format!(
                        "{} · {}",
                        weekday_name(language, now.weekday()),
                        now.year()
                    ))
                    .size(12.0),
                    palette.photo_muted,
                    TextSurface::Photo,
                );
                ui.add_space(5.0);
                let project = snapshot
                    .active_cwd
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or(tr(language, Text::TodayWork));
                let focus_text = format!("{}  {project}", tr(language, Text::FocusPrefix));
                shadowed_truncated_label(
                    ui,
                    RichText::new(&focus_text).size(11.5),
                    palette.photo_accent,
                    TextSurface::Photo,
                    ui.available_width(),
                )
                .on_hover_text(focus_text);
            });
        })
        .response
        .rect
    }

    fn week_strip(&self, ui: &mut egui::Ui, weather: &WeatherState, language: Language) -> Rect {
        let now = Local::now();
        let today_index = now.weekday().num_days_from_monday() as usize;
        let weather_icon_theme = self.config.read().weather_icon_theme;
        Frame::new()
            .fill(Color32::from_black_alpha(96))
            .corner_radius(12)
            .inner_margin(Margin::same(6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (index, day) in weekday_short(language).iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(
                                (ui.available_width() - (6 - index) as f32 * 6.0)
                                    / (7 - index) as f32,
                                44.0,
                            ),
                            Sense::hover(),
                        );
                        let active = index == today_index;
                        let forecast = weather.days.iter().find(|forecast| {
                            NaiveDate::parse_from_str(&forecast.date, "%Y-%m-%d").is_ok_and(
                                |date| {
                                    date.iso_week() == now.date_naive().iso_week()
                                        && date.weekday().num_days_from_monday() as usize == index
                                },
                            )
                        });
                        if let Some(forecast) = forecast {
                            response.on_hover_text(format!(
                                "{}  {:.0}°/{:.0}°",
                                condition_label(Some(forecast.weather_code), language),
                                forecast.temperature_min.unwrap_or_default(),
                                forecast.temperature_max.unwrap_or_default(),
                            ));
                        }
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(8),
                            if active {
                                Color32::from_rgba_unmultiplied(
                                    PAPER.r(),
                                    PAPER.g(),
                                    PAPER.b(),
                                    236,
                                )
                            } else {
                                Color32::from_white_alpha(16)
                            },
                        );
                        paint_shadowed_text(
                            ui.painter(),
                            rect.center_top() + Vec2::new(0.0, 5.0),
                            Align2::CENTER_TOP,
                            day,
                            FontId::proportional(10.5),
                            if active {
                                INK
                            } else {
                                Color32::from_white_alpha(230)
                            },
                            TextSurface::Glass,
                        );
                        let icon_rect = Rect::from_center_size(
                            rect.center_bottom() - Vec2::new(0.0, 12.0),
                            Vec2::splat(22.0),
                        );
                        egui::Image::new(weather_icon_source(
                            weather_icon_theme,
                            forecast.and_then(|day| day.qweather_icon.as_deref()),
                            forecast.map(|day| day.weather_code),
                        ))
                        .fit_to_exact_size(icon_rect.size())
                        .paint_at(ui, icon_rect);
                    }
                });
            })
            .response
            .rect
    }

    fn quota_block(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &UsageSnapshot,
        palette: TextPalette,
        frosted: Option<&TextureHandle>,
        scene_rect: Rect,
        language: Language,
    ) -> Rect {
        let mut windows: Vec<&RateWindow> = snapshot
            .plan
            .iter()
            .flat_map(|plan| [plan.primary.as_ref(), plan.secondary.as_ref()])
            .flatten()
            .collect();
        windows.sort_by_key(|window| quota_window_order(window));
        let five_hour_window = windows
            .iter()
            .copied()
            .find(|window| quota_window_order(window) == 0);
        let weekly_window = windows
            .iter()
            .copied()
            .find(|window| quota_window_order(window) == 1);
        let ring_window = weekly_window
            .or(five_hour_window)
            .or_else(|| windows.first().copied());
        let secondary_ring = weekly_window.and(five_hour_window);
        let remaining = ring_window.map_or(100, |window| window.remaining_pct);
        let ring_label =
            ring_window.map_or(tr(language, Text::QuotaRemaining).to_owned(), |window| {
                format!(
                    "{}{}",
                    quota_window_name(window, language),
                    tr(language, Text::RemainingSuffix)
                )
            });
        let plan = snapshot
            .plan
            .as_ref()
            .and_then(|p| p.plan_type.as_deref())
            .unwrap_or("LOCAL")
            .to_ascii_uppercase();
        let reset_credits_available = snapshot
            .plan
            .as_ref()
            .and_then(|plan| plan.reset_credits_available);
        let panel_width = ui.available_width();
        let background_opacity = self.config.read().background_opacity;
        frosted_panel(
            ui,
            FrostedContext {
                texture: frosted,
                scene_rect,
                texture_opacity: background_opacity,
            },
            palette.glass,
            18,
            Margin::same(15),
            |ui| {
                ui.set_min_width((panel_width - 30.0).max(0.0));
                let content_rect = ui.available_rect_before_wrap();
                let statistics_rect = Rect::from_min_size(
                    Pos2::new(content_rect.right() - 28.0, content_rect.top()),
                    Vec2::splat(28.0),
                );
                if svg_icon_button_at(
                    ui,
                    statistics_rect,
                    "quota-statistics",
                    egui::include_image!("../assets/ui-icons/statistics.svg"),
                    tr(language, Text::MoreStatistics),
                )
                .clicked()
                {
                    self.stats_open = true;
                    self.settings_open = false;
                }
                ui.horizontal(|ui| {
                    let (cool_tone, _) = semantic_card_tones(palette);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
                    paint_quota_ring(ui, rect, remaining, &ring_label, palette, false);
                    ui.add_space(8.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(112.0, 96.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            shadowed_label(
                                ui,
                                RichText::new(format!("{plan} {}", tr(language, Text::PlanSuffix)))
                                    .size(12.0)
                                    .strong(),
                                cool_tone,
                                TextSurface::Glass,
                            );
                            ui.add_space(4.0);
                            if windows.is_empty() {
                                shadowed_label(
                                    ui,
                                    RichText::new(tr(language, Text::WaitingQuota)).size(11.0),
                                    palette.card_muted,
                                    TextSurface::Glass,
                                );
                            } else if let Some(window) = ring_window {
                                let (cool_tone, warm_tone) = semantic_card_tones(palette);
                                ui.add_space(7.0);
                                shadowed_label(
                                    ui,
                                    RichText::new(format!(
                                        "{}{}",
                                        localized_reset_duration(window, language),
                                        tr(language, Text::ResetAfter)
                                    ))
                                    .size(11.0)
                                    .strong(),
                                    warm_tone,
                                    TextSurface::Glass,
                                );
                                if let Some(count) = reset_credits_available {
                                    ui.add_space(5.0);
                                    shadowed_label(
                                        ui,
                                        RichText::new(manual_reset_count_label(count, language))
                                            .size(10.0)
                                            .strong(),
                                        cool_tone,
                                        TextSurface::Glass,
                                    );
                                }
                            }
                        },
                    );
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(72.0, 96.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.add_space(30.0);
                            if let Some(window) = secondary_ring {
                                let label = format!(
                                    "{}{}",
                                    quota_window_name(window, language),
                                    tr(language, Text::RemainingSuffix)
                                );
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(66.0), Sense::hover());
                                paint_quota_ring(
                                    ui,
                                    rect,
                                    window.remaining_pct,
                                    &label,
                                    palette,
                                    true,
                                );
                            }
                        },
                    );
                });
            },
        )
        .response
        .rect
    }

    fn statistics_panel(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &UsageSnapshot,
        palette: TextPalette,
        frosted: Option<&TextureHandle>,
        scene_rect: Rect,
        language: Language,
    ) -> Rect {
        let panel_width = ui.available_width();
        let background_opacity = self.config.read().background_opacity;
        frosted_panel(
            ui,
            FrostedContext {
                texture: frosted,
                scene_rect,
                texture_opacity: background_opacity,
            },
            palette.glass,
            18,
            Margin::same(14),
            |ui| {
                ui.set_min_width((panel_width - 28.0).max(0.0));
                ui.set_min_height(194.0);

                let content_rect = ui.available_rect_before_wrap();
                let back_rect = Rect::from_min_size(
                    Pos2::new(content_rect.right() - 28.0, content_rect.top()),
                    Vec2::splat(28.0),
                );
                if svg_icon_button_at(
                    ui,
                    back_rect,
                    "statistics-back",
                    egui::include_image!("../assets/ui-icons/back.svg"),
                    tr(language, Text::Back),
                )
                .clicked()
                {
                    self.stats_open = false;
                }

                shadowed_label(
                    ui,
                    RichText::new(tr(language, Text::RecentSevenDays))
                        .size(13.0)
                        .strong(),
                    palette.card_primary,
                    TextSurface::Glass,
                );
                ui.add_space(4.0);

                let projects = project_chart_entries(snapshot, palette, language);
                let has_project_data = projects.iter().any(|(_, tokens, _)| *tokens > 0);
                let has_daily_data = snapshot.recent_daily_usage.iter().any(|day| day.tokens > 0);

                if !has_project_data && !has_daily_data {
                    ui.add_space(25.0);
                    shadowed_label(
                        ui,
                        RichText::new(tr(language, Text::NoLocalUsage))
                            .size(14.0)
                            .strong(),
                        palette.card_primary,
                        TextSurface::Glass,
                    );
                    ui.add_space(6.0);
                    shadowed_label(
                        ui,
                        RichText::new(tr(language, Text::CcusageHint)).size(10.0),
                        palette.card_muted,
                        TextSurface::Glass,
                    );
                    return;
                }

                ui.horizontal(|ui| {
                    let (donut_rect, _) =
                        ui.allocate_exact_size(Vec2::new(92.0, 88.0), Sense::hover());
                    paint_project_donut(ui, donut_rect, &projects, palette, language);
                    ui.add_space(8.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 88.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            shadowed_label(
                                ui,
                                RichText::new(tr(language, Text::ProjectShare))
                                    .size(10.5)
                                    .strong(),
                                palette.card_accent,
                                TextSurface::Glass,
                            );
                            ui.add_space(2.0);
                            for (name, tokens, color) in &projects {
                                project_legend_row(ui, name, *tokens, *color, palette);
                            }
                        },
                    );
                });
                ui.add_space(2.0);
                shadowed_label(
                    ui,
                    RichText::new(tr(language, Text::DailyTokens))
                        .size(10.5)
                        .strong(),
                    palette.card_accent,
                    TextSurface::Glass,
                );
                let (chart_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), Sense::hover());
                paint_daily_token_chart(ui, chart_rect, &snapshot.recent_daily_usage, palette);
            },
        )
        .response
        .rect
    }

    fn metrics(
        &self,
        ui: &mut egui::Ui,
        snapshot: &UsageSnapshot,
        palette: TextPalette,
        frosted: Option<&TextureHandle>,
        scene_rect: Rect,
        language: Language,
    ) -> Rect {
        let glass = FrostedContext {
            texture: frosted,
            scene_rect,
            texture_opacity: self.config.read().background_opacity,
        };
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 8.0) / 2.0;
            metric_card(
                ui,
                width,
                MetricCardContent {
                    label: tr(language, Text::CallsToday),
                    value: snapshot.calls_today.to_string(),
                    caption: format!(
                        "{} {}",
                        snapshot.sessions_today,
                        tr(language, Text::Sessions)
                    ),
                    tone: MetricTone::Cool,
                },
                palette,
                glass,
            );
            metric_card(
                ui,
                width,
                MetricCardContent {
                    label: tr(language, Text::TokensToday),
                    value: compact_number(snapshot.daily.used),
                    caption: tr(language, Text::LocalDailyTotal).to_owned(),
                    tone: MetricTone::Warm,
                },
                palette,
                glass,
            );
        })
        .response
        .rect
    }

    fn settings(&mut self, ui: &mut egui::Ui, palette: TextPalette) {
        while let Ok((query, result)) = self.location_search_rx.try_recv() {
            if self.location_search_pending.as_deref() != Some(query.as_str()) {
                continue;
            }
            self.location_search_pending = None;
            match result {
                Ok(candidates) => {
                    self.location_candidates = candidates;
                    self.location_search_error = None;
                }
                Err(error) => {
                    self.location_candidates.clear();
                    self.location_search_error = Some(error);
                }
            }
        }
        let mut cfg = self.config.read().clone();
        let mut language = Language::from_code(cfg.language.as_deref());
        let before = (
            cfg.follow_codex,
            cfg.always_on_top,
            cfg.reduced_motion,
            cfg.auto_scene,
            cfg.desktop_attached,
            cfg.background_opacity,
            cfg.desktop_glass,
            cfg.auto_start,
        );
        let language_before = cfg.language.clone();
        let weather_service_before = (cfg.weather_provider, cfg.qweather_api_host.clone());
        let weather_icon_theme_before = cfg.weather_icon_theme;
        ui.label(
            RichText::new(tr(language, Text::Language))
                .size(11.0)
                .color(palette.card_muted),
        );
        if self.language_popup_requested {
            let button_id = ui.make_persistent_id(egui::IdSalt::new("widget-language"));
            egui::Popup::open_id(ui.ctx(), button_id.with("popup"));
            self.language_popup_requested = false;
        }
        egui::ComboBox::from_id_salt("widget-language")
            .selected_text(language.native_name())
            .width(ui.available_width())
            .popup_style(settings_popup_style(palette))
            .show_ui(ui, |ui| {
                for option in Language::ALL {
                    ui.selectable_value(&mut language, option, option.native_name());
                }
            });
        cfg.language = Some(language.code().to_owned());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                RichText::new(tr(language, Text::WeatherService))
                    .size(11.0)
                    .color(palette.card_muted),
            );
            if should_show_qweather_help(cfg.weather_provider)
                && qweather_help_button(ui, tr(language, Text::QWeatherHelp), palette.card_accent)
                    .clicked()
            {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(qweather_help_url(language)));
            }
        });
        egui::ComboBox::from_id_salt("weather-provider")
            .selected_text(match cfg.weather_provider {
                WeatherProvider::Auto => tr(language, Text::WeatherProviderAuto),
                WeatherProvider::OpenMeteo => tr(language, Text::WeatherProviderOpenMeteo),
                WeatherProvider::QWeather => tr(language, Text::WeatherProviderQWeather),
            })
            .width(ui.available_width())
            .popup_style(settings_popup_style(palette))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut cfg.weather_provider,
                    WeatherProvider::Auto,
                    tr(language, Text::WeatherProviderAuto),
                );
                ui.selectable_value(
                    &mut cfg.weather_provider,
                    WeatherProvider::OpenMeteo,
                    tr(language, Text::WeatherProviderOpenMeteo),
                );
                ui.selectable_value(
                    &mut cfg.weather_provider,
                    WeatherProvider::QWeather,
                    tr(language, Text::WeatherProviderQWeather),
                );
            });
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr(language, Text::WeatherIconTheme))
                .size(10.0)
                .color(palette.card_muted),
        );
        egui::ComboBox::from_id_salt("weather-icon-theme")
            .selected_text(match cfg.weather_icon_theme {
                WeatherIconTheme::Crystal => tr(language, Text::WeatherIconThemeCrystal),
                WeatherIconTheme::Duotone => tr(language, Text::WeatherIconThemeDuotone),
                WeatherIconTheme::Neon => tr(language, Text::WeatherIconThemeNeon),
            })
            .width(ui.available_width())
            .popup_style(settings_popup_style(palette))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut cfg.weather_icon_theme,
                    WeatherIconTheme::Crystal,
                    tr(language, Text::WeatherIconThemeCrystal),
                );
                ui.selectable_value(
                    &mut cfg.weather_icon_theme,
                    WeatherIconTheme::Duotone,
                    tr(language, Text::WeatherIconThemeDuotone),
                );
                ui.selectable_value(
                    &mut cfg.weather_icon_theme,
                    WeatherIconTheme::Neon,
                    tr(language, Text::WeatherIconThemeNeon),
                );
            });
        if cfg.weather_provider != WeatherProvider::OpenMeteo {
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(language, Text::QWeatherApiHost))
                    .size(10.0)
                    .color(palette.card_muted),
            );
            let host_id = ui.make_persistent_id("qweather-api-host-input");
            let host_focused = ui.memory(|memory| memory.has_focus(host_id));
            let show_host_hint = should_show_text_hint(&cfg.qweather_api_host, host_focused);
            let mut host_edit = egui::TextEdit::singleline(&mut cfg.qweather_api_host).id(host_id);
            if show_host_hint {
                host_edit = host_edit.hint_text(
                    RichText::new("abc123.qweatherapi.com")
                        .italics()
                        .color(palette.card_muted.gamma_multiply(0.58)),
                );
            }
            ui.add_sized([ui.available_width(), 28.0], host_edit);
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(language, Text::QWeatherApiKey))
                    .size(10.0)
                    .color(palette.card_muted),
            );
            ui.horizontal(|ui| {
                ui.add_sized(
                    [ui.available_width() - 104.0, 28.0],
                    egui::TextEdit::singleline(&mut self.qweather_api_key_draft).password(true),
                );
                let has_key_draft = !self.qweather_api_key_draft.trim().is_empty();
                if ui
                    .add_enabled_ui(has_key_draft, |ui| {
                        ui.add_sized(
                            [98.0, 28.0],
                            egui::Button::new(tr(language, Text::SaveApiKey)),
                        )
                    })
                    .inner
                    .clicked()
                {
                    match credentials::save_qweather_api_key(&self.qweather_api_key_draft) {
                        Ok(()) => {
                            self.qweather_api_key_saved = true;
                            self.qweather_api_key_draft.clear();
                            self.weather_credentials_status =
                                Some(tr(language, Text::ApiKeySaved).to_owned());
                        }
                        Err(error) => {
                            self.weather_credentials_status =
                                Some(format!("{}: {error}", tr(language, Text::ApiKeySaveFailed)));
                        }
                    }
                }
            });
            ui.label(
                RichText::new(tr(language, Text::QWeatherSetupHint))
                    .size(9.5)
                    .color(palette.card_muted),
            );
            if let Some(status) = self.weather_credentials_status.as_deref() {
                ui.label(RichText::new(status).size(9.5).color(palette.card_accent));
            } else if self.qweather_api_key_saved {
                ui.label(
                    RichText::new(tr(language, Text::ApiKeySaved))
                        .size(9.5)
                        .color(palette.card_accent),
                );
            }
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr(language, Text::WeatherAddress))
                .size(11.0)
                .color(palette.card_muted),
        );
        let mut search_requested = false;
        ui.horizontal(|ui| {
            let edit = ui.add_sized(
                [ui.available_width() - 74.0, 28.0],
                egui::TextEdit::singleline(&mut self.location_draft)
                    .hint_text(tr(language, Text::LocationPlaceholder)),
            );
            search_requested = ui
                .add_sized(
                    [68.0, 28.0],
                    egui::Button::new(tr(language, Text::SearchLocation)),
                )
                .clicked()
                || (edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
        });
        if search_requested {
            let query = self.location_draft.trim().to_owned();
            if query.chars().count() < 2 {
                self.location_search_error = Some(tr(language, Text::SearchMinimum).to_owned());
            } else if self
                .location_search_tx
                .send(LocationSearchRequest {
                    query: query.clone(),
                    language,
                    services: weather::WeatherServices::new(
                        cfg.weather_provider,
                        cfg.qweather_api_host.clone(),
                        Some(if self.qweather_api_key_draft.trim().is_empty() {
                            credentials::load_qweather_api_key()
                                .ok()
                                .flatten()
                                .unwrap_or_default()
                        } else {
                            self.qweather_api_key_draft.clone()
                        }),
                    ),
                })
                .is_ok()
            {
                self.location_search_pending = Some(query);
                self.location_candidates.clear();
                self.location_search_error = None;
            }
        }
        if self.location_search_pending.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(tr(language, Text::SearchingLocations))
                        .size(10.0)
                        .color(palette.card_muted),
                );
            });
        } else if let Some(error) = self.location_search_error.as_deref() {
            ui.label(
                RichText::new(format!("{}: {error}", tr(language, Text::SearchFailed)))
                    .size(10.0)
                    .color(TERRACOTTA),
            );
        }

        let mut selected_candidate = None;
        if !self.location_candidates.is_empty() {
            ui.label(
                RichText::new(tr(language, Text::SelectForecastLocation))
                    .size(10.0)
                    .color(palette.card_muted),
            );
            egui::ScrollArea::vertical()
                .max_height(132.0)
                .show(ui, |ui| {
                    for (index, candidate) in self.location_candidates.iter().enumerate() {
                        let selected = cfg
                            .weather_latitude
                            .is_some_and(|latitude| (latitude - candidate.latitude).abs() < 0.0001)
                            && cfg.weather_longitude.is_some_and(|longitude| {
                                (longitude - candidate.longitude).abs() < 0.0001
                            });
                        if ui
                            .selectable_label(selected, candidate.display_name())
                            .on_hover_text(format!(
                                "{:.4}, {:.4}",
                                candidate.latitude, candidate.longitude
                            ))
                            .clicked()
                        {
                            selected_candidate = Some(index);
                        }
                    }
                });
        }
        if let Some(index) = selected_candidate {
            let candidate = &self.location_candidates[index];
            cfg.weather_location = self.location_draft.trim().to_owned();
            cfg.weather_display_name = Some(candidate.display_name());
            cfg.weather_latitude = Some(candidate.latitude);
            cfg.weather_longitude = Some(candidate.longitude);
            cfg.auto_scene = true;
            self.location_search_error = None;
        }

        let weather = self.weather.read().clone();
        let weather_status = if let Some(location) = cfg.weather_display_name.as_deref() {
            format!("{}: {location}", tr(language, Text::SelectedLocation))
        } else if let Some(error) = weather.error.as_deref() {
            format!("{}: {error}", tr(language, Text::WeatherUpdateFailed))
        } else if let Some(location) = weather.resolved_location.as_deref() {
            format!("{}: {location}", tr(language, Text::LocatedAt))
        } else {
            tr(language, Text::SearchLocationHint).to_owned()
        };
        ui.label(
            RichText::new(weather_status)
                .size(10.0)
                .color(palette.card_muted),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(language, Text::BackgroundOpacity))
                    .size(11.0)
                    .color(palette.card_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{}%",
                        (cfg.background_opacity * 100.0).round() as u8
                    ))
                    .size(10.0)
                    .color(palette.card_primary),
                );
            });
        });
        ui.add(
            egui::Slider::new(&mut cfg.background_opacity, 0.0..=1.0)
                .show_value(false)
                .step_by(0.01),
        );
        ui.add_space(8.0);
        egui::Grid::new("settings-options")
            .num_columns(2)
            .spacing(Vec2::new(14.0, 6.0))
            .show(ui, |ui| {
                ui.checkbox(&mut cfg.follow_codex, tr(language, Text::FollowCodex));
                ui.checkbox(&mut cfg.reduced_motion, tr(language, Text::ReduceMotion));
                ui.end_row();

                ui.checkbox(&mut cfg.auto_scene, tr(language, Text::FollowWeather));
                if cfg!(target_os = "windows") {
                    ui.checkbox(&mut cfg.desktop_attached, tr(language, Text::AttachDesktop));
                    ui.end_row();
                    ui.add_enabled(
                        cfg.desktop_attached,
                        egui::Checkbox::new(
                            &mut cfg.desktop_glass,
                            tr(language, Text::DesktopGlass),
                        ),
                    );
                }

                ui.add_enabled(
                    !cfg.desktop_attached,
                    egui::Checkbox::new(&mut cfg.always_on_top, tr(language, Text::AlwaysOnTop)),
                );
                ui.end_row();

                if cfg!(target_os = "windows") {
                    let mut auto_start = cfg.auto_start;
                    if ui
                        .checkbox(&mut auto_start, tr(language, Text::AutoStart))
                        .changed()
                    {
                        match crate::autostart::configure(auto_start) {
                            Ok(()) => {
                                cfg.auto_start = auto_start;
                                self.autostart_error = None;
                            }
                            Err(error) => {
                                self.autostart_error = Some(format!(
                                    "{}: {error}",
                                    tr(language, Text::AutoStartFailed)
                                ));
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        if let Some(error) = self.autostart_error.as_deref() {
            ui.add_space(4.0);
            ui.label(RichText::new(error).size(10.0).color(TERRACOTTA));
        }
        let after = (
            cfg.follow_codex,
            cfg.always_on_top,
            cfg.reduced_motion,
            cfg.auto_scene,
            cfg.desktop_attached,
            cfg.background_opacity,
            cfg.desktop_glass,
            cfg.auto_start,
        );
        let current_cfg = self.config.read();
        let address_changed = cfg.weather_location != current_cfg.weather_location
            || cfg.weather_display_name != current_cfg.weather_display_name
            || cfg.weather_latitude != current_cfg.weather_latitude
            || cfg.weather_longitude != current_cfg.weather_longitude;
        let weather_service_changed =
            weather_service_before != (cfg.weather_provider, cfg.qweather_api_host.clone());
        drop(current_cfg);
        if before != after
            || address_changed
            || weather_service_changed
            || weather_icon_theme_before != cfg.weather_icon_theme
            || language_before != cfg.language
        {
            ui.ctx().send_viewport_cmd(ViewportCommand::WindowLevel(
                if cfg.always_on_top && !cfg.desktop_attached {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                },
            ));
            let _ = cfg.save();
            *self.config.write() = cfg;
        }
    }
}

impl eframe::App for CalendarApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let diagnostic_state = {
            let cfg = self.config.read();
            let (focused, minimized) =
                ctx.input(|i| (i.viewport().focused, i.viewport().minimized));
            format!(
                "settings={} stats={} visible={} focused={focused:?} minimized={minimized:?} attached_config={} attached_actual={:?} glass={} opacity={} quitting={}",
                self.settings_open,
                self.stats_open,
                self.visible.load(Ordering::Relaxed),
                cfg.desktop_attached,
                self.native_attached_state,
                cfg.desktop_glass,
                cfg.background_opacity,
                self.quitting
            )
        };
        let diagnostic_due = self.diagnostic_state != diagnostic_state
            || self.diagnostic_heartbeat.elapsed() >= Duration::from_secs(60);
        if diagnostic_due {
            crate::diagnostics::event(format!("ui_state {diagnostic_state}"));
            self.diagnostic_state = diagnostic_state;
            self.diagnostic_heartbeat = Instant::now();
        }
        if self.startup_warmup_frames > 0 {
            self.startup_warmup_frames -= 1;
            ctx.request_repaint();
        }
        self.receive_desktop_glass(ctx);
        self.handle_commands(ctx);
        let configured_attached = self.config.read().desktop_attached;
        // A HWND parented into Explorer's desktop host remains pointer-interactive,
        // but Windows does not reliably route keyboard focus to its text controls.
        // While the settings editor is open, temporarily restore the exact same
        // frameless HWND as a top-level popup. Reattach it after the editor closes.
        let attached = configured_attached && !self.settings_open;
        let was_attached = self.native_attached_state == Some(true);
        if self.native_attached_state != Some(attached)
            || (attached && !crate::desktop::is_desktop_attached(frame))
        {
            let record_attempt = diagnostic_due
                || self
                    .diagnostic_attach_failure
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(60));
            if record_attempt {
                crate::diagnostics::event(format!(
                    "native_attach_begin desired={attached} previous={:?}",
                    self.native_attached_state
                ));
            }
            match crate::desktop::set_attached(frame, attached) {
                Ok(()) => {
                    self.diagnostic_attach_failure = None;
                    crate::diagnostics::event(format!(
                        "native_attach_end desired={attached} result=ok"
                    ));
                    self.native_attached_state = Some(attached);
                    if was_attached && !attached && self.settings_open {
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                        let _ = crate::desktop::request_keyboard_focus(frame);
                    }
                }
                Err(error) if record_attempt => {
                    self.diagnostic_attach_failure = Some(Instant::now());
                    crate::diagnostics::event(format!(
                        "native_attach_end desired={attached} result=error error={error}"
                    ));
                }
                Err(_) => {}
            }
        }
        // Focus changes and WorkerW composition can restore DWM's one-pixel
        // non-client border without changing the HWND or its size. Reassert the
        // frameless policy independently from the rounded-region cache.
        let chrome_suppressed = crate::desktop::suppress_native_chrome(frame, attached).is_ok();
        let mut native_ready = !cfg!(target_os = "windows");
        if let Some(window) = frame.winit_window() {
            let size = window.inner_size();
            let size = (size.width, size.height);
            // Winit/WorkerW can clear a window region after a frame-style or
            // ownership refresh. Do not trust the cached size alone: verify
            // that the HRGN still exists and restore it on the next frame.
            if self.native_region_size != Some(size) || !crate::desktop::has_window_region(frame) {
                let corner_diameter = (48.0 * ctx.pixels_per_point()).round() as i32;
                if crate::desktop::apply_round_region(frame, corner_diameter.max(1)).is_ok() {
                    self.native_region_size = Some(size);
                }
            }
            if cfg!(target_os = "windows") {
                native_ready = chrome_suppressed
                    && self.native_attached_state == Some(attached)
                    && self.native_region_size == Some(size)
                    && crate::desktop::has_window_region(frame);
            }
            if let Ok(position) = window.outer_position() {
                let position = (position.x, position.y);
                if self.last_native_position != Some(position) {
                    self.last_native_position = Some(position);
                    self.position_save_due = Some(Instant::now() + Duration::from_millis(350));
                }
            }
        }
        let desktop_glass_ready = !self.config.read().desktop_glass || !self.desktop_glass_loading;
        if self.initial_show_pending
            && self.startup_warmup_frames == 0
            && native_ready
            && desktop_glass_ready
        {
            self.initial_show_pending = false;
            crate::diagnostics::event("initial_window_ready");
            let show = self.visible.load(Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::Visible(show));
        }
        if self
            .position_save_due
            .is_some_and(|due| Instant::now() >= due)
            && let Some((x, y)) = self.last_native_position
        {
            let mut cfg = self.config.write();
            cfg.window_x = Some(x as f32);
            cfg.window_y = Some(y as f32);
            let _ = cfg.save();
            self.position_save_due = None;
        }
        if !self.quitting && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.visible.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let snapshot = self.snapshot.read().clone();
        let weather = self.weather.read().clone();
        let scene_index = self.scene_index();
        let language = self.language();
        self.ensure_backgrounds(&ctx, scene_index);
        let text_palette = self.active_text_palette(scene_index);
        let frosted_background = self.active_frosted_background(scene_index);

        Frame::NONE
            // The swapchain and photo are already alpha-aware. Any opaque
            // fill here survives even when the photo opacity reaches zero and
            // appears as the smaller blue-gray rectangle seen behind content.
            .fill(Color32::TRANSPARENT)
            .inner_margin(Margin::same(if cfg!(target_os = "windows") {
                0
            } else {
                8
            }))
            .show(root_ui, |ui| {
                let card = ui.max_rect();
                if !cfg!(target_os = "windows") {
                    ui.painter().rect(
                        card.translate(Vec2::new(0.0, 3.0)),
                        CornerRadius::same(24),
                        Color32::from_black_alpha(28),
                        Stroke::NONE,
                        StrokeKind::Outside,
                    );
                }
                // Rect tessellation follows logical pixel boundaries, so its
                // first row can miss the physical pixel center at some scale
                // factors. Windows has a native HRGN as the final hard clip;
                // overlap the swapchain boundary by one logical pixel to keep
                // every visible edge sampled from the scene texture.
                let scene_paint_rect = if cfg!(target_os = "windows") {
                    card.expand(1.0)
                } else {
                    card
                };
                self.paint_scene(ui, scene_paint_rect, scene_index, &weather, frame);
                let background_click = ui.interact(
                    Rect::from_min_max(
                        Pos2::new(card.left(), card.top() + 42.0),
                        Pos2::new(card.right(), card.top() + card.height() * 0.56),
                    ),
                    ui.id().with("background-cycle"),
                    Sense::click(),
                );
                let content_rect = Rect::from_min_max(
                    card.min + Vec2::new(19.0, 8.0),
                    card.max - Vec2::new(19.0, 14.0),
                );
                let widget_hovered = ui.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|position| card.contains(position))
                });
                let reduced_motion = self.config.read().reduced_motion;
                let controls_visibility = if reduced_motion {
                    f32::from(widget_hovered)
                } else {
                    let progress = ui.ctx().animate_bool_with_time(
                        ui.id().with("header-controls-visible"),
                        widget_hovered,
                        0.18,
                    );
                    progress * progress * (3.0 - 2.0 * progress)
                };
                let mut foreground_rects = Vec::with_capacity(5);
                ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    foreground_rects.push(self.header(
                        ui,
                        frame,
                        scene_index,
                        &weather,
                        language,
                        controls_visibility,
                    ));
                    ui.add_space(5.0);
                    foreground_rects.push(self.date_block(ui, &snapshot, text_palette, language));
                    ui.add_space(10.0);
                    foreground_rects.push(self.week_strip(ui, &weather, language));
                    ui.add_space(13.0);
                    if self.stats_open {
                        foreground_rects.push(self.statistics_panel(
                            ui,
                            &snapshot,
                            text_palette,
                            frosted_background.as_ref(),
                            card,
                            language,
                        ));
                    } else {
                        foreground_rects.push(self.quota_block(
                            ui,
                            &snapshot,
                            text_palette,
                            frosted_background.as_ref(),
                            card,
                            language,
                        ));
                        ui.add_space(12.0);
                        foreground_rects.push(self.metrics(
                            ui,
                            &snapshot,
                            text_palette,
                            frosted_background.as_ref(),
                            card,
                            language,
                        ));
                    }
                });
                let cycle_requested = background_click.clicked()
                    && background_click
                        .interact_pointer_pos()
                        .is_some_and(|position| {
                            hits_unobstructed_background(
                                position,
                                background_click.rect,
                                &foreground_rects,
                                self.settings_open || self.stats_open,
                            )
                        });
                if cycle_requested {
                    let mut cfg = self.config.write();
                    cfg.background = (cfg.background + 1) % 3;
                    let _ = cfg.save();
                }
            });
        if self.settings_open {
            let scene_rect = root_ui.max_rect();
            let background_opacity = self.config.read().background_opacity;
            root_ui.painter().rect_filled(
                scene_rect,
                CornerRadius::same(WINDOW_CORNER_RADIUS),
                Color32::from_black_alpha(34),
            );
            let mut close_requested = false;
            egui::Window::new(tr(language, Text::WidgetSettings))
                .id(egui::Id::new("widget-settings-window"))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(340.0)
                .min_width(340.0)
                .max_width(340.0)
                .frame(Frame::NONE)
                .show(&ctx, |ui| {
                    frosted_panel(
                        ui,
                        FrostedContext {
                            texture: frosted_background.as_ref(),
                            scene_rect,
                            texture_opacity: background_opacity,
                        },
                        text_palette.glass,
                        20,
                        Margin::same(16),
                        |ui| {
                            apply_settings_visuals(ui, text_palette);
                            ui.horizontal(|ui| {
                                shadowed_label(
                                    ui,
                                    RichText::new(tr(language, Text::WidgetSettings))
                                        .size(17.0)
                                        .strong(),
                                    text_palette.card_primary,
                                    TextSurface::Glass,
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    close_requested = svg_icon_button(
                                        ui,
                                        egui::include_image!("../assets/ui-icons/close.svg"),
                                        tr(language, Text::Close),
                                        1.0,
                                    )
                                    .clicked();
                                });
                            });
                            ui.add_space(9.0);
                            ui.separator();
                            ui.add_space(9.0);
                            egui::ScrollArea::vertical()
                                .id_salt("widget-settings-scroll")
                                .max_height(350.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.settings(ui, text_palette));
                        },
                    );
                });
            if close_requested {
                self.settings_open = false;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(
            if self.config.read().reduced_motion {
                1000
            } else {
                33
            },
        ));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[derive(Clone, Copy, Default)]
struct ColorBucket {
    count: u32,
    red: u64,
    green: u64,
    blue: u64,
}

fn dominant_color(image: &image::RgbaImage) -> Color32 {
    const HUE_BINS: usize = 18;
    const SATURATION_BINS: usize = 4;
    const LIGHTNESS_BINS: usize = 6;
    let mut buckets = vec![ColorBucket::default(); HUE_BINS * SATURATION_BINS * LIGHTNESS_BINS];
    let pixel_count = (image.width() as usize).saturating_mul(image.height() as usize);
    let step = ((pixel_count / 12_000).max(1) as f32).sqrt().ceil() as usize;

    for y in (0..image.height()).step_by(step) {
        for x in (0..image.width()).step_by(step) {
            let pixel = image.get_pixel(x, y).0;
            if pixel[3] < 128 {
                continue;
            }
            let color = Color32::from_rgb(pixel[0], pixel[1], pixel[2]);
            let hsl = rgb_to_hsl(color);
            let hue = ((hsl.hue / 360.0) * HUE_BINS as f32)
                .floor()
                .clamp(0.0, (HUE_BINS - 1) as f32) as usize;
            let saturation = (hsl.saturation * SATURATION_BINS as f32)
                .floor()
                .clamp(0.0, (SATURATION_BINS - 1) as f32) as usize;
            let lightness = (hsl.lightness * LIGHTNESS_BINS as f32)
                .floor()
                .clamp(0.0, (LIGHTNESS_BINS - 1) as f32) as usize;
            let index = (hue * SATURATION_BINS + saturation) * LIGHTNESS_BINS + lightness;
            let bucket = &mut buckets[index];
            bucket.count += 1;
            bucket.red += u64::from(pixel[0]);
            bucket.green += u64::from(pixel[1]);
            bucket.blue += u64::from(pixel[2]);
        }
    }

    let Some(bucket) = buckets.iter().max_by_key(|bucket| bucket.count) else {
        return INK;
    };
    if bucket.count == 0 {
        return INK;
    }
    let count = u64::from(bucket.count);
    Color32::from_rgb(
        (bucket.red / count) as u8,
        (bucket.green / count) as u8,
        (bucket.blue / count) as u8,
    )
}

fn load_desktop_glass_image() -> anyhow::Result<Option<image::RgbaImage>> {
    let Some(path) = crate::desktop::wallpaper_path()? else {
        return Ok(None);
    };
    let source = image::open(&path)
        .map_err(|error| anyhow::anyhow!("open desktop wallpaper {}: {error}", path.display()))?
        .into_rgba8();
    let scale = (960.0 / source.width().max(1) as f32)
        .min(540.0 / source.height().max(1) as f32)
        .min(1.0);
    let width = (source.width() as f32 * scale).round().max(1.0) as u32;
    let height = (source.height() as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(
        &source,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    Ok(Some(image::imageops::blur(&resized, 10.0)))
}

fn text_palette(dominant: Color32) -> TextPalette {
    let hsl = rgb_to_hsl(dominant);
    let glass = complementary_glass_palette(dominant, hsl);
    let (photo_primary, photo_muted, photo_accent) = contrasting_hsl_colors(dominant, hsl.hue);
    let (card_primary, card_muted, card_accent) = contrasting_hsl_colors(glass.composite, hsl.hue);
    TextPalette {
        photo_primary,
        photo_muted,
        photo_accent,
        card_primary,
        card_muted,
        card_accent,
        glass,
    }
}

fn complementary_glass_palette(dominant: Color32, background_hsl: Hsl) -> GlassPalette {
    let complementary_hue = (background_hsl.hue + 180.0) % 360.0;
    let dark_background = relative_luminance(dominant) < 0.36;
    let saturation = (background_hsl.saturation * 0.55 + 0.22).clamp(0.24, 0.52);
    let tint = hsl_to_color(Hsl {
        hue: complementary_hue,
        saturation,
        lightness: if dark_background { 0.74 } else { 0.28 },
    });
    let tint_alpha = if dark_background { 62 } else { 50 };
    let composite = blend_rgb(dominant, tint, f32::from(tint_alpha) / 255.0);
    GlassPalette {
        tint,
        tint_alpha,
        composite,
    }
}

fn contrasting_hsl_colors(background: Color32, hue: f32) -> (Color32, Color32, Color32) {
    let light = hsl_to_color(Hsl {
        hue,
        saturation: 0.06,
        lightness: 0.98,
    });
    let dark = hsl_to_color(Hsl {
        hue,
        saturation: 0.12,
        lightness: 0.05,
    });
    let use_light = contrast_ratio(light, background) >= contrast_ratio(dark, background);
    let fallback = if use_light {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    let (primary, muted, accent) = if use_light {
        (
            light,
            hsl_to_color(Hsl {
                hue,
                saturation: 0.08,
                lightness: 0.92,
            }),
            hsl_to_color(Hsl {
                hue: (hue + 180.0) % 360.0,
                saturation: 0.42,
                lightness: 0.90,
            }),
        )
    } else {
        (
            dark,
            hsl_to_color(Hsl {
                hue,
                saturation: 0.12,
                lightness: 0.10,
            }),
            hsl_to_color(Hsl {
                hue: (hue + 180.0) % 360.0,
                saturation: 0.42,
                lightness: 0.12,
            }),
        )
    };
    (
        ensure_contrast(primary, background, fallback),
        ensure_contrast(muted, background, fallback),
        ensure_contrast(accent, background, fallback),
    )
}

fn ensure_contrast(candidate: Color32, background: Color32, fallback: Color32) -> Color32 {
    if contrast_ratio(candidate, background) >= 4.5 {
        candidate
    } else {
        fallback
    }
}

fn rgb_to_hsl(color: Color32) -> Hsl {
    let red = f32::from(color.r()) / 255.0;
    let green = f32::from(color.g()) / 255.0;
    let blue = f32::from(color.b()) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let lightness = (max + min) * 0.5;
    if delta <= f32::EPSILON {
        return Hsl {
            hue: 0.0,
            saturation: 0.0,
            lightness,
        };
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue_sector = if max == red {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };
    Hsl {
        hue: hue_sector * 60.0,
        saturation,
        lightness,
    }
}

fn hsl_to_color(hsl: Hsl) -> Color32 {
    let chroma = (1.0 - (2.0 * hsl.lightness - 1.0).abs()) * hsl.saturation;
    let hue = hsl.hue.rem_euclid(360.0) / 60.0;
    let secondary = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue {
        h if h < 1.0 => (chroma, secondary, 0.0),
        h if h < 2.0 => (secondary, chroma, 0.0),
        h if h < 3.0 => (0.0, chroma, secondary),
        h if h < 4.0 => (0.0, secondary, chroma),
        h if h < 5.0 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    let match_value = hsl.lightness - chroma * 0.5;
    Color32::from_rgb(
        ((red + match_value) * 255.0).round() as u8,
        ((green + match_value) * 255.0).round() as u8,
        ((blue + match_value) * 255.0).round() as u8,
    )
}

fn blend_rgb(background: Color32, foreground: Color32, alpha: f32) -> Color32 {
    let blend = |back: u8, front: u8| {
        (f32::from(back) * (1.0 - alpha) + f32::from(front) * alpha).round() as u8
    };
    Color32::from_rgb(
        blend(background.r(), foreground.r()),
        blend(background.g(), foreground.g()),
        blend(background.b(), foreground.b()),
    )
}

fn contrast_ratio(first: Color32, second: Color32) -> f32 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

fn relative_luminance(color: Color32) -> f32 {
    let linear = |component: u8| {
        let value = f32::from(component) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    linear(color.r()) * 0.2126 + linear(color.g()) * 0.7152 + linear(color.b()) * 0.0722
}

fn quota_window_order(window: &RateWindow) -> u8 {
    match window.window_minutes {
        Some(minutes) if minutes <= 360 => 0,
        Some(minutes) if minutes >= 7 * 24 * 60 => 1,
        _ if window.window_label.eq_ignore_ascii_case("5h") => 0,
        _ if window.window_label.eq_ignore_ascii_case("weekly") => 1,
        _ => 2,
    }
}

fn quota_window_name(window: &RateWindow, language: Language) -> String {
    match quota_window_order(window) {
        0 => tr(language, Text::FiveHours).to_owned(),
        1 => tr(language, Text::SevenDays).to_owned(),
        _ if !window.window_label.is_empty() => window.window_label.clone(),
        _ => tr(language, Text::ThisPeriod).to_owned(),
    }
}

fn format_reset_duration(seconds: i64, language: Language) -> String {
    let seconds = seconds.max(0);
    let total_minutes = (seconds + 59) / 60;
    let total_hours = total_minutes / 60;
    let days = total_hours / 24;
    let hours = total_hours % 24;
    let minutes = total_minutes % 60;
    match language {
        Language::ZhCn if days > 0 => format!("{days}天{hours}小时"),
        Language::ZhCn if total_hours > 0 => format!("{total_hours}小时"),
        Language::ZhCn => format!("{minutes}分钟"),
        Language::EnUs if days > 0 => format!("{days}d {hours}h"),
        Language::EnUs if total_hours > 0 => format!("{total_hours}h"),
        Language::EnUs => format!("{minutes}m"),
    }
}

fn parse_compact_duration(value: &str) -> Option<i64> {
    let mut seconds = 0_i64;
    let mut found = false;
    for token in value.split_whitespace() {
        let (number, multiplier) = if let Some(number) = token.strip_suffix('d') {
            (number, 24 * 60 * 60)
        } else if let Some(number) = token.strip_suffix('h') {
            (number, 60 * 60)
        } else if let Some(number) = token.strip_suffix('m') {
            (number, 60)
        } else {
            continue;
        };
        if let Ok(number) = number.parse::<i64>() {
            seconds = seconds.saturating_add(number.saturating_mul(multiplier));
            found = true;
        }
    }
    found.then_some(seconds)
}

fn localized_reset_duration(window: &RateWindow, language: Language) -> String {
    if let Some(resets_at) = window.resets_at {
        return format_reset_duration(resets_at - Utc::now().timestamp(), language);
    }
    parse_compact_duration(&window.reset_label)
        .map(|seconds| format_reset_duration(seconds, language))
        .unwrap_or_else(|| window.reset_label.clone())
}

fn manual_reset_count_label(count: u32, language: Language) -> String {
    match language {
        Language::ZhCn => format!("{} {count} 次", tr(language, Text::ManualResetsLeft)),
        Language::EnUs => format!("{}: {count}", tr(language, Text::ManualResetsLeft)),
    }
}

fn project_chart_entries(
    snapshot: &UsageSnapshot,
    palette: TextPalette,
    language: Language,
) -> Vec<(String, u64, Color32)> {
    let colors = [
        palette.card_accent,
        Color32::from_rgb(255, 187, 92),
        Color32::from_rgb(108, 174, 255),
        Color32::from_rgb(235, 124, 173),
    ];
    let nonzero: Vec<_> = snapshot
        .project_usage
        .iter()
        .filter(|project| project.tokens > 0)
        .collect();
    if nonzero.len() <= 4 {
        return nonzero
            .into_iter()
            .enumerate()
            .map(|(index, project)| (project.name.clone(), project.tokens, colors[index]))
            .collect();
    }

    let mut entries: Vec<_> = nonzero
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, project)| (project.name.clone(), project.tokens, colors[index]))
        .collect();
    let other_tokens = nonzero
        .iter()
        .skip(3)
        .fold(0_u64, |total, project| total.saturating_add(project.tokens));
    entries.push((
        tr(language, Text::OtherProject).to_owned(),
        other_tokens,
        colors[3],
    ));
    entries
}

fn paint_project_donut(
    ui: &egui::Ui,
    rect: Rect,
    projects: &[(String, u64, Color32)],
    palette: TextPalette,
    language: Language,
) {
    let total = projects
        .iter()
        .fold(0_u64, |sum, (_, tokens, _)| sum.saturating_add(*tokens));
    let center = rect.center() - Vec2::new(0.0, 1.0);
    let radius = 31.0;
    ui.painter().circle_stroke(
        center,
        radius,
        Stroke::new(9.0, Color32::from_white_alpha(38)),
    );
    if total > 0 {
        let mut start = -std::f32::consts::FRAC_PI_2;
        let gap = 0.035;
        for (_, tokens, color) in projects {
            let sweep = std::f32::consts::TAU * (*tokens as f32 / total as f32);
            let visible_sweep = (sweep - gap).max(0.012);
            let steps = ((visible_sweep * 18.0).ceil() as usize).max(5);
            let points = (0..=steps)
                .map(|step| {
                    let angle = start + gap * 0.5 + visible_sweep * step as f32 / steps as f32;
                    center + Vec2::angled(angle) * radius
                })
                .collect();
            ui.painter()
                .add(Shape::line(points, Stroke::new(9.0, *color)));
            start += sweep;
        }
    }
    paint_shadowed_text(
        ui.painter(),
        center - Vec2::new(0.0, 3.0),
        Align2::CENTER_CENTER,
        compact_number(total),
        FontId::proportional(14.0),
        palette.card_primary,
        TextSurface::Glass,
    );
    paint_shadowed_text(
        ui.painter(),
        center + Vec2::new(0.0, 12.0),
        Align2::CENTER_CENTER,
        match language {
            Language::ZhCn => "Token",
            Language::EnUs => "tokens",
        },
        FontId::proportional(8.5),
        palette.card_muted,
        TextSurface::Glass,
    );
}

fn project_legend_row(
    ui: &mut egui::Ui,
    name: &str,
    tokens: u64,
    color: Color32,
    palette: TextPalette,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 15.0), Sense::hover());
    ui.painter()
        .circle_filled(Pos2::new(rect.left() + 4.0, rect.center().y), 3.2, color);
    paint_shadowed_text(
        ui.painter(),
        Pos2::new(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        truncate_chart_label(name, 17),
        FontId::proportional(9.2),
        palette.card_primary,
        TextSurface::Glass,
    );
    paint_shadowed_text(
        ui.painter(),
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        compact_number(tokens),
        FontId::proportional(9.2),
        palette.card_muted,
        TextSurface::Glass,
    );
}

fn truncate_chart_label(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn paint_daily_token_chart(
    ui: &mut egui::Ui,
    rect: Rect,
    days: &[DailyUsage],
    palette: TextPalette,
) {
    if days.is_empty() {
        return;
    }
    let maximum = days.iter().map(|day| day.tokens).max().unwrap_or(0);
    let slot_width = rect.width() / days.len() as f32;
    let baseline = rect.bottom() - 14.0;
    let chart_height = 29.0;
    ui.painter().line_segment(
        [
            Pos2::new(rect.left(), baseline + 0.5),
            Pos2::new(rect.right(), baseline + 0.5),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(48)),
    );
    for (index, day) in days.iter().enumerate() {
        let center_x = rect.left() + slot_width * (index as f32 + 0.5);
        let ratio = if maximum == 0 {
            0.0
        } else {
            day.tokens as f32 / maximum as f32
        };
        let height = if day.tokens == 0 {
            2.0
        } else {
            (chart_height * ratio).max(4.0)
        };
        let bar_rect = Rect::from_min_max(
            Pos2::new(center_x - slot_width * 0.24, baseline - height),
            Pos2::new(center_x + slot_width * 0.24, baseline),
        );
        ui.painter().rect_filled(
            bar_rect,
            CornerRadius::same(3),
            if day.tokens == 0 {
                Color32::from_white_alpha(42)
            } else {
                palette.card_accent
            },
        );
        paint_shadowed_text(
            ui.painter(),
            Pos2::new(center_x, (bar_rect.top() - 2.0).max(rect.top() + 5.0)),
            Align2::CENTER_BOTTOM,
            compact_number(day.tokens),
            FontId::proportional(7.7),
            palette.card_primary,
            TextSurface::Glass,
        );
        let day_label = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
            .map(|date| date.day().to_string())
            .unwrap_or_else(|_| day.date.clone());
        paint_shadowed_text(
            ui.painter(),
            Pos2::new(center_x, rect.bottom() - 1.0),
            Align2::CENTER_BOTTOM,
            day_label,
            FontId::proportional(8.0),
            palette.card_muted,
            TextSurface::Glass,
        );
        let response = ui.interact(
            Rect::from_min_max(
                Pos2::new(center_x - slot_width * 0.5, rect.top()),
                Pos2::new(center_x + slot_width * 0.5, rect.bottom()),
            ),
            ui.id().with(("daily-token-bar", index)),
            Sense::hover(),
        );
        response.on_hover_text(format!("{} · {} tokens", day.date, day.tokens));
    }
}

fn weather_icon_source(
    theme: WeatherIconTheme,
    qweather_code: Option<&str>,
    wmo_code: Option<u16>,
) -> egui::ImageSource<'static> {
    crate::generated_weather_icons::weather_icon_source(theme, qweather_code, wmo_code)
}

fn scene_icon_source(theme: WeatherIconTheme, scene_index: usize) -> egui::ImageSource<'static> {
    match scene_index {
        RAIN_SCENE => weather_icon_source(theme, Some("306"), Some(63)),
        THUNDER_SCENE => weather_icon_source(theme, Some("302"), Some(95)),
        SNOW_SCENE => weather_icon_source(theme, Some("401"), Some(73)),
        CLEAR_SCENE => weather_icon_source(theme, Some("100"), Some(0)),
        CLOUDY_SCENE => weather_icon_source(theme, Some("101"), Some(2)),
        OVERCAST_SCENE => weather_icon_source(theme, Some("104"), Some(3)),
        FOG_SCENE => weather_icon_source(theme, Some("501"), Some(45)),
        _ => weather_icon_source(theme, Some("999"), None),
    }
}

fn scene_label(scene_index: usize, language: Language) -> &'static str {
    let weather_code = match scene_index {
        RAIN_SCENE => Some(61),
        THUNDER_SCENE => Some(95),
        SNOW_SCENE => Some(71),
        CLEAR_SCENE => Some(0),
        CLOUDY_SCENE => Some(2),
        OVERCAST_SCENE => Some(3),
        FOG_SCENE => Some(45),
        _ => None,
    };
    condition_label(weather_code, language)
}

fn cover_uv(image_size: Vec2, target_size: Vec2) -> Rect {
    let image_aspect = image_size.x / image_size.y.max(1.0);
    let target_aspect = target_size.x / target_size.y.max(1.0);
    if image_aspect > target_aspect {
        let visible = target_aspect / image_aspect;
        let margin = (1.0 - visible) * 0.5;
        Rect::from_min_max(Pos2::new(margin, 0.0), Pos2::new(1.0 - margin, 1.0))
    } else {
        let visible = image_aspect / target_aspect;
        let margin = (1.0 - visible) * 0.5;
        Rect::from_min_max(Pos2::new(0.0, margin), Pos2::new(1.0, 1.0 - margin))
    }
}

fn desktop_glass_uv(texture_size: Vec2, sample: crate::desktop::DesktopSample) -> Rect {
    let monitor_size = Vec2::new(sample.monitor_size[0], sample.monitor_size[1]);
    let monitor_uv = cover_uv(texture_size, monitor_size);
    let [left, top, right, bottom] = sample.window_fraction;
    Rect::from_min_max(
        Pos2::new(
            monitor_uv.left() + monitor_uv.width() * left,
            monitor_uv.top() + monitor_uv.height() * top,
        ),
        Pos2::new(
            monitor_uv.left() + monitor_uv.width() * right,
            monitor_uv.top() + monitor_uv.height() * bottom,
        ),
    )
}

fn paint_weather_particles(
    painter: &egui::Painter,
    rect: Rect,
    effects: weather::WeatherEffects,
    time: f32,
    reduced_motion: bool,
) {
    if effects.rain > 0 {
        paint_rain_drops(painter, rect, time, reduced_motion, effects.rain);
    }
    if effects.snow > 0 {
        paint_snow(painter, rect, time, reduced_motion, effects.snow);
    }
    if effects.lightning && !reduced_motion {
        let phase = (time / 9.7).fract();
        let flash = lightning_pulse(phase, 0.78, 0.025).max(lightning_pulse(phase, 0.84, 0.014));
        if flash > 0.0 {
            painter.rect_filled(
                rect,
                CornerRadius::same(WINDOW_CORNER_RADIUS),
                Color32::from_rgba_unmultiplied(210, 225, 255, (flash * 72.0) as u8),
            );
        }
    }
}

fn effects_for_manual_scene(scene_index: usize) -> weather::WeatherEffects {
    match scene_index {
        RAIN_SCENE => weather::WeatherEffects {
            rain: 2,
            ..weather::WeatherEffects::default()
        },
        THUNDER_SCENE => weather::WeatherEffects {
            rain: 2,
            lightning: true,
            ..weather::WeatherEffects::default()
        },
        SNOW_SCENE => weather::WeatherEffects {
            snow: 2,
            ..weather::WeatherEffects::default()
        },
        _ => weather::WeatherEffects::default(),
    }
}

fn lightning_pulse(phase: f32, center: f32, half_width: f32) -> f32 {
    (1.0 - ((phase - center).abs() / half_width)).clamp(0.0, 1.0)
}

fn paint_rain_drops(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    reduced_motion: bool,
    intensity: u8,
) {
    let glass = rect.shrink(4.0);
    let count = if reduced_motion {
        [0, 5, 8, 11][intensity.min(3) as usize]
    } else {
        [0, 14, 24, 34][intensity.min(3) as usize]
    };
    for i in 0..count {
        let seed = ((i * 47 + 19) % 101) as f32 / 101.0;
        let seed_b = ((i * 71 + 7) % 97) as f32 / 97.0;
        let duration = 4.6 + seed_b * 4.2;
        let phase = (time / duration + seed).fract();
        let impact_end = 0.16 + seed_b * 0.035;
        let slide_start = impact_end * 0.72;
        let slide_end = 0.68 + seed * 0.075;
        let hold_end = 0.84 + seed_b * 0.055;
        let slide_phase = ((phase - slide_start) / (slide_end - slide_start)).clamp(0.0, 1.0);
        let fade = if phase < 0.06 {
            phase / 0.06
        } else if phase > hold_end {
            ((1.0 - phase) / (1.0 - hold_end)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let x = glass.left() + 12.0 + seed_b * (glass.width() - 24.0);
        let start_y = glass.top() + 12.0 + seed * (glass.height() * 0.58);
        let travel = (48.0 + seed_b * 92.0).min(glass.bottom() - start_y - 7.0);
        let radius = 1.55 + (i % 5) as f32 * 0.42;
        let position_at = |progress: f32| {
            let gravity = progress.clamp(0.0, 1.0).powf(1.62);
            let drift_strength = 0.72 + seed_b * 1.35;
            let drift = gravity
                * ((gravity * 7.3 + seed * 11.0).sin()
                    + (gravity * 16.7 + seed_b * 9.0).sin() * 0.38)
                * drift_strength;
            Pos2::new(x + drift, start_y + gravity * travel)
        };
        let center = position_at(slide_phase);
        let head_radius = radius * (1.0 - slide_phase * 0.56);

        if phase < impact_end {
            let impact = phase / impact_end;
            painter.circle_stroke(
                Pos2::new(x, start_y),
                radius + impact * 6.0,
                Stroke::new(
                    0.7,
                    Color32::from_white_alpha(((1.0 - impact).powi(2) * 112.0) as u8),
                ),
            );
        }

        if slide_phase > 0.0 {
            // The wet trace stays anchored at the impact point and grows down
            // towards the shrinking head. Older, upper sections are fainter.
            let segments = 14;
            for segment in 0..segments {
                let from_phase = slide_phase * segment as f32 / segments as f32;
                let to_phase = slide_phase * (segment + 1) as f32 / segments as f32;
                let freshness = (segment + 1) as f32 / segments as f32;
                let trail_alpha = (fade * (0.18 + freshness.powf(1.45) * 0.82) * 64.0) as u8;
                let from = position_at(from_phase);
                let to = position_at(to_phase);
                let from_width = (radius * (1.0 - from_phase * 0.56) * 0.62).max(0.28);
                let to_width = (radius * (1.0 - to_phase * 0.56) * 0.62).max(0.28);
                if trail_alpha > 0 {
                    painter.add(Shape::convex_polygon(
                        vec![
                            from - Vec2::new(from_width, 0.0),
                            from + Vec2::new(from_width, 0.0),
                            to + Vec2::new(to_width, 0.0),
                            to - Vec2::new(to_width, 0.0),
                        ],
                        Color32::from_white_alpha(trail_alpha / 7),
                        Stroke::NONE,
                    ));
                    painter.line_segment(
                        [
                            from - Vec2::new(from_width, 0.0),
                            to - Vec2::new(to_width, 0.0),
                        ],
                        Stroke::new(0.48, Color32::from_white_alpha(trail_alpha)),
                    );
                    painter.line_segment(
                        [
                            from + Vec2::new(from_width, 0.0),
                            to + Vec2::new(to_width, 0.0),
                        ],
                        Stroke::new(0.48, Color32::from_white_alpha(trail_alpha)),
                    );
                }
            }
        }

        let body_alpha = (fade * 76.0) as u8;
        let edge_alpha = (fade * 122.0) as u8;
        let glint_alpha = (fade * 190.0) as u8;
        painter.circle_filled(
            center + Vec2::new(0.45, 0.65),
            head_radius + 0.45,
            Color32::from_black_alpha((fade * 30.0) as u8),
        );
        painter.circle(
            center,
            head_radius,
            Color32::from_white_alpha(body_alpha),
            Stroke::new(0.65, Color32::from_white_alpha(edge_alpha)),
        );
        painter.circle_filled(
            center - Vec2::new(head_radius * 0.35, head_radius * 0.42),
            (head_radius * 0.25).max(0.34),
            Color32::from_white_alpha(glint_alpha),
        );
    }
}

fn paint_snow(painter: &egui::Painter, rect: Rect, time: f32, reduced_motion: bool, intensity: u8) {
    let count = if reduced_motion {
        [0, 8, 13, 18][intensity.min(3) as usize]
    } else {
        [0, 24, 42, 58][intensity.min(3) as usize]
    };
    for i in 0..count {
        let seed = i as f32 * 47.37;
        let depth = 0.35 + (i % 8) as f32 / 11.0;
        let x = rect.left() + (seed * 1.91 + (time * 0.8 + seed).sin() * 8.0) % rect.width();
        let y = rect.top() + (seed * 1.29 + time * (8.0 + depth * 18.0)) % rect.height();
        painter.circle_filled(
            Pos2::new(x, y),
            0.9 + depth * 2.5,
            Color32::from_white_alpha((105.0 + depth * 125.0) as u8),
        );
    }
}

fn svg_icon_button(
    ui: &mut egui::Ui,
    source: egui::ImageSource<'static>,
    tooltip: &str,
    visibility: f32,
) -> egui::Response {
    let visibility = visibility.clamp(0.0, 1.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let image_rect = Rect::from_center_size(
        rect.center() + Vec2::new(0.0, (1.0 - visibility) * 4.0),
        Vec2::splat(19.0),
    );
    egui::Image::new(source)
        .fit_to_exact_size(image_rect.size())
        .tint(Color32::from_white_alpha((visibility * 255.0).round() as u8))
        .paint_at(ui, image_rect);
    if visibility >= 0.55 {
        response.on_hover_text(tooltip)
    } else {
        response
    }
}

fn qweather_help_button(ui: &mut egui::Ui, tooltip: &str, accent: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
    let color = if response.hovered() {
        accent
    } else {
        accent.gamma_multiply(0.78)
    };
    let center = rect.center() + Vec2::new(0.0, -0.5);
    ui.painter()
        .circle(center, 6.0, Color32::TRANSPARENT, Stroke::new(1.0, color));
    ui.painter().text(
        center + Vec2::new(0.0, -0.2),
        Align2::CENTER_CENTER,
        "?",
        FontId::proportional(10.5),
        color,
    );
    response.on_hover_text(tooltip)
}

fn svg_icon_button_at(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &'static str,
    source: egui::ImageSource<'static>,
    tooltip: &str,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    if response.hovered() {
        ui.painter().circle_filled(
            rect.center(),
            13.0,
            Color32::from_white_alpha(if response.is_pointer_button_down_on() {
                44
            } else {
                28
            }),
        );
    }
    let image_rect = Rect::from_center_size(rect.center(), Vec2::splat(19.0));
    egui::Image::new(source)
        .fit_to_exact_size(image_rect.size())
        .paint_at(ui, image_rect);
    response.on_hover_text(tooltip)
}

fn apply_settings_visuals(ui: &mut egui::Ui, palette: TextPalette) {
    apply_settings_style(ui.style_mut(), palette, false);
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 7.0);
    ui.spacing_mut().button_padding = Vec2::new(11.0, 6.0);
}

fn settings_popup_style(palette: TextPalette) -> egui::style::StyleModifier {
    egui::style::StyleModifier::new(move |style| {
        egui::containers::menu::menu_style(style);
        apply_settings_style(style, palette, true);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.item_spacing = Vec2::new(3.0, 3.0);
        style.spacing.button_padding = Vec2::new(8.0, 5.0);
    })
}

fn apply_settings_style(style: &mut egui::Style, palette: TextPalette, popup: bool) {
    let light_text = relative_luminance(palette.card_primary) >= 0.5;
    let control_fill = if light_text {
        Color32::from_black_alpha(92)
    } else {
        Color32::from_white_alpha(126)
    };
    let control_hover = if light_text {
        Color32::from_black_alpha(126)
    } else {
        Color32::from_white_alpha(166)
    };
    let control_active = with_alpha(palette.card_accent, 112);
    let accent_hsl = rgb_to_hsl(palette.card_accent);
    let menu_highlight = hsl_to_color(Hsl {
        hue: accent_hsl.hue,
        saturation: accent_hsl.saturation.max(0.32),
        lightness: if light_text { 0.28 } else { 0.78 },
    });
    let edge = if light_text {
        Color32::from_white_alpha(72)
    } else {
        Color32::from_black_alpha(44)
    };

    let visuals = &mut style.visuals;
    visuals.dark_mode = light_text;
    visuals.override_text_color = Some(palette.card_primary);
    visuals.weak_text_color = Some(palette.card_muted);
    visuals.extreme_bg_color = control_fill;
    visuals.text_edit_bg_color = Some(control_fill);
    visuals.faint_bg_color = control_fill;
    visuals.selection.bg_fill = with_alpha(palette.card_accent, 138);
    visuals.selection.stroke = Stroke::new(1.0, palette.card_primary);
    visuals.window_fill = if popup {
        if light_text {
            Color32::from_rgba_unmultiplied(25, 31, 35, 242)
        } else {
            Color32::from_rgba_unmultiplied(244, 241, 233, 242)
        }
    } else {
        with_alpha(palette.glass.composite, 244)
    };
    visuals.window_stroke = Stroke::new(1.0, edge);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.popup_shadow = Shadow {
        offset: [7, 8],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(92),
    };

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
    ] {
        widget.bg_fill = control_fill;
        widget.weak_bg_fill = control_fill;
        widget.bg_stroke = Stroke::new(1.0, edge);
        widget.fg_stroke = Stroke::new(1.2, palette.card_primary);
        widget.corner_radius = CornerRadius::same(7);
    }
    visuals.widgets.hovered.bg_fill = control_hover;
    visuals.widgets.hovered.weak_bg_fill = control_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.card_accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.3, palette.card_primary);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    visuals.widgets.active.bg_fill = control_active;
    visuals.widgets.active.weak_bg_fill = control_active;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.card_accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.3, palette.card_primary);
    visuals.widgets.active.corner_radius = CornerRadius::same(7);
    visuals.widgets.open = visuals.widgets.hovered;

    if popup {
        visuals.widgets.inactive.bg_fill = visuals.window_fill;
        visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.bg_fill = with_alpha(menu_highlight, 190);
        visuals.widgets.hovered.weak_bg_fill = with_alpha(menu_highlight, 190);
        visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        visuals.widgets.active.bg_fill = with_alpha(menu_highlight, 220);
        visuals.widgets.active.weak_bg_fill = with_alpha(menu_highlight, 220);
        visuals.widgets.active.bg_stroke = Stroke::NONE;
        visuals.selection.bg_fill = with_alpha(menu_highlight, 232);
    }
}

fn paint_ring(ui: &egui::Ui, rect: Rect, progress: f32, start_color: Color32, end_color: Color32) {
    let center = rect.center();
    let radius = rect.width() * 0.42;
    let stroke_width = (rect.width() * 0.073).clamp(4.5, 7.0);
    ui.painter().circle_stroke(
        center,
        radius,
        Stroke::new(stroke_width + 5.0, with_alpha(start_color, 24)),
    );
    ui.painter().circle_stroke(
        center,
        radius,
        Stroke::new(
            stroke_width,
            with_alpha(blend_rgb(start_color, end_color, 0.5), 72),
        ),
    );
    ui.painter().circle_stroke(
        center,
        radius - stroke_width * 0.78,
        Stroke::new(1.0, Color32::from_white_alpha(35)),
    );
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * progress.clamp(0.0, 1.0);
    let glow_points: Vec<Pos2> = (0..=64)
        .map(|index| {
            let angle = start + sweep * index as f32 / 64.0;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    ui.painter().add(Shape::line(
        glow_points,
        Stroke::new(stroke_width + 3.0, with_alpha(start_color, 34)),
    ));

    const SEGMENTS: usize = 56;
    for index in 0..SEGMENTS {
        let start_t = index as f32 / SEGMENTS as f32;
        if start_t >= progress {
            break;
        }
        let end_t = ((index + 1) as f32 / SEGMENTS as f32).min(progress);
        let color_t = (start_t / progress.max(f32::EPSILON)).clamp(0.0, 1.0);
        let color = blend_rgb(start_color, end_color, color_t);
        let point_a = center + Vec2::angled(start + std::f32::consts::TAU * start_t) * radius;
        let point_b = center + Vec2::angled(start + std::f32::consts::TAU * end_t) * radius;
        ui.painter()
            .line_segment([point_a, point_b], Stroke::new(stroke_width, color));
    }

    if sweep > 0.0 {
        let endpoint = center + Vec2::angled(start + sweep) * radius;
        ui.painter()
            .circle_filled(endpoint, stroke_width * 0.82, with_alpha(end_color, 42));
        ui.painter()
            .circle_filled(endpoint, stroke_width * 0.43, end_color);
        ui.painter().circle_filled(
            endpoint - Vec2::splat(stroke_width * 0.12),
            stroke_width * 0.13,
            Color32::from_white_alpha(220),
        );
    }
}

fn paint_quota_ring(
    ui: &egui::Ui,
    rect: Rect,
    remaining_percent: u8,
    label: &str,
    palette: TextPalette,
    compact: bool,
) {
    let (cool_tone, warm_tone) = semantic_card_tones(palette);
    paint_ring(
        ui,
        rect,
        f32::from(remaining_percent) / 100.0,
        cool_tone,
        warm_tone,
    );
    let value_size = if compact { 18.0 } else { 26.0 };
    let label_size = if compact { 8.0 } else { 10.5 };
    let value_offset = if compact { 7.0 } else { 10.0 };
    let label_offset = if compact { 12.0 } else { 15.0 };
    paint_shadowed_text(
        ui.painter(),
        rect.center() - Vec2::new(0.0, value_offset),
        Align2::CENTER_CENTER,
        format!("{remaining_percent}%"),
        FontId::proportional(value_size),
        cool_tone,
        TextSurface::Glass,
    );
    paint_shadowed_text(
        ui.painter(),
        rect.center() + Vec2::new(0.0, label_offset),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(label_size),
        warm_tone,
        TextSurface::Glass,
    );
}

fn frosted_panel<R>(
    ui: &mut egui::Ui,
    context: FrostedContext<'_>,
    glass: GlassPalette,
    corner_radius: u8,
    inner_margin: Margin,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let background = ui.painter().add(Shape::Noop);
    let response = Frame::NONE
        .inner_margin(inner_margin)
        .show(ui, add_contents);
    let rect = response.response.rect;
    let radius = CornerRadius::same(corner_radius);
    let mut shapes = Vec::with_capacity(5);
    shapes.push(
        Shadow {
            // With a top-left light source the shadow may only escape on the
            // bottom and right. The offset is larger than half the blur width,
            // so no penumbra reaches the top or left edges.
            offset: [6, 8],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(96),
        }
        .as_shape(rect, radius)
        .into(),
    );
    if let Some(texture) = context.texture {
        shapes.push(
            RectShape::filled(
                rect,
                radius,
                Color32::from_white_alpha(
                    (context.texture_opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
            )
            .with_texture(texture.id(), panel_uv(texture, context.scene_rect, rect))
            .into(),
        );
    }
    shapes.push(RectShape::filled(rect, radius, with_alpha(glass.tint, glass.tint_alpha)).into());
    let edge_inset = f32::from(corner_radius) * 0.72;
    let arc_radius = (f32::from(corner_radius) - 1.0).max(1.0);
    let arc_center = Pos2::new(
        rect.left() + f32::from(corner_radius),
        rect.top() + f32::from(corner_radius),
    );
    let mut highlight = Vec::with_capacity(10);
    highlight.push(Pos2::new(rect.left() + 1.0, rect.bottom() - edge_inset));
    for step in 0..=7 {
        let angle = std::f32::consts::PI + std::f32::consts::FRAC_PI_2 * step as f32 / 7.0;
        highlight.push(arc_center + Vec2::angled(angle) * arc_radius);
    }
    highlight.push(Pos2::new(rect.right() - edge_inset, rect.top() + 1.0));
    shapes.push(Shape::line(
        highlight.clone(),
        Stroke::new(3.0, Color32::from_white_alpha(48)),
    ));
    shapes.push(Shape::line(
        highlight,
        Stroke::new(1.1, Color32::from_white_alpha(166)),
    ));
    ui.painter().set(background, Shape::Vec(shapes));
    response
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn hits_unobstructed_background(
    position: Pos2,
    background_rect: Rect,
    foreground_rects: &[Rect],
    overlay_open: bool,
) -> bool {
    !overlay_open
        && background_rect.contains(position)
        && foreground_rects
            .iter()
            .all(|foreground| !foreground.contains(position))
}

fn shadowed_label(
    ui: &mut egui::Ui,
    text: RichText,
    color: Color32,
    surface: TextSurface,
) -> egui::Response {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    paint_shadowed_galley(ui.painter(), rect.min, galley, color, surface);
    response
}

fn shadowed_truncated_label(
    ui: &mut egui::Ui,
    text: RichText,
    color: Color32,
    surface: TextSurface,
    max_width: f32,
) -> egui::Response {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        max_width.max(0.0),
        egui::TextStyle::Body,
    );
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    paint_shadowed_galley(ui.painter(), rect.min, galley, color, surface);
    response
}

#[derive(Clone, Copy)]
struct TextShadowLayer {
    offset: Vec2,
    color: Color32,
}

fn text_shadow_layers(surface: TextSurface) -> [TextShadowLayer; 2] {
    let (warm_alpha, cool_alpha) = match surface {
        TextSurface::Photo => (150, 138),
        TextSurface::Glass => (96, 84),
    };
    [
        TextShadowLayer {
            offset: Vec2::new(0.7, 0.85),
            color: Color32::from_rgba_unmultiplied(82, 61, 64, warm_alpha),
        },
        TextShadowLayer {
            offset: Vec2::new(-0.45, -0.45),
            color: Color32::from_rgba_unmultiplied(60, 70, 84, cool_alpha),
        },
    ]
}

fn paint_text_shadow_layers(
    painter: &egui::Painter,
    position: Pos2,
    galley: &std::sync::Arc<egui::Galley>,
    surface: TextSurface,
) {
    for layer in text_shadow_layers(surface) {
        painter.galley_with_override_text_color(
            position + layer.offset,
            galley.clone(),
            layer.color,
        );
    }
}

fn paint_shadowed_galley(
    painter: &egui::Painter,
    position: Pos2,
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
    surface: TextSurface,
) {
    paint_text_shadow_layers(painter, position, &galley, surface);
    painter.galley_with_override_text_color(position, galley, color);
}

fn paint_shadowed_text(
    painter: &egui::Painter,
    position: Pos2,
    anchor: Align2,
    text: impl ToString,
    font: FontId,
    color: Color32,
    surface: TextSurface,
) {
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let position = anchor
        .anchor_rect(Rect::from_min_size(position, galley.size()))
        .min;
    paint_shadowed_galley(painter, position, galley, color, surface);
}

fn panel_uv(texture: &TextureHandle, scene_rect: Rect, panel_rect: Rect) -> Rect {
    let full_uv = cover_uv(texture.size_vec2(), scene_rect.size());
    let x0 = ((panel_rect.left() - scene_rect.left()) / scene_rect.width()).clamp(0.0, 1.0);
    let y0 = ((panel_rect.top() - scene_rect.top()) / scene_rect.height()).clamp(0.0, 1.0);
    let x1 = ((panel_rect.right() - scene_rect.left()) / scene_rect.width()).clamp(0.0, 1.0);
    let y1 = ((panel_rect.bottom() - scene_rect.top()) / scene_rect.height()).clamp(0.0, 1.0);
    Rect::from_min_max(
        Pos2::new(
            full_uv.left() + x0 * full_uv.width(),
            full_uv.top() + y0 * full_uv.height(),
        ),
        Pos2::new(
            full_uv.left() + x1 * full_uv.width(),
            full_uv.top() + y1 * full_uv.height(),
        ),
    )
}

#[derive(Clone, Copy)]
enum MetricTone {
    Cool,
    Warm,
}

struct MetricCardContent<'a> {
    label: &'a str,
    value: String,
    caption: String,
    tone: MetricTone,
}

fn semantic_tone(background: Color32, hue: f32) -> Color32 {
    let dark_background = relative_luminance(background) < 0.52;
    let candidates = if dark_background {
        [0.7, 0.8, 0.9]
    } else {
        [0.46, 0.36, 0.28]
    };
    candidates
        .into_iter()
        .map(|lightness| {
            hsl_to_color(Hsl {
                hue,
                saturation: 0.72,
                lightness,
            })
        })
        .max_by(|first, second| {
            contrast_ratio(*first, background).total_cmp(&contrast_ratio(*second, background))
        })
        .expect("semantic tone candidates are never empty")
}

fn semantic_card_tones(palette: TextPalette) -> (Color32, Color32) {
    (
        semantic_tone(palette.glass.composite, 198.0),
        semantic_tone(palette.glass.composite, 14.0),
    )
}

fn metric_card(
    ui: &mut egui::Ui,
    width: f32,
    content: MetricCardContent<'_>,
    palette: TextPalette,
    glass: FrostedContext<'_>,
) {
    let (cool_tone, warm_tone) = semantic_card_tones(palette);
    let accent = match content.tone {
        MetricTone::Cool => cool_tone,
        MetricTone::Warm => warm_tone,
    };
    let label_color = blend_rgb(palette.card_muted, accent, 0.82);
    let caption_color = blend_rgb(palette.card_muted, accent, 0.92);
    frosted_panel(ui, glass, palette.glass, 14, Margin::same(12), |ui| {
        ui.set_min_width(width - 24.0);
        ui.vertical(|ui| {
            shadowed_label(
                ui,
                RichText::new(content.label).size(10.5).strong(),
                label_color,
                TextSurface::Glass,
            );
            shadowed_label(
                ui,
                RichText::new(content.value).size(22.0).strong(),
                accent,
                TextSurface::Glass,
            );
            shadowed_label(
                ui,
                RichText::new(content.caption).size(10.0),
                caption_color,
                TextSurface::Glass,
            );
        });
    });
}

fn compact_number(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod quota_tests {
    use super::*;

    fn window(minutes: u64, label: &str) -> RateWindow {
        RateWindow {
            window_minutes: Some(minutes),
            window_label: label.to_owned(),
            ..RateWindow::default()
        }
    }

    #[test]
    fn orders_five_hour_before_weekly_window() {
        let five_hour = window(300, "5h");
        let weekly = window(10_080, "weekly");
        assert_eq!(quota_window_order(&five_hour), 0);
        assert_eq!(quota_window_order(&weekly), 1);
        assert_eq!(quota_window_name(&five_hour, Language::ZhCn), "5小时");
        assert_eq!(quota_window_name(&weekly, Language::ZhCn), "7天");
        assert_eq!(quota_window_name(&weekly, Language::EnUs), "7 days");
    }

    #[test]
    fn reset_duration_is_localized_without_changing_english_abbreviations() {
        let seconds = 5 * 24 * 60 * 60 + 16 * 60 * 60;
        assert_eq!(format_reset_duration(seconds, Language::ZhCn), "5天16小时");
        assert_eq!(format_reset_duration(seconds, Language::EnUs), "5d 16h");
        assert_eq!(parse_compact_duration("5d 16h"), Some(seconds));
    }

    #[test]
    fn text_hint_is_hidden_as_soon_as_the_empty_field_has_focus() {
        assert!(should_show_text_hint("", false));
        assert!(!should_show_text_hint("", true));
        assert!(!should_show_text_hint("saved.example.com", false));
    }

    #[test]
    fn truncated_shadowed_label_never_exceeds_its_available_width() {
        let context = egui::Context::default();
        let mut rendered_width = 0.0;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            rendered_width = shadowed_truncated_label(
                ui,
                RichText::new(
                    "正在专注  dear-dmit-customer-dmit-proactive-sequence-with-a-long-name",
                )
                .size(11.5),
                Color32::WHITE,
                TextSurface::Photo,
                96.0,
            )
            .rect
            .width();
        });
        output.textures_delta.clear();

        assert!(rendered_width <= 96.0);
    }

    #[test]
    fn qweather_help_is_visible_only_for_providers_that_use_qweather() {
        assert!(should_show_qweather_help(WeatherProvider::Auto));
        assert!(should_show_qweather_help(WeatherProvider::QWeather));
        assert!(!should_show_qweather_help(WeatherProvider::OpenMeteo));
        assert!(!qweather_help_url(Language::ZhCn).contains("/en/"));
        assert!(qweather_help_url(Language::EnUs).contains("/en/"));
    }

    #[test]
    fn manual_reset_count_is_explicitly_labeled() {
        assert_eq!(
            manual_reset_count_label(1, Language::ZhCn),
            "剩余手动重置 1 次"
        );
        assert_eq!(
            manual_reset_count_label(2, Language::EnUs),
            "Manual resets left: 2"
        );
    }

    #[test]
    fn background_cycle_ignores_foreground_content_and_overlays() {
        let background = Rect::from_min_max(Pos2::ZERO, Pos2::new(300.0, 260.0));
        let date = Rect::from_min_max(Pos2::new(20.0, 30.0), Pos2::new(220.0, 120.0));
        let week = Rect::from_min_max(Pos2::new(20.0, 135.0), Pos2::new(280.0, 190.0));
        let quota = Rect::from_min_max(Pos2::new(20.0, 205.0), Pos2::new(280.0, 300.0));
        let foreground = [date, week, quota];

        assert!(hits_unobstructed_background(
            Pos2::new(285.0, 100.0),
            background,
            &foreground,
            false,
        ));
        assert!(!hits_unobstructed_background(
            Pos2::new(100.0, 160.0),
            background,
            &foreground,
            false,
        ));
        assert!(!hits_unobstructed_background(
            Pos2::new(100.0, 230.0),
            background,
            &foreground,
            false,
        ));
        assert!(!hits_unobstructed_background(
            Pos2::new(285.0, 100.0),
            background,
            &foreground,
            true,
        ));
    }

    #[test]
    fn desktop_glass_uv_tracks_the_window_inside_its_monitor() {
        let sample = crate::desktop::DesktopSample {
            monitor_size: [1920.0, 1080.0],
            window_fraction: [0.25, 0.20, 0.50, 0.60],
        };
        let uv = desktop_glass_uv(Vec2::new(960.0, 540.0), sample);
        assert!((uv.left() - 0.25).abs() < f32::EPSILON);
        assert!((uv.top() - 0.20).abs() < f32::EPSILON);
        assert!((uv.right() - 0.50).abs() < f32::EPSILON);
        assert!((uv.bottom() - 0.60).abs() < f32::EPSILON);
    }

    #[test]
    fn dynamic_palette_keeps_primary_text_readable() {
        let dark = Color32::from_rgb(24, 32, 38);
        let light = Color32::from_rgb(224, 232, 236);
        let dark_palette = text_palette(dark);
        let light_palette = text_palette(light);
        assert!(contrast_ratio(dark_palette.photo_primary, dark) >= 4.5);
        assert!(contrast_ratio(light_palette.photo_primary, light) >= 4.5);
        assert!(relative_luminance(dark_palette.photo_primary) > relative_luminance(dark));
        assert!(relative_luminance(light_palette.photo_primary) < relative_luminance(light));
        for color in [
            dark_palette.photo_primary,
            dark_palette.photo_muted,
            dark_palette.photo_accent,
        ] {
            assert!(contrast_ratio(color, dark) >= 4.5);
        }
        for color in [
            light_palette.photo_primary,
            light_palette.photo_muted,
            light_palette.photo_accent,
        ] {
            assert!(contrast_ratio(color, light) >= 4.5);
        }
        for color in [
            dark_palette.card_primary,
            dark_palette.card_muted,
            dark_palette.card_accent,
        ] {
            assert!(contrast_ratio(color, dark_palette.glass.composite) >= 4.5);
        }
        for color in [
            light_palette.card_primary,
            light_palette.card_muted,
            light_palette.card_accent,
        ] {
            assert!(contrast_ratio(color, light_palette.glass.composite) >= 4.5);
        }
    }

    #[test]
    fn semantic_card_tones_never_collapse_to_neutral_black_or_white() {
        for background in [
            Color32::from_rgb(18, 28, 34),
            Color32::from_rgb(112, 118, 124),
            Color32::from_rgb(232, 226, 218),
        ] {
            let cool = semantic_tone(background, 198.0);
            let warm = semantic_tone(background, 14.0);

            for tone in [cool, warm] {
                let hsl = rgb_to_hsl(tone);
                assert!(hsl.saturation >= 0.68, "semantic tone lost its hue");
                assert_ne!(tone, Color32::BLACK);
                assert_ne!(tone, Color32::WHITE);
            }
            assert_ne!(cool, warm, "cool and warm semantics must stay distinct");
        }
    }

    #[test]
    fn glass_tint_uses_the_backgrounds_complementary_hue() {
        let background = Color32::from_rgb(28, 92, 184);
        let source_hue = rgb_to_hsl(background).hue;
        let tint_hue = rgb_to_hsl(text_palette(background).glass.tint).hue;
        let expected = (source_hue + 180.0) % 360.0;
        let raw_difference = (tint_hue - expected).abs();
        let circular_difference = raw_difference.min(360.0 - raw_difference);
        assert!(circular_difference < 2.0);
    }

    #[test]
    fn dominant_color_uses_the_largest_hsl_bucket() {
        let mut image = image::RgbaImage::from_pixel(20, 20, image::Rgba([28, 86, 142, 255]));
        for y in 0..5 {
            for x in 0..5 {
                image.put_pixel(x, y, image::Rgba([220, 80, 40, 255]));
            }
        }
        let dominant = dominant_color(&image);
        assert!(dominant.b() > dominant.r());
        assert!(dominant.b() > dominant.g());
    }
}
