/// Unified first-run keybind detection — one WSL round-trip for everything.
///
/// A single `wsl bash -lc` invocation fetches four text sections separated by
/// markers: tmux server prefix, tmux prefix-table bindings, ~/.tmux.conf, and
/// ~/.claude/keybindings.json. Each section is parsed by a pure function so
/// all parsing is unit-testable without WSL.
///
/// Everything is best-effort: missing tmux, missing Claude Code, or missing
/// WSL simply yields empty sections and the mapper falls back to defaults.

use crate::mapper::{parse_key_combo, VKey};
use crate::tmux_detect::{self, TmuxDetected};
use crate::wsl::run_wsl;
use std::collections::HashMap;

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

const MARKER: &str = "===DS4CC===";

/// Run the single detection probe via WSL and parse all sections.
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
            let Some(action) = action.as_str() else { continue };
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

    normalized
        .split_whitespace()
        .map(parse_key_combo)
        .collect()
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
        assert_eq!(
            map["app:toggleTodos"],
            vec![vec![VKey::Control, VKey::T]]
        );
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
        assert_eq!(
            map["chat:thinkingToggle"],
            vec![vec![VKey::Alt, VKey::T]]
        );
    }

    #[test]
    fn parses_two_chord_sequence() {
        let map = parse_claude_keybindings(SAMPLE);
        assert_eq!(
            map["chat:killAgents"],
            vec![
                vec![VKey::Control, VKey::X],
                vec![VKey::Control, VKey::K]
            ]
        );
    }

    #[test]
    fn empty_or_invalid_json_yields_empty_map() {
        assert!(parse_claude_keybindings("").is_empty());
        assert!(parse_claude_keybindings("not json").is_empty());
    }
}
