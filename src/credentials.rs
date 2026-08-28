use anyhow::Context;
#[cfg(not(any(windows, target_os = "macos")))]
use anyhow::anyhow;

const SERVICE: &str = "Codex Weather Widget";
const QWEATHER_ACCOUNT: &str = "qweather-api-key";

#[cfg(any(windows, target_os = "macos"))]
fn qweather_entry() -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, QWEATHER_ACCOUNT).context("open secure credential store")
}

#[cfg(any(windows, target_os = "macos"))]
pub fn load_qweather_api_key() -> anyhow::Result<Option<String>> {
    match qweather_entry()?.get_password() {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("read QWeather API key"),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn load_qweather_api_key() -> anyhow::Result<Option<String>> {
    Ok(None)
}

#[cfg(any(windows, target_os = "macos"))]
pub fn save_qweather_api_key(value: &str) -> anyhow::Result<()> {
    let entry = qweather_entry()?;
    if value.trim().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error).context("delete QWeather API key"),
        }
    } else {
        entry
            .set_password(value.trim())
            .context("save QWeather API key")
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn save_qweather_api_key(value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "secure credential storage is not available on this platform"
        ))
    }
}
