/// TOML configuration with sensible defaults.
/// No config file is required to run — defaults work out of the box.
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lightbar: ColorConfig,
    pub scroll: ScrollConfig,
    pub stick_mouse: StickMouseConfig,
    pub touchpad: TouchpadConfig,
    /// Profile P1 button map (active at startup). PS cycles P1→P2→P3→P4→P1.
    pub buttons: ButtonsConfig,
    /// Optional maps for P2–P4. Missing → ship [`ButtonsConfig::default`].
    #[serde(default)]
    pub profile_1: Option<ButtonsConfig>,
    #[serde(default)]
    pub profile_2: Option<ButtonsConfig>,
    #[serde(default)]
    pub profile_3: Option<ButtonsConfig>,
    pub tmux: TmuxConfig,
    pub launchers: HashMap<String, LauncherAction>,
    pub codex_micro: CodexMicroConfig,
    pub voice: VoiceConfig,
}

/// Optional voice-app integration.
///
/// `app_command` names an external voice app to launch from the tray
/// ("Open voice app"). Used on Linux; ignored on Windows (which keeps the
/// built-in Wispr Flow integration). Empty = feature disabled.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// Absolute argv-style command is NOT split here — launcher/tray code
    /// spawns it without a shell. Empty string = unset.
    pub app_command: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            app_command: String::new(),
        }
    }
}

/// Opt-in Codex Micro semantic layer. Existing controller behavior is unchanged
/// unless `enabled` is explicitly set.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CodexMicroConfig {
    pub enabled: bool,
    /// Deprecated compatibility switch. It no longer enables a fake transport.
    pub demo_mode: bool,
    /// Absolute executable, or `codex` to resolve it with PATH.
    pub codex_executable: String,
    /// Optional working directory for new threads and skills/list.
    pub cwd: String,
    pub request_timeout_ms: u64,
    pub reconnect_min_ms: u64,
    pub reconnect_max_ms: u64,
    pub composer_limit: usize,
    /// Absolute argv for a local speech-to-text adapter. No shell is used.
    pub voice_argv: Vec<String>,
    pub voice_timeout_ms: u64,
    pub voice_output_limit: usize,
    pub brightness: u8,
    pub inactivity_seconds: u64,
    pub analog_dead_zone: u8,
    pub analog_hysteresis: u8,
    /// recent | pinned | priority | custom
    pub source_policy: String,
    /// Exact thread IDs, in order, when source_policy = "custom".
    pub custom_order: Vec<String>,
    pub commands: HashMap<String, String>,
    pub skills: HashMap<String, String>,
    /// up/down/left/right values are prompt text submitted as a turn.
    pub cardinal_actions: HashMap<String, String>,
}

impl Default for CodexMicroConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            demo_mode: false,
            codex_executable: "codex".into(),
            cwd: String::new(),
            request_timeout_ms: 15_000,
            reconnect_min_ms: 250,
            reconnect_max_ms: 8_000,
            composer_limit: 16_384,
            voice_argv: Vec::new(),
            voice_timeout_ms: 30_000,
            voice_output_limit: 16_384,
            brightness: 70,
            inactivity_seconds: 180,
            analog_dead_zone: 48,
            analog_hysteresis: 12,
            source_policy: "recent".into(),
            custom_order: Vec::new(),
            commands: HashMap::new(),
            skills: HashMap::new(),
            cardinal_actions: HashMap::new(),
        }
    }
}

impl CodexMicroConfig {
    pub fn normalize(&mut self) {
        self.brightness = self.brightness.min(100);
        self.analog_dead_zone = self.analog_dead_zone.clamp(1, 127);
        self.analog_hysteresis = self
            .analog_hysteresis
            .min(self.analog_dead_zone.saturating_sub(1));
        self.request_timeout_ms = self.request_timeout_ms.clamp(100, 120_000);
        self.reconnect_min_ms = self.reconnect_min_ms.clamp(50, 30_000);
        self.reconnect_max_ms = self
            .reconnect_max_ms
            .max(self.reconnect_min_ms)
            .min(120_000);
        self.composer_limit = self.composer_limit.clamp(1, 1_048_576);
        self.voice_timeout_ms = self.voice_timeout_ms.clamp(100, 300_000);
        self.voice_output_limit = self.voice_output_limit.clamp(1, 1_048_576);
        for prompt in self
            .commands
            .values_mut()
            .chain(self.cardinal_actions.values_mut())
        {
            if prompt.chars().count() > self.composer_limit {
                *prompt = prompt.chars().take(self.composer_limit).collect();
            }
        }
    }

    pub fn runtime_active(&self) -> bool {
        self.enabled
    }
}

/// Named launcher actions for "launcher:<name>" button values.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct LauncherAction {
    /// Unicode text to emit.
    pub text: String,
    /// Whether to submit Enter after emitting text.
    pub enter: bool,
}

/// RGB color. Used for the static lightbar color.
#[derive(Debug, Clone, Deserialize)]
pub struct ColorConfig {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            r: 255,
            g: 140,
            b: 0,
        } // orange
    }
}

/// Right stick scroll configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScrollConfig {
    /// Dead zone radius around center (0-127). Values within this range are ignored.
    pub dead_zone: u8,
    /// Scroll speed multiplier. 1.0 = normal, 2.0 = double speed.
    pub sensitivity: f32,
    /// Enable horizontal scrolling (X axis).
    pub horizontal: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            dead_zone: 20,
            sensitivity: 1.0,
            horizontal: true,
        }
    }
}

/// Button → action mapping.
///
/// Each value is resolved at startup, in priority order:
///   1. Empty string → unmapped (button does nothing)
///   2. Tmux action name (e.g., "previous-window") → prefix + detected key
///   3. Claude Code action name (e.g., "chat:cycleMode") → detected key sequence
///      from ~/.claude/keybindings.json
///   4. Launcher action name (e.g., "launcher:godspeed") → Unicode text
///   5. Direct key combo (e.g., "ctrl+g", "Shift+7")
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ButtonsConfig {
    pub l1: String,
    pub r1: String,
    pub r2: String,
    pub square: String,
    pub share: String,
    pub options: String,
    pub touchpad: String,
    pub cross: String,
    pub circle: String,
    pub triangle: String,
    pub l3: String,
    pub r3: String,
}

impl Default for ButtonsConfig {
    fn default() -> Self {
        Self {
            l1: "previous-window".into(),
            r1: "next-window".into(),
            r2: "kill-window".into(),
            square: "new-window".into(),
            share: "".into(),   // unmapped
            options: "".into(), // unmapped
            // Physical touchpad press → new tmux window with cwd = home.
            // Prefer a matching bind in tmux (`new-window -c ~`); falls back
            // to the default `c` key (prefix then C).
            touchpad: "new-window -c ~".into(),
            cross: "enter".into(),
            circle: "escape".into(),
            triangle: "tab".into(),
            // L3 → type "| godspeed", 16 ms, Enter↓, 10 ms, Enter↑
            l3: "launcher:godspeed".into(),
            r3: "ctrl+u".into(),
        }
    }
}

/// Tmux detection configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TmuxConfig {
    /// Auto-detect prefix and key bindings from tmux via WSL.
    /// When false, `prefix` and hardcoded tmux defaults are used as-is.
    pub auto_detect: bool,
    /// Tmux prefix key combo (e.g., "Ctrl+B"). Used as fallback if auto-detect fails.
    pub prefix: String,
}

impl Default for TmuxConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            prefix: "Ctrl+B".into(), // tmux default, overridden by auto-detect
        }
    }
}

/// Left stick as **precise** mouse cursor control.
///
/// Runs alongside the touchpad (fast swipe). Stick uses a soft response curve
/// near center so small deflections move the cursor slowly; max pixels/frame
/// stays low by default.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StickMouseConfig {
    /// Enable left stick cursor control.
    pub enabled: bool,
    /// Max pixels per input frame at full deflection. Default: 7.0 (high).
    pub sensitivity: f32,
    /// Dead zone radius around center (0-127). Default: 6.
    pub dead_zone: u8,
}

impl Default for StickMouseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 7.0,
            dead_zone: 6,
        }
    }
}

/// Touchpad as **fast** mouse cursor control.
///
/// Swipe scales more aggressively than the stick. Physical press still uses
/// `[buttons].touchpad` (or left-click when that string is empty).
/// DS4 touchpad coordinates are not yet supported.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TouchpadConfig {
    /// Enable touchpad cursor control. Set false to use only stick / button maps.
    pub enabled: bool,
    /// Cursor speed multiplier on raw pad deltas. Default 10.0 (very fast).
    pub sensitivity: f32,
}

impl Default for TouchpadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 10.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut launchers = HashMap::new();
        launchers.insert(
            "godspeed".to_string(),
            LauncherAction {
                text: "| godspeed".to_string(),
                enter: true,
            },
        );
        Self {
            lightbar: ColorConfig::default(),
            scroll: ScrollConfig::default(),
            stick_mouse: StickMouseConfig::default(),
            touchpad: TouchpadConfig::default(),
            buttons: ButtonsConfig::default(),
            profile_1: None,
            profile_2: None,
            profile_3: None,
            tmux: TmuxConfig::default(),
            launchers,
            codex_micro: CodexMicroConfig::default(),
            voice: VoiceConfig::default(),
        }
    }
}

impl Config {
    /// Inject built-in launcher actions into `map` without overriding user entries.
    /// Called after loading a config file so built-ins are always resolvable.
    fn merge_default_launchers(map: &mut HashMap<String, LauncherAction>) {
        // Built-in: | godspeed + Enter — matches claude-launcher's proven behaviour.
        // Unassigned by default (no button maps to it unless user configures one).
        map.entry("godspeed".to_string()).or_insert(LauncherAction {
            text: "| godspeed".to_string(),
            enter: true,
        });
    }

    /// Load config from the default config file path, or return defaults if not found.
    pub fn load() -> Self {
        let config_path = config_file_path();
        match std::fs::read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<Self>(&contents) {
                Ok(mut config) => {
                    log::info!("Loaded config from {}", config_path.display());
                    Self::merge_default_launchers(&mut config.launchers);
                    config.codex_micro.normalize();
                    #[cfg(target_os = "linux")]
                    if config.codex_micro.enabled {
                        log::warn!(
                            "[codex_micro] enabled, but the codex runtime is Windows-only; ignored on Linux"
                        );
                    }
                    config
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse config file {}: {e}. Using defaults.",
                        config_path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => {
                log::info!(
                    "No config file found at {}. Using defaults.",
                    config_path.display()
                );
                Self::default()
            }
        }
    }
}

/// Config file path: `platform::config_dir()/config.toml`.
///
/// Windows: `%APPDATA%\ds4cc\config.toml` (legacy behavior; when APPDATA is
/// unset, `config_dir()` is empty and the legacy relative `ds4cc.toml`
/// fallback is preserved). Linux: `$XDG_CONFIG_HOME/ds4cc/config.toml` or
/// `~/.config/ds4cc/config.toml`.
fn config_file_path() -> std::path::PathBuf {
    let dir = crate::platform::config_dir();
    if dir.as_os_str().is_empty() {
        // Legacy Windows fallback when %APPDATA% is unavailable.
        return std::path::PathBuf::from("ds4cc.toml");
    }
    dir.join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = Config::default();
        assert_eq!(config.lightbar.r, 255);
        assert_eq!(config.lightbar.g, 140);
        assert_eq!(config.tmux.prefix, "Ctrl+B");
        assert_eq!(config.buttons.cross, "enter");
        assert_eq!(config.buttons.circle, "escape");
        assert_eq!(config.buttons.square, "new-window");
        assert_eq!(config.buttons.touchpad, "new-window -c ~");
        let gs = config
            .launchers
            .get("godspeed")
            .expect("godspeed launcher must exist");
        assert_eq!(gs.text, "| godspeed");
        assert!(gs.enter, "godspeed built-in must submit Enter");
    }

    #[test]
    fn deserialize_partial_toml() {
        let toml_str = r#"
            [lightbar]
            r = 100
            g = 100
            b = 100

            [tmux]
            prefix = "Ctrl+A"

            [buttons]
            share = "chat:cycleMode"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lightbar.r, 100);
        assert_eq!(config.tmux.prefix, "Ctrl+A");
        assert_eq!(config.buttons.share, "chat:cycleMode");
        // Other fields should be defaults
        assert_eq!(config.scroll.dead_zone, 20);
        assert_eq!(config.buttons.l1, "previous-window");
        assert!(config.touchpad.enabled);
        assert!(!config.codex_micro.enabled);
        assert_eq!(config.codex_micro.source_policy, "recent");
        assert!(config.codex_micro.custom_order.is_empty());
    }

    #[test]
    fn deserialize_partial_codex_micro_uses_defaults_and_normalizes() {
        let mut config: Config = toml::from_str(
            r#"
            [codex_micro]
            enabled = true
            analog_dead_zone = 0
            analog_hysteresis = 200
            "#,
        )
        .unwrap();
        assert!(!config.codex_micro.demo_mode);
        assert_eq!(config.codex_micro.brightness, 70);
        assert_eq!(config.codex_micro.inactivity_seconds, 180);
        config.codex_micro.normalize();
        assert_eq!(config.codex_micro.analog_dead_zone, 1);
        assert_eq!(config.codex_micro.analog_hysteresis, 0);
    }
    #[test]
    fn configured_prompt_bodies_are_bounded() {
        let mut cfg = CodexMicroConfig {
            composer_limit: 3,
            ..Default::default()
        };
        cfg.commands.insert("x".into(), "abcdef".into());
        cfg.cardinal_actions.insert("up".into(), "uvwxyz".into());
        cfg.normalize();
        assert_eq!(cfg.commands["x"], "abc");
        assert_eq!(cfg.cardinal_actions["up"], "uvw");
    }

    #[test]
    fn deserialize_launcher_action_with_enter() {
        let toml_str = r#"
            [launchers.godspeed]
            text = "| godspeed 🚀"
            enter = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let action = config
            .launchers
            .get("godspeed")
            .expect("expected launcher action");
        assert_eq!(action.text, "| godspeed 🚀");
        assert!(action.enter);
    }

    #[test]
    fn deserialize_launcher_action_without_enter_defaults_false() {
        let toml_str = r#"
            [launchers.myaction]
            text = "custom text"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let action = config
            .launchers
            .get("myaction")
            .expect("expected launcher action");
        assert_eq!(action.text, "custom text");
        assert!(!action.enter, "enter should default to false");
    }

    #[test]
    fn deserialize_backward_compat_no_launcher_section() {
        // Old configs without [launchers] must still parse and get built-in defaults
        let toml_str = r#"
            [lightbar]
            r = 200
            g = 100
            b = 50
        "#;
        let mut config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lightbar.r, 200);
        // Simulate what Config::load does after parsing
        Config::merge_default_launchers(&mut config.launchers);
        let gs = config
            .launchers
            .get("godspeed")
            .expect("godspeed must exist after merge");
        assert_eq!(gs.text, "| godspeed");
        assert!(gs.enter);
    }

    #[test]
    fn voice_section_defaults_to_empty() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.voice.app_command, "");
    }

    #[test]
    fn deserialize_voice_app_command() {
        let config: Config = toml::from_str(
            r#"
            [voice]
            app_command = "/usr/bin/wispr-flow"
            "#,
        )
        .unwrap();
        assert_eq!(config.voice.app_command, "/usr/bin/wispr-flow");
    }

    #[test]
    fn codex_micro_stays_parseable_with_voice_section() {
        // Schema is OS-neutral: [codex_micro] must parse alongside [voice].
        let config: Config = toml::from_str(
            r#"
            [codex_micro]
            enabled = true

            [voice]
            app_command = ""
            "#,
        )
        .unwrap();
        assert!(config.codex_micro.enabled);
        assert_eq!(config.voice.app_command, "");
    }

    #[test]
    fn deserialize_launcher_unicode_text() {
        let toml_str = r#"
            [launchers.emoji]
            text = "🎮 gaming time 🎮"
            enter = false
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let action = config
            .launchers
            .get("emoji")
            .expect("expected emoji launcher");
        assert_eq!(action.text, "🎮 gaming time 🎮");
        assert!(!action.enter);
    }
}
