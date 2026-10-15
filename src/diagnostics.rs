//! Small, synchronous flight recorder. Never record credentials or API payloads.
use parking_lot::Mutex;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::OnceLock,
    time::Instant,
};

const LIMIT: u64 = 2 * 1024 * 1024;
static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

struct Recorder {
    file: File,
    path: PathBuf,
    bytes: u64,
}

impl Recorder {
    fn write(&mut self, text: &str) -> io::Result<()> {
        if self.bytes + text.len() as u64 > LIMIT {
            // Copy rather than rename an open file (Windows disallows it).
            self.file.flush()?;
            fs::copy(&self.path, self.path.with_extension("previous.log"))?;
            self.file.set_len(0)?;
            self.bytes = 0;
        }
        self.file.write_all(text.as_bytes())?;
        self.file.flush()?;
        self.bytes += text.len() as u64;
        Ok(())
    }
}

pub fn init() -> io::Result<()> {
    START.get_or_init(Instant::now);
    let root = crate::config::Config::path().with_file_name("logs");
    fs::create_dir_all(&root)?;
    // Keep five prior sessions plus the new session, each at most two files.
    let mut sessions: Vec<_> = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("diagnostic-") && n.ends_with(".log") && !n.ends_with(".previous.log")
            })
        })
        .collect();
    sessions.sort();
    for path in sessions.iter().take(sessions.len().saturating_sub(5)) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("previous.log"));
    }
    let path = root.join(format!(
        "diagnostic-{}-{}.log",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        std::process::id()
    ));
    let file = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(&path)?;
    let _ = RECORDER.set(Mutex::new(Recorder {
        file,
        path,
        bytes: 0,
    }));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        event(format!(
            "panic {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        ));
        previous(info);
    }));
    event(format!(
        "session_start version={} pid={} os={} arch={}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    Ok(())
}

pub fn event(message: impl std::fmt::Display) {
    if let Some(recorder) = RECORDER.get() {
        let thread = std::thread::current();
        let line = format!(
            "{} uptime_ms={} thread={} {message}\n",
            chrono::Utc::now().to_rfc3339(),
            START.get().map_or(0, |s| s.elapsed().as_millis()),
            thread.name().unwrap_or("unnamed")
        );
        let _ = recorder.lock().write(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_preserves_tail_and_appends_from_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .unwrap();
        let mut recorder = Recorder {
            file,
            path: path.clone(),
            bytes: 0,
        };
        let full = "x".repeat(LIMIT as usize);
        recorder.write(&full).unwrap();
        recorder.write("next\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "next\n");
        assert_eq!(
            fs::read_to_string(path.with_extension("previous.log")).unwrap(),
            full
        );
    }
}
