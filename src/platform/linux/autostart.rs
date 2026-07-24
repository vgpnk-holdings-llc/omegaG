//! Linux autostart: systemd user unit (preferred) with an XDG autostart
//! `.desktop` fallback for session-less environments.
//!
//! - Unit file: `~/.config/systemd/user/ds4cc.service` (mirrors
//!   `packaging/linux/ds4cc.service`, with `ExecStart` pointing at the
//!   current executable's absolute path).
//! - Enable/disable: `systemctl --user enable|disable --now ds4cc.service`.
//! - If `systemctl --user` is unavailable (no user manager / no D-Bus
//!   session), falls back to `~/.config/autostart/ds4cc.desktop`
//!   (`Terminal=false`).
//! - `autostart_is_enabled()`: unit is enabled OR the desktop file exists.

use std::path::PathBuf;

use crate::platform::linux::paths::config_dir;

const UNIT_NAME: &str = "ds4cc.service";
const DESKTOP_NAME: &str = "ds4cc.desktop";

pub fn autostart_is_enabled() -> bool {
    systemd_unit_enabled() || desktop_file_path().is_some_and(|p| p.exists())
}

pub fn autostart_set(enabled: bool) -> Result<(), String> {
    if systemctl_user_available() {
        set_systemd(enabled)
    } else {
        set_xdg(enabled)
    }
}

// ── systemd user unit ────────────────────────────────────────────────

fn systemd_dir() -> Option<PathBuf> {
    // Systemd user units live under XDG_CONFIG_HOME/systemd/user.
    config_dir()
        .parent()
        .map(|xdg| xdg.join("systemd").join("user"))
}

fn unit_path() -> Option<PathBuf> {
    systemd_dir().map(|d| d.join(UNIT_NAME))
}

fn systemctl(args: &[&str]) -> Result<std::process::ExitStatus, String> {
    let mut full = vec!["--user"];
    full.extend_from_slice(args);
    std::process::Command::new("systemctl")
        .args(&full)
        .status()
        .map_err(|e| format!("systemctl failed to start: {e}"))
}

/// True when a systemd user manager is reachable.
fn systemctl_user_available() -> bool {
    systemctl(&["show", "--property=Version", "--value"])
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemd_unit_enabled() -> bool {
    systemctl_user_available()
        && systemctl(&["is-enabled", UNIT_NAME])
            .map(|s| s.success())
            .unwrap_or(false)
}

fn unit_contents(exe: &std::path::Path) -> String {
    format!(
        "[Unit]\n\
         Description=ds4cc — DualSense/DS4 controller shortcut daemon\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe.display()
    )
}

fn set_systemd(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot determine exe path for auto-start: {e}"))?;
        let unit_path = unit_path().ok_or("cannot resolve systemd user unit dir")?;
        if let Some(parent) = unit_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::write(&unit_path, unit_contents(&exe))
            .map_err(|e| format!("write {}: {e}", unit_path.display()))?;
        let _ = systemctl(&["daemon-reload"]);
        run_systemctl_toggle(&["enable", "--now", UNIT_NAME])
    } else {
        // Best-effort stop+disable; a missing unit is not an error.
        match systemctl(&["disable", "--now", UNIT_NAME]) {
            Ok(status) if status.success() => {
                log::info!("Auto-start disabled (systemd user unit)");
                Ok(())
            }
            Ok(status) => {
                log::debug!("systemctl disable exited with {status} (unit may not exist)");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

fn run_systemctl_toggle(args: &[&str]) -> Result<(), String> {
    match systemctl(args) {
        Ok(status) if status.success() => {
            log::info!("Auto-start enabled (systemd user unit)");
            Ok(())
        }
        Ok(status) => Err(format!("systemctl {} failed (exit {status})", args[0])),
        Err(e) => Err(e),
    }
}

// ── XDG autostart fallback ───────────────────────────────────────────

fn desktop_file_path() -> Option<PathBuf> {
    config_dir()
        .parent()
        .map(|xdg| xdg.join("autostart").join(DESKTOP_NAME))
}

fn desktop_contents(exe: &std::path::Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ds4cc\n\
         Comment=DualSense/DS4 controller shortcut daemon\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exe.display()
    )
}

fn set_xdg(enabled: bool) -> Result<(), String> {
    let path = desktop_file_path().ok_or("cannot resolve XDG autostart dir")?;
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot determine exe path for auto-start: {e}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, desktop_contents(&exe))
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        log::info!("Auto-start enabled (XDG autostart)");
        Ok(())
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => {
                log::info!("Auto-start disabled (XDG autostart)");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove {}: {e}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_file_has_required_stanzas() {
        let unit = unit_contents(std::path::Path::new("/home/u/.local/bin/ds4cc"));
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("ExecStart=/home/u/.local/bin/ds4cc"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn desktop_file_has_required_keys() {
        let desktop = desktop_contents(std::path::Path::new("/usr/bin/ds4cc"));
        assert!(desktop.contains("[Desktop Entry]"));
        assert!(desktop.contains("Type=Application"));
        assert!(desktop.contains("Exec=/usr/bin/ds4cc"));
        assert!(desktop.contains("Terminal=false"));
    }
}
