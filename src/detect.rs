/// Unified first-run keybind detection — one probe round for everything.
///
/// Windows: a single `wsl bash -lc` invocation fetches four text sections
/// separated by markers: tmux server prefix, tmux prefix-table bindings,
/// ~/.tmux.conf, and ~/.claude/keybindings.json.
///
/// Linux: the same four sections are fetched natively — `tmux show-options
/// -g prefix` and `tmux list-keys -T prefix` via `native_run` (no shell),
/// ~/.tmux.conf and ~/.claude/keybindings.json read directly from $HOME.
///
/// Each section is parsed by a pure function (shared with the Windows path)
/// so all parsing is unit-testable without WSL or tmux.
///
/// Everything is best-effort: missing tmux, missing Claude Code, or missing
/// WSL simply yields empty sections and the mapper falls back to defaults.
use crate::mapper::{VKey, parse_key_combo};
use crate::tmux_detect::{self, TmuxDetected};
#[cfg(windows)]
use crate::wsl::run_wsl;
use std::collections::HashMap;

// Native subprocess probing lives in its own module (declared here via path
// so it stays self-contained with the Linux detection code; the file is
// empty on non-Linux targets via its inner cfg).
#[cfg(target_os = "linux")]
#[path = "native_run.rs"]
mod native_run;

/// Everything detected at startup.
#[derive(Debug, Clone, Default)]
pub struct Detected {
    /// Tmux prefix + action→key bindings (None if tmux unavailable).
    pub tmux: Option<TmuxDetected>,
    /// Claude Code CLI action → key sequence (e.g. "chat:externalEditor" → [[Ctrl,G]]).
    /// Multi-chord bindings like "ctrl+x ctrl+k" become sequences of combos.
    pub claude: HashMap<String, Vec<Vec<VKey>>>,
}

impl Detected {
    /// Look up a Claude Code action binding (e.g. "chat:cycleMode").
    pub fn claude_binding(&self, action: &str) -> Option<&Vec<Vec<VKey>>> {
        self.claude.get(action)
    }
}

#[cfg(windows)]
const MARKER: &str = "===DS4CC===";

/// Run the single detection probe via WSL and parse all sections.
#[cfg(windows)]
pub fn detect() -> Detected {
    log::info!("Detecting keybinds via WSL (tmux + Claude Code)...");
    let start = std::time::Instant::now();

    // Trailing `true` keeps the compound exit status 0 even when the last
    // probe fails (e.g. no ~/.claude/keybindings.json) — run_wsl discards
    // ALL output on non-zero exit, which would drop valid tmux data too.
    let cmd = format!(
        "tmux show-options -g prefix 2>/dev/null; echo {MARKER}; \
         tmux list-keys -T prefix 2>/dev/null; echo {MARKER}; \
         cat ~/.tmux.conf 2>/dev/null; echo {MARKER}; \
         cat ~/.claude/keybindings.json 2>/dev/null; true"
    );

    let Some(output) = run_wsl(&cmd) else {
        log::warn!("WSL unavailable — keybind detection skipped, using defaults");
        return Detected::default();
    };

    let mut sections = output.split(MARKER);
    let prefix_out = sections.next().unwrap_or("");
    let keys_out = sections.next().unwrap_or("");
    let conf_out = sections.next().unwrap_or("");
    let claude_json = sections.next().unwrap_or("");

    let tmux = tmux_detect::parse(prefix_out, keys_out, conf_out);
    let claude = parse_claude_keybindings(claude_json);

    log::info!(
        "Keybind detection done in {:?} (tmux: {}, Claude Code actions: {})",
        start.elapsed(),
        if tmux.is_some() { "ok" } else { "unavailable" },
        claude.len(),
    );

    Detected { tmux, claude }
}

/// Run the single detection probe natively (Linux) and parse all sections.
///
/// Probing ladder — same order and graceful degradation as the Windows
/// WSL probe, with a missing/failing tmux behaving exactly like the
/// Windows "no WSL" path (empty sections → mapper defaults):
///   1. `tmux show-options -g prefix`  — running server's prefix
///   2. `tmux list-keys -T prefix`     — running server's prefix table
///   3. `~/.tmux.conf`                 — fallback when no server / no tmux
///   4. `~/.claude/keybindings.json`   — Claude Code bindings
///
/// Each probe is independent and best-effort: no tmux binary, no tmux
/// server, or missing files just yield empty sections.
#[cfg(target_os = "linux")]
pub fn detect() -> Detected {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    detect_native(&home, "tmux")
}

/// Linux detection, with the tmux binary name and $HOME injectable so tests
/// can point at a fake-tmux script and a fixture home directory (SPEC §10:
/// no test may require a real tmux server).
#[cfg(target_os = "linux")]
fn detect_native(home: &std::path::Path, tmux_bin: &str) -> Detected {
    log::info!("Detecting keybinds natively (tmux + Claude Code)...");
    let start = std::time::Instant::now();
    /// Bound each tmux probe so a hung server cannot stall startup.
    const PROBE_TIMEOUT_MS: u64 = 2_000;

    let prefix_out = probe(
        native_run::run(
            &[tmux_bin, "show-options", "-g", "prefix"],
            PROBE_TIMEOUT_MS,
        ),
        tmux_bin,
        "show-options",
    );
    let keys_out = probe(
        native_run::run(&[tmux_bin, "list-keys", "-T", "prefix"], PROBE_TIMEOUT_MS),
        tmux_bin,
        "list-keys",
    );
    let conf_out = read_home_file(home, ".tmux.conf");
    let claude_json = read_home_file(home, ".claude/keybindings.json");

    // Pure parsers shared with the Windows path (tmux_detect.rs untouched).
    let tmux = tmux_detect::parse(&prefix_out, &keys_out, &conf_out);
    let claude = parse_claude_keybindings(&claude_json);

    log::info!(
        "Keybind detection done in {:?} (tmux: {}, Claude Code actions: {})",
        start.elapsed(),
        if tmux.is_some() { "ok" } else { "unavailable" },
        claude.len(),
    );

    Detected { tmux, claude }
}

/// Map a tmux probe result to a section string ("" on any failure).
#[cfg(target_os = "linux")]
fn probe(result: std::io::Result<String>, tmux_bin: &str, what: &str) -> String {
    match result {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!("tmux binary '{tmux_bin}' not found — detection falls back to ~/.tmux.conf");
            String::new()
        }
        Err(e) => {
            // No server running (exit 1), timeout, etc. — same degradation
            // as an empty section in the Windows WSL probe.
            log::debug!("tmux {what} probe failed: {e}");
            String::new()
        }
    }
}

/// Read a file relative to $HOME; missing/unreadable → "".
#[cfg(target_os = "linux")]
fn read_home_file(home: &std::path::Path, rel: &str) -> String {
    let path = home.join(rel);
    match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::debug!("{} unreadable: {e}", path.display());
            String::new()
        }
    }
}

// ── Claude Code keybindings.json ─────────────────────────────────────

/// Parse ~/.claude/keybindings.json into an action → key-sequence map.
///
/// Schema: `{ "bindings": [ { "context": "...", "bindings": { "<keys>": "<action>" } } ] }`
/// Key strings are space-separated chords of `mod+...+key` (e.g. "ctrl+x ctrl+k").
/// The first binding seen for an action wins (Global context comes first).
fn parse_claude_keybindings(json: &str) -> HashMap<String, Vec<Vec<VKey>>> {
    let mut actions = HashMap::new();
    let json = json.trim();
    if json.is_empty() {
        return actions;
    }

    let Ok(root) = serde_json::from_str::<serde_json::Value>(json) else {
        log::warn!("Failed to parse ~/.claude/keybindings.json");
        return actions;
    };

    let Some(contexts) = root.get("bindings").and_then(|b| b.as_array()) else {
        return actions;
    };

    for context in contexts {
        let Some(bindings) = context.get("bindings").and_then(|b| b.as_object()) else {
            continue;
        };
        for (keys, action) in bindings {
            let Some(action) = action.as_str() else {
                continue;
            };
            if actions.contains_key(action) {
                continue; // first binding wins
            }
            if let Some(seq) = parse_claude_keys(keys) {
                actions.insert(action.to_string(), seq);
            }
        }
    }

    actions
}

/// Parse a Claude Code key string ("ctrl+shift+b", "meta+t", "ctrl+x ctrl+k")
/// into a sequence of VKey combos. In terminal context "meta"/"cmd" mean Alt.
fn parse_claude_keys(s: &str) -> Option<Vec<Vec<VKey>>> {
    // meta/cmd are the terminal Alt key, not the Windows key — remap before
    // parse_key_combo (whose "meta" resolves to VKey::Win for desktop combos).
    let normalized = s
        .to_ascii_lowercase()
        .replace("meta+", "alt+")
        .replace("cmd+", "alt+");

    normalized.split_whitespace().map(parse_key_combo).collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "bindings": [
            { "context": "Global", "bindings": {
                "ctrl+t": "app:toggleTodos",
                "ctrl+]": "app:openArtifact"
            }},
            { "context": "Chat", "bindings": {
                "shift+tab": "chat:cycleMode",
                "meta+t": "chat:thinkingToggle",
                "ctrl+x ctrl+k": "chat:killAgents",
                "ctrl+g": "chat:externalEditor",
                "ctrl+t": "chat:shadowedByGlobal"
            }}
        ]
    }"#;

    #[test]
    fn parses_simple_combo() {
        let map = parse_claude_keybindings(SAMPLE);
        assert_eq!(map["app:toggleTodos"], vec![vec![VKey::Control, VKey::T]]);
    }

    #[test]
    fn parses_bracket_key() {
        let map = parse_claude_keybindings(SAMPLE);
        assert_eq!(
            map["app:openArtifact"],
            vec![vec![VKey::Control, VKey::RightBracket]]
        );
    }

    #[test]
    fn meta_means_alt() {
        let map = parse_claude_keybindings(SAMPLE);
        assert_eq!(map["chat:thinkingToggle"], vec![vec![VKey::Alt, VKey::T]]);
    }

    #[test]
    fn parses_two_chord_sequence() {
        let map = parse_claude_keybindings(SAMPLE);
        assert_eq!(
            map["chat:killAgents"],
            vec![vec![VKey::Control, VKey::X], vec![VKey::Control, VKey::K]]
        );
    }

    #[test]
    fn empty_or_invalid_json_yields_empty_map() {
        assert!(parse_claude_keybindings("").is_empty());
        assert!(parse_claude_keybindings("not json").is_empty());
    }
}

// ── Linux native detection tests (fake tmux + fixture $HOME) ─────────
//
// SPEC §10: no test may require a real tmux server, HID, or network.
// A fake `tmux` shell script and a fixture home directory stand in for
// both; the full detect_native ladder (spawn → parse → fallbacks) runs
// for real.

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct Fixture {
        root: PathBuf,
        home: PathBuf,
        tmux: PathBuf,
    }

    impl Fixture {
        /// Create a fresh fixture dir with an executable fake `tmux` script.
        fn new(script: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let root =
                std::env::temp_dir().join(format!("ds4cc-detect-{}-{n}", std::process::id()));
            let home = root.join("home");
            let bin = root.join("bin");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(&bin).unwrap();
            let tmux = bin.join("tmux");
            std::fs::write(&tmux, script).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            Self { root, home, tmux }
        }

        fn write_home(&self, rel: &str, contents: &str) {
            let path = self.home.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        fn tmux_str(&self) -> &str {
            self.tmux.to_str().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Fake tmux with a running server: answers both probes.
    const FAKE_TMUX_SERVER: &str = r#"#!/bin/sh
case "$1" in
  show-options) printf 'prefix C-a\n' ;;
  list-keys) printf 'bind-key    -T prefix p       previous-window\nbind-key    -T prefix |       split-window -h\n' ;;
  *) exit 1 ;;
esac
"#;

    /// Fake tmux with NO server running: every probe exits 1 (like real tmux).
    const FAKE_TMUX_NO_SERVER: &str = "#!/bin/sh\nexit 1\n";

    #[test]
    fn detects_from_running_tmux_server() {
        let fx = Fixture::new(FAKE_TMUX_SERVER);
        let d = detect_native(&fx.home, fx.tmux_str());

        let tmux = d.tmux.expect("tmux must be detected from fake server");
        assert_eq!(tmux.prefix, Some(vec![VKey::Control, VKey::A]));
        assert_eq!(tmux.key_for_action("previous-window"), Some(&vec![VKey::P]));
        assert_eq!(
            tmux.key_for_action("split-window -h"),
            Some(&vec![VKey::Shift, VKey::Backslash])
        );
    }

    #[test]
    fn falls_back_to_tmux_conf_when_no_server() {
        let fx = Fixture::new(FAKE_TMUX_NO_SERVER);
        fx.write_home(
            ".tmux.conf",
            "set -g prefix C-f\nbind | split-window -h\nbind -r n next-window\n",
        );
        let d = detect_native(&fx.home, fx.tmux_str());

        let tmux = d.tmux.expect("tmux.conf fallback must still detect");
        assert_eq!(tmux.prefix, Some(vec![VKey::Control, VKey::F]));
        assert_eq!(
            tmux.key_for_action("split-window -h"),
            Some(&vec![VKey::Shift, VKey::Backslash])
        );
        assert_eq!(tmux.key_for_action("next-window"), Some(&vec![VKey::N]));
    }

    #[test]
    fn no_tmux_binary_degrades_like_no_wsl() {
        // tmux absent + no conf → tmux None (mapper uses defaults), but
        // Claude Code bindings are still picked up natively.
        let fx = Fixture::new(FAKE_TMUX_SERVER);
        fx.write_home(
            ".claude/keybindings.json",
            r#"{ "bindings": [ { "context": "Global", "bindings": { "ctrl+t": "app:toggleTodos" } } ] }"#,
        );
        let missing = fx.root.join("bin").join("no-such-tmux");
        let d = detect_native(&fx.home, missing.to_str().unwrap());

        assert!(d.tmux.is_none(), "no tmux binary must yield tmux: None");
        assert_eq!(
            d.claude_binding("app:toggleTodos"),
            Some(&vec![vec![VKey::Control, VKey::T]])
        );
    }

    #[test]
    fn everything_missing_yields_defaults_without_panic() {
        let fx = Fixture::new(FAKE_TMUX_NO_SERVER);
        let missing = fx.root.join("bin").join("no-such-tmux");
        let d = detect_native(&fx.home, missing.to_str().unwrap());
        assert!(d.tmux.is_none());
        assert!(d.claude.is_empty());
    }

    #[test]
    fn hung_tmux_probe_is_bounded_by_timeout() {
        // Fake tmux that sleeps forever: detect must still complete quickly.
        let fx = Fixture::new("#!/bin/sh\nsleep 60\n");
        let start = std::time::Instant::now();
        let d = detect_native(&fx.home, fx.tmux_str());
        assert!(d.tmux.is_none());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "two 2s probes must bound total detection time, took {:?}",
            start.elapsed()
        );
    }
}
