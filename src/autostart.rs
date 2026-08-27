#[cfg(target_os = "windows")]
mod platform {
    use std::{
        process::{Command, Output},
        time::Duration,
    };

    use anyhow::{Context, Result, anyhow};

    const TASK_NAME: &str = "Codex Weather Widget";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn configure(enabled: bool) -> Result<()> {
        if !enabled {
            if task_exists()? {
                let output = run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;
                ensure_succeeded("remove startup task", &output)?;
            }
            return Ok(());
        }

        let executable = std::env::current_exe().context("read the current executable path")?;
        let action = format!("\"{}\"", executable.display());
        let output = run_schtasks(&[
            "/Create", "/TN", TASK_NAME, "/TR", &action, "/SC", "ONLOGON", "/DELAY", "0000:10",
            "/RL", "LIMITED", "/F",
        ])?;
        ensure_succeeded("create startup task", &output)?;

        // schtasks can report success before Task Scheduler has made a newly
        // created task queryable. Give it one short retry before surfacing an
        // error in the settings panel.
        if !task_exists()? {
            std::thread::sleep(Duration::from_millis(80));
            if !task_exists()? {
                return Err(anyhow!(
                    "the startup task could not be verified after creation"
                ));
            }
        }
        Ok(())
    }

    fn task_exists() -> Result<bool> {
        Ok(run_schtasks(&["/Query", "/TN", TASK_NAME])?
            .status
            .success())
    }

    fn run_schtasks(arguments: &[&str]) -> Result<Output> {
        use std::os::windows::process::CommandExt;

        Command::new("schtasks.exe")
            .args(arguments)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("run the Windows Task Scheduler command")
    }

    fn ensure_succeeded(action: &str, output: &Output) -> Result<()> {
        if output.status.success() {
            return Ok(());
        }
        let detail = if output.stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout)
        } else {
            String::from_utf8_lossy(&output.stderr)
        };
        Err(anyhow!("{action} failed: {}", detail.trim()))
    }
}

#[cfg(target_os = "windows")]
pub use platform::configure;

#[cfg(not(target_os = "windows"))]
pub fn configure(enabled: bool) -> anyhow::Result<()> {
    if enabled {
        anyhow::bail!("startup registration is currently supported on Windows only");
    }
    Ok(())
}
