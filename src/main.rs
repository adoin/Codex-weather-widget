#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

use codex_calendar::{
    autostart,
    config::Config,
    credentials,
    i18n::{Language, Text, tr},
    live::fetch_live_usage,
    model::{AppCommand, UsageSnapshot},
    server,
    ui::{CalendarApp, install_fonts},
    usage::{UsageCollector, discover_codex_home},
    watcher,
    weather::{self, WeatherState},
};
use eframe::egui::{Pos2, Vec2, ViewportBuilder, WindowLevel};
use parking_lot::RwLock;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

fn main() -> eframe::Result {
    let config = Arc::new(RwLock::new(Config::load()));
    if config.read().auto_start
        && let Err(error) = autostart::configure(true)
    {
        eprintln!("failed to synchronize startup task: {error:#}");
    }
    let port = config.read().port;
    if server::notify_existing(port) {
        return Ok(());
    }

    let snapshot = Arc::new(RwLock::new(UsageSnapshot::default()));
    let weather_state = Arc::new(RwLock::new(WeatherState::default()));
    let visible = Arc::new(AtomicBool::new(true));
    let (commands_tx, commands_rx) = mpsc::channel();

    let _server = server::start(port, snapshot.clone(), visible.clone(), commands_tx.clone())
        .map_err(|error| {
            eframe::Error::AppCreation(Box::new(std::io::Error::other(error.to_string())))
        })?;
    let _watcher = watcher::start(config.clone(), commands_tx.clone(), visible.clone());
    start_usage_worker(config.clone(), snapshot.clone());
    start_weather_worker(config.clone(), weather_state.clone());
    let language = Language::from_code(config.read().language.as_deref());
    let _tray = setup_tray(commands_tx.clone(), language).ok();

    let cfg = config.read().clone();
    let mut viewport = ViewportBuilder::default()
        .with_title("Codex Calendar")
        .with_inner_size(Vec2::new(cfg.window_width, cfg.window_height))
        .with_min_inner_size(Vec2::new(350.0, 446.0))
        .with_decorations(false)
        .with_resizable(false)
        // Keep the native HWND hidden until the first app frame has removed
        // non-client chrome, attached it to WorkerW, and installed its HRGN.
        // Otherwise Windows can expose a default title bar during GL startup.
        .with_visible(false)
        // The photo opacity setting needs the swapchain alpha channel so the
        // desktop, rather than an opaque scene color, appears underneath it.
        .with_transparent(true)
        .with_taskbar(false)
        .with_window_level(if cfg.always_on_top && !cfg.desktop_attached {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        });
    if let (Some(x), Some(y)) = (cfg.window_x, cfg.window_y) {
        viewport = viewport.with_position(Pos2::new(x, y));
    }
    let native_options = eframe::NativeOptions {
        viewport,
        // On this Windows compositor the WGPU surface advertises no usable
        // alpha mode and silently presents transparent pixels as black. Glow
        // requests an alpha-capable GL config explicitly.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Codex Calendar",
        native_options,
        Box::new(move |creation| {
            install_fonts(&creation.egui_ctx);
            creation
                .egui_ctx
                .set_pixels_per_point(creation.egui_ctx.pixels_per_point().max(1.0));
            Ok(Box::new(CalendarApp::new(
                config,
                snapshot,
                weather_state,
                commands_rx,
                visible,
            )))
        }),
    )
}

fn start_weather_worker(config: Arc<RwLock<Config>>, state: Arc<RwLock<WeatherState>>) {
    thread::Builder::new()
        .name("weather-refresh".into())
        .spawn(move || {
            let mut active_location = None;
            let mut last_fetch = Instant::now()
                .checked_sub(Duration::from_secs(24 * 60 * 60))
                .unwrap_or_else(Instant::now);
            loop {
                let cfg = config.read().clone();
                let language = Language::from_code(cfg.language.as_deref());
                let location = cfg.weather_location.trim().to_owned();
                if location.is_empty() {
                    if active_location.is_some() || !state.read().requested_location.is_empty() {
                        *state.write() = WeatherState::default();
                    }
                    active_location = None;
                } else {
                    let qweather_api_key = credentials::load_qweather_api_key().unwrap_or_default();
                    let location_key = (
                        location.clone(),
                        cfg.weather_display_name.clone(),
                        cfg.weather_latitude.map(f64::to_bits),
                        cfg.weather_longitude.map(f64::to_bits),
                        language.code(),
                        cfg.weather_provider,
                        cfg.qweather_api_host.clone(),
                        qweather_api_key.clone(),
                    );
                    if active_location.as_ref() != Some(&location_key)
                        || last_fetch.elapsed()
                            >= Duration::from_secs(cfg.weather_refresh_minutes.max(5) * 60)
                    {
                        let next = match (
                            cfg.weather_display_name.as_deref(),
                            cfg.weather_latitude,
                            cfg.weather_longitude,
                        ) {
                            (Some(display_name), Some(latitude), Some(longitude)) => {
                                let services = weather::WeatherServices::new(
                                    cfg.weather_provider,
                                    cfg.qweather_api_host.clone(),
                                    qweather_api_key,
                                );
                                weather::fetch_coordinates_with_services(
                                    &location,
                                    display_name,
                                    latitude,
                                    longitude,
                                    language,
                                    &services,
                                )
                            }
                            _ => WeatherState {
                                requested_location: location.clone(),
                                error: Some(
                                    tr(language, Text::SelectLocationInSettings).to_owned(),
                                ),
                                ..WeatherState::default()
                            },
                        };
                        *state.write() = next;
                        active_location = Some(location_key);
                        last_fetch = Instant::now();
                    }
                }
                thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("spawn weather worker");
}

fn start_usage_worker(config: Arc<RwLock<Config>>, snapshot: Arc<RwLock<UsageSnapshot>>) {
    thread::Builder::new()
        .name("usage-refresh".into())
        .spawn(move || {
            let initial_config = config.read().clone();
            let codex_home = discover_codex_home(initial_config.codex_home.as_deref());
            let mut usage_collector = UsageCollector::new(codex_home.clone());
            let mut live_plan = None;
            let mut live_error = None;
            let mut last_live = Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now);
            loop {
                let cfg = config.read().clone();
                let language = Language::from_code(cfg.language.as_deref());
                if last_live.elapsed() >= Duration::from_millis(cfg.live_usage_ms.max(10_000)) {
                    match fetch_live_usage(&codex_home) {
                        Ok(plan) => {
                            live_plan = Some(plan);
                            live_error = None;
                        }
                        Err(error) => {
                            if let Some(plan) = live_plan.as_mut() {
                                plan.stale = true;
                            }
                            live_error = Some(format!(
                                "{}: {error}",
                                tr(language, Text::LiveQuotaUnavailable)
                            ));
                        }
                    }
                    last_live = Instant::now();
                }
                let mut next = usage_collector.collect(cfg.daily_budget_tokens, live_plan.clone());
                if next.status_message.is_none() {
                    next.status_message.clone_from(&live_error);
                }
                *snapshot.write() = next;
                thread::sleep(Duration::from_millis(cfg.refresh_ms.max(1_000)));
            }
        })
        .expect("spawn usage worker");
}

fn setup_tray(commands: mpsc::Sender<AppCommand>, language: Language) -> anyhow::Result<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::new(tr(language, Text::TrayToggle), true, None);
    let refresh = MenuItem::new(tr(language, Text::TrayShow), true, None);
    let quit = MenuItem::new(tr(language, Text::TrayQuit), true, None);
    menu.append(&show)?;
    menu.append(&refresh)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;
    let show_id = show.id().clone();
    let refresh_id = refresh.id().clone();
    let quit_id = quit.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = if event.id == show_id {
            Some(AppCommand::Toggle)
        } else if event.id == refresh_id {
            Some(AppCommand::Show)
        } else if event.id == quit_id {
            Some(AppCommand::Quit)
        } else {
            None
        };
        if let Some(command) = command {
            let _ = commands.send(command);
        }
    }));

    let icon = application_icon()?;
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tr(language, Text::TrayTooltip))
        .with_icon(icon)
        .build()
        .map_err(Into::into)
}

fn application_icon() -> anyhow::Result<Icon> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../assets/icons/codex-weather-widget-32.png"),
        image::ImageFormat::Png,
    )?
    .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).map_err(Into::into)
}
