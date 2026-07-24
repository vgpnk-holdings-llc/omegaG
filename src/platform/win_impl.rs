//! Windows platform adapters — thin re-exports/adapters over the existing
//! Windows code. Behavior is byte-for-byte identical to the pre-port code:
//! same `%APPDATA%\ds4cc` config dir, same HKCU Run registry autostart
//! (via `reg.exe`, as `tray.rs` has always done), same Core Audio mic
//! toggle in `mic.rs`.
//!
//! NOTE: the Windows `Injector` implementation is NOT here — `WinInjector`
//! is owned by `mapper.rs` (C2), wrapping the existing SendInput code, and
//! Windows callers construct it directly.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

// ── paths ────────────────────────────────────────────────────────────

/// `%APPDATA%\ds4cc`. Returns an empty path when APPDATA is unset so
/// `config.rs` can keep its legacy relative-file fallback (`ds4cc.toml`).
pub fn config_dir() -> PathBuf {
    match std::env::var("APPDATA") {
        Ok(appdata) => PathBuf::from(appdata).join("ds4cc"),
        Err(_) => PathBuf::new(),
    }
}

/// Windows keeps logs next to the config (file logging is a Linux feature;
/// the Windows tray shows a console log window instead).
pub fn log_dir() -> PathBuf {
    config_dir()
}

// ── autostart (HKCU Run registry key, via reg.exe) ───────────────────

const APP_NAME: &str = "DS4CC";
const REG_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

pub fn autostart_is_enabled() -> bool {
    std::process::Command::new("reg")
        .args(["query", REG_RUN_KEY, "/v", APP_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn autostart_set(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot determine exe path for auto-start: {e}"))?;
        // Quote path to handle spaces (matches legacy tray.rs behavior).
        let value = format!("\"{}\"", exe.to_string_lossy());
        let status = std::process::Command::new("reg")
            .args([
                "add",
                REG_RUN_KEY,
                "/v",
                APP_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])
            .status()
            .map_err(|e| format!("reg add error: {e}"))?;
        if status.success() {
            log::info!("Auto-start enabled: {value}");
            Ok(())
        } else {
            Err(format!("reg add failed (exit {status})"))
        }
    } else {
        let status = std::process::Command::new("reg")
            .args(["delete", REG_RUN_KEY, "/v", APP_NAME, "/f"])
            .status()
            .map_err(|e| format!("reg delete error: {e}"))?;
        if status.success() {
            log::info!("Auto-start disabled");
            Ok(())
        } else {
            Err(format!("reg delete failed (exit {status})"))
        }
    }
}

// ── mic (Core Audio, delegates to the existing mic.rs) ───────────────

pub fn mic_toggle() -> Result<(), String> {
    crate::mic::toggle_mute();
    Ok(())
}

pub fn mic_is_muted() -> Option<bool> {
    Some(crate::mic::MIC_MUTED.load(Ordering::Relaxed))
}
