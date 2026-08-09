//! Free local voice dictation (STT) backend resolution for OmegaG / DS4CC.
//!
//! ## Homage
//!
//! OmegaG's free Linux voice path is **not** an original STT engine. It stands
//! on the shoulders of:
//!
//! - **[hyprwhspr](https://github.com/goodroot/hyprwhspr)** by **goodroot** —
//!   the original native speech-to-text dictation project for Linux that
//!   established the private, local-first Wispr Flow alternative.
//! - **[hyprwhspr-rs](https://github.com/better-slop/hyprwhspr-rs)** by
//!   **better-slop** — the blazing-fast Rust reimplementation for Hyprland /
//!   Omarchy, with whisper.cpp, multi-provider STT, and compositor paste.
//!
//! We wire those tools into the controller tray and install path; we do not
//! rebrand their work as ours. See repository root `CREDITS.md`.
//!
//! ## Stack
//!
//! `hyprwhspr-rs` + **OpenAI Whisper medium** (via whisper.cpp) by default —
//! fully offline, no subscription.

use crate::config::VoiceConfig;
use std::path::{Path, PathBuf};

/// Resolved tray / launch target for the voice app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVoiceApp {
    /// Full command string (program + args) for `launch_voice_app`.
    pub command: String,
    /// Menu label shown in the Linux tray.
    pub label: String,
    /// True when we auto-discovered hyprwhspr-rs rather than using an
    /// explicit `app_command`.
    pub from_hyprwhspr_discover: bool,
}

/// Resolve what the Linux tray should launch, if anything.
///
/// Priority:
/// 1. Non-empty `[voice] app_command`
/// 2. When `auto_discover` and backend is hyprwhspr-rs (default): find binary
/// 3. Otherwise `None` (hide tray item)
pub fn resolve_voice_app(voice: &VoiceConfig) -> Option<ResolvedVoiceApp> {
    let explicit = voice.app_command.trim();
    if !explicit.is_empty() {
        let label = if voice.tray_label.trim().is_empty() {
            tray_label_for(voice, false)
        } else {
            voice.tray_label.trim().to_string()
        };
        return Some(ResolvedVoiceApp {
            command: explicit.to_string(),
            label,
            from_hyprwhspr_discover: false,
        });
    }

    if !voice.auto_discover {
        return None;
    }

    if !is_hyprwhspr_backend(&voice.backend) {
        return None;
    }

    let path = discover_hyprwhspr_rs()?;
    let label = if voice.tray_label.trim().is_empty() {
        tray_label_for(voice, true)
    } else {
        voice.tray_label.trim().to_string()
    };
    Some(ResolvedVoiceApp {
        command: path.display().to_string(),
        label,
        from_hyprwhspr_discover: true,
    })
}

fn is_hyprwhspr_backend(backend: &str) -> bool {
    let b = backend.trim().to_ascii_lowercase();
    b.is_empty() || b == "hyprwhspr-rs" || b == "hyprwhspr" || b == "free" || b == "whisper"
}

fn tray_label_for(voice: &VoiceConfig, discovered: bool) -> String {
    if discovered || is_hyprwhspr_backend(&voice.backend) {
        format!(
            "Open hyprwhspr (free Whisper {})",
            voice.whisper_model.trim()
        )
    } else {
        "Open voice app".into()
    }
}

/// Locate `hyprwhspr-rs` on PATH and common install prefixes.
pub fn discover_hyprwhspr_rs() -> Option<PathBuf> {
    const NAMES: &[&str] = &["hyprwhspr-rs"];

    // Absolute candidates first (stable for tray spawn).
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join(".cargo/bin/hyprwhspr-rs"));
        candidates.push(home.join(".local/bin/hyprwhspr-rs"));
    }
    if let Some(xdg) = std::env::var_os("XDG_BIN_HOME") {
        candidates.push(PathBuf::from(xdg).join("hyprwhspr-rs"));
    }
    candidates.push(PathBuf::from("/usr/local/bin/hyprwhspr-rs"));
    candidates.push(PathBuf::from("/usr/bin/hyprwhspr-rs"));

    for c in &candidates {
        if is_executable(c) {
            return Some(c.clone());
        }
    }

    // PATH walk as last resort; prefer absolute resolution.
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in NAMES {
                let p = dir.join(name);
                if is_executable(&p) {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// One-line credit suitable for logs at voice launch.
pub fn credit_line() -> &'static str {
    "Free STT: hyprwhspr-rs (better-slop) · original hyprwhspr (goodroot) · OpenAI Whisper"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VoiceConfig;

    #[test]
    fn explicit_app_command_wins() {
        let voice = VoiceConfig {
            app_command: "/opt/my-stt --daemon".into(),
            backend: "external".into(),
            whisper_model: "medium".into(),
            auto_discover: true,
            tray_label: String::new(),
        };
        let r = resolve_voice_app(&voice).expect("resolved");
        assert_eq!(r.command, "/opt/my-stt --daemon");
        assert!(!r.from_hyprwhspr_discover);
        assert_eq!(r.label, "Open voice app");
    }

    #[test]
    fn custom_tray_label() {
        let voice = VoiceConfig {
            app_command: "/bin/true".into(),
            tray_label: "Dictation".into(),
            ..VoiceConfig::default()
        };
        let r = resolve_voice_app(&voice).unwrap();
        assert_eq!(r.label, "Dictation");
    }

    #[test]
    fn no_discover_when_disabled() {
        let voice = VoiceConfig {
            app_command: String::new(),
            auto_discover: false,
            ..VoiceConfig::default()
        };
        assert!(resolve_voice_app(&voice).is_none());
    }

    #[test]
    fn credit_line_names_creators() {
        let line = credit_line();
        assert!(line.contains("goodroot"));
        assert!(line.contains("better-slop") || line.contains("hyprwhspr-rs"));
        assert!(line.contains("Whisper"));
    }

    #[test]
    fn hyprwhspr_backend_aliases() {
        assert!(is_hyprwhspr_backend("hyprwhspr-rs"));
        assert!(is_hyprwhspr_backend("hyprwhspr"));
        assert!(is_hyprwhspr_backend("free"));
        assert!(is_hyprwhspr_backend(""));
        assert!(!is_hyprwhspr_backend("external"));
    }
}
