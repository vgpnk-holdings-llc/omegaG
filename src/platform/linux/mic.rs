//! Linux microphone mute control via `pactl` (PulseAudio/PipeWire
//! compatibility layer) with a `wpctl` (PipeWire native) fallback.
//!
//! Fixed argv arrays, no shell. The last known mute state is cached so the
//! controller mic LED keeps working even when the query tools are missing or
//! the audio server is unreachable.
//!
//! Divergence from Windows (documented): Windows tracks the LED from the
//! cached state plus an initial query at startup; Linux does the same here,
//! but the initial query happens lazily on the first `mic_is_muted()` call
//! instead of an explicit `init()` — the observable LED behavior is equal.

use std::sync::Mutex;

/// Last known mute state (updated by successful toggles and queries).
static CACHED_MUTED: Mutex<Option<bool>> = Mutex::new(None);

/// Toggle the default audio capture source mute state.
///
/// Tries `pactl set-source-mute @DEFAULT_SOURCE@ toggle` first; if pactl is
/// missing or exits non-zero, falls back to
/// `wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle`.
pub fn mic_toggle() -> Result<(), String> {
    let pactl_err = match run_status("pactl", &["set-source-mute", "@DEFAULT_SOURCE@", "toggle"]) {
        Ok(()) => {
            record_toggle();
            return Ok(());
        }
        Err(e) => e,
    };
    match run_status("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]) {
        Ok(()) => {
            record_toggle();
            Ok(())
        }
        Err(wpctl_err) => Err(format!(
            "mic toggle failed (pactl: {pactl_err}; wpctl: {wpctl_err})"
        )),
    }
}

/// Current mute state: live query first (pactl, then wpctl), cached value as
/// fallback, `None` if nothing is known.
pub fn mic_is_muted() -> Option<bool> {
    if let Some(muted) = query_muted() {
        set_cached(muted);
        return Some(muted);
    }
    cached()
}

// ── internals ────────────────────────────────────────────────────────

fn cached() -> Option<bool> {
    *CACHED_MUTED.lock().unwrap_or_else(|p| p.into_inner())
}

fn set_cached(muted: bool) {
    let mut guard = CACHED_MUTED.lock().unwrap_or_else(|p| p.into_inner());
    *guard = Some(muted);
}

/// After a successful toggle: refresh from a live query if possible,
/// otherwise flip the cached value so the LED still tracks.
fn record_toggle() {
    if let Some(muted) = query_muted() {
        set_cached(muted);
        log::info!("mic: {}", if muted { "muted" } else { "unmuted" });
        return;
    }
    let mut guard = CACHED_MUTED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(prev) = *guard {
        *guard = Some(!prev);
        log::info!(
            "mic: {} (cached; query unavailable)",
            if !prev { "muted" } else { "unmuted" }
        );
    } else {
        log::warn!("mic: toggled but state unknown (no query tool available)");
    }
}

fn query_muted() -> Option<bool> {
    if let Ok(out) = run_output("pactl", &["get-source-mute", "@DEFAULT_SOURCE@"]) {
        if let Some(m) = parse_pactl_mute(&out) {
            return Some(m);
        }
    }
    if let Ok(out) = run_output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SOURCE@"]) {
        if let Some(m) = parse_wpctl_muted(&out) {
            return Some(m);
        }
    }
    None
}

fn run_status(cmd: &str, args: &[&str]) -> Result<(), String> {
    match std::process::Command::new(cmd).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{cmd} exited with {status}")),
        Err(e) => Err(format!("{cmd} failed to start: {e}")),
    }
}

fn run_output(cmd: &str, args: &[&str]) -> Result<String, String> {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => Err(format!("{cmd} exited with {}", out.status)),
        Err(e) => Err(format!("{cmd} failed to start: {e}")),
    }
}

/// Parse `pactl get-source-mute` output ("Mute: yes" / "Mute: no").
fn parse_pactl_mute(out: &str) -> Option<bool> {
    for line in out.lines() {
        if let Some(rest) = line.trim().strip_prefix("Mute:") {
            return match rest.trim() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            };
        }
    }
    None
}

/// Parse `wpctl get-volume` output ("Volume: 0.40 [MUTED]" / "Volume: 0.40").
fn parse_wpctl_muted(out: &str) -> Option<bool> {
    let text = out.trim();
    if text.contains("[MUTED]") {
        Some(true)
    } else if text.starts_with("Volume:") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pactl_mute_yes() {
        assert_eq!(parse_pactl_mute("Mute: yes\n"), Some(true));
    }

    #[test]
    fn pactl_mute_no() {
        assert_eq!(parse_pactl_mute("Mute: no\n"), Some(false));
    }

    #[test]
    fn pactl_mute_whitespace_tolerant() {
        assert_eq!(parse_pactl_mute("  Mute:   yes  \n"), Some(true));
    }

    #[test]
    fn pactl_garbage_is_none() {
        assert_eq!(parse_pactl_mute(""), None);
        assert_eq!(parse_pactl_mute("not pactl output"), None);
        assert_eq!(parse_pactl_mute("Mute: maybe"), None);
    }

    #[test]
    fn wpctl_muted_marker() {
        assert_eq!(parse_wpctl_muted("Volume: 0.40 [MUTED]\n"), Some(true));
    }

    #[test]
    fn wpctl_unmuted() {
        assert_eq!(parse_wpctl_muted("Volume: 0.40\n"), Some(false));
        assert_eq!(parse_wpctl_muted("Volume: 1.00"), Some(false));
    }

    #[test]
    fn wpctl_garbage_is_none() {
        assert_eq!(parse_wpctl_muted(""), None);
        assert_eq!(parse_wpctl_muted("error: no such node"), None);
    }
}
