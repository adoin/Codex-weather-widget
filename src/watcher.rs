use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::Duration,
};

use parking_lot::RwLock;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::{config::Config, model::AppCommand};

pub fn start(
    config: Arc<RwLock<Config>>,
    commands: Sender<AppCommand>,
    visible: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("codex-watcher".into())
        .spawn(move || {
            let refresh = RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing());
            let mut system = System::new_with_specifics(refresh);
            let mut last_running = None;
            loop {
                let cfg = config.read().clone();
                if cfg.follow_codex {
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::All,
                        true,
                        ProcessRefreshKind::nothing(),
                    );
                    let watched: Vec<String> = cfg
                        .watch_processes
                        .iter()
                        .map(|name| name.trim_end_matches(".exe").to_ascii_lowercase())
                        .collect();
                    let running = system.processes().values().any(|process| {
                        let name = process
                            .name()
                            .to_string_lossy()
                            .trim_end_matches(".exe")
                            .to_ascii_lowercase();
                        watched.iter().any(|wanted| wanted == &name)
                    });
                    if last_running != Some(running) {
                        if running {
                            if !visible.load(Ordering::Relaxed) {
                                let _ = commands.send(AppCommand::Show);
                            }
                        } else if visible.load(Ordering::Relaxed) {
                            let _ = commands.send(AppCommand::Hide);
                        }
                        last_running = Some(running);
                    }
                } else {
                    last_running = None;
                }
                thread::sleep(Duration::from_millis(cfg.watch_interval_ms.max(1_000)));
            }
        })
        .expect("spawn Codex watcher")
}
