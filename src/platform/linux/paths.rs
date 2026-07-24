//! XDG path resolution (Linux).

use std::ffi::OsStr;
use std::path::PathBuf;

/// `$XDG_CONFIG_HOME/ds4cc`, falling back to `~/.config/ds4cc`.
pub fn config_dir() -> PathBuf {
    config_dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// `$XDG_STATE_HOME/ds4cc`, falling back to `~/.local/state/ds4cc`.
pub fn log_dir() -> PathBuf {
    log_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

/// Pure helper (unit-tested): `xdg` = XDG_CONFIG_HOME, `home` = HOME.
fn config_dir_from(xdg: Option<impl AsRef<OsStr>>, home: Option<impl AsRef<OsStr>>) -> PathBuf {
    if let Some(x) = xdg.filter(|v| !v.as_ref().is_empty()) {
        return PathBuf::from(x.as_ref()).join("ds4cc");
    }
    if let Some(h) = home.filter(|v| !v.as_ref().is_empty()) {
        return PathBuf::from(h.as_ref()).join(".config").join("ds4cc");
    }
    // Last-resort relative fallback (no HOME at all).
    PathBuf::from(".config").join("ds4cc")
}

/// Pure helper (unit-tested): `xdg` = XDG_STATE_HOME, `home` = HOME.
fn log_dir_from(xdg: Option<impl AsRef<OsStr>>, home: Option<impl AsRef<OsStr>>) -> PathBuf {
    if let Some(x) = xdg.filter(|v| !v.as_ref().is_empty()) {
        return PathBuf::from(x.as_ref()).join("ds4cc");
    }
    if let Some(h) = home.filter(|v| !v.as_ref().is_empty()) {
        return PathBuf::from(h.as_ref())
            .join(".local")
            .join("state")
            .join("ds4cc");
    }
    PathBuf::from(".local").join("state").join("ds4cc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_prefers_xdg_config_home() {
        let dir = config_dir_from(Some("/tmp/xdgconf"), Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/tmp/xdgconf/ds4cc"));
    }

    #[test]
    fn config_falls_back_to_home_dot_config() {
        let dir = config_dir_from(None::<&str>, Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/home/u/.config/ds4cc"));
    }

    #[test]
    fn config_empty_xdg_is_ignored() {
        let dir = config_dir_from(Some(""), Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/home/u/.config/ds4cc"));
    }

    #[test]
    fn config_relative_fallback_without_home() {
        let dir = config_dir_from(None::<&str>, None::<&str>);
        assert_eq!(dir, PathBuf::from(".config/ds4cc"));
    }

    #[test]
    fn log_prefers_xdg_state_home() {
        let dir = log_dir_from(Some("/tmp/xdgstate"), Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/tmp/xdgstate/ds4cc"));
    }

    #[test]
    fn log_falls_back_to_local_state() {
        let dir = log_dir_from(None::<&str>, Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/home/u/.local/state/ds4cc"));
    }

    #[test]
    fn log_empty_xdg_is_ignored() {
        let dir = log_dir_from(Some(""), Some("/home/u"));
        assert_eq!(dir, PathBuf::from("/home/u/.local/state/ds4cc"));
    }
}
