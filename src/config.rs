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
    pub buttons: ButtonsConfig,
    pub tmux: TmuxConfig,
    pub launchers: HashMap<String, LauncherAction>,
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
            share: "".into(),    // unmapped
            options: "".into(),  // unmapped
            touchpad: "".into(), // unmapped
            cross: "enter".into(),
            circle: "escape".into(),
            triangle: "tab".into(),
            l3: "ctrl+t".into(),
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

/// Left stick as mouse cursor configuration.
///
/// When enabled, deflecting the left analog stick moves the mouse cursor.
/// Speed is proportional to deflection; a sub-pixel accumulator ensures smooth
/// movement even at low sensitivity values.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StickMouseConfig {
    /// Enable left stick cursor control.
    pub enabled: bool,
    /// Max pixels per input frame at full deflection. Default: 8.0.
    pub sensitivity: f32,
    /// Dead zone radius around center (0-127). Default: 15.
    pub dead_zone: u8,
}

impl Default for StickMouseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 8.0,
            dead_zone: 15,
        }
    }
}

/// Touchpad-as-mouse configuration.
///
/// When enabled, sliding a finger on the DualSense touchpad moves the cursor,
/// and pressing (clicking) the touchpad sends a left mouse button click.
/// DS4 touchpad coordinates are not yet supported.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TouchpadConfig {
    /// Enable touchpad cursor control. Set false to use the touchpad button in tmux mappings.
    pub enabled: bool,
    /// Cursor speed multiplier. 1.0 = raw touchpad units → pixels 1:1. Default 1.5.
    pub sensitivity: f32,
}

impl Default for TouchpadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 1.5,
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
            tmux: TmuxConfig::default(),
            launchers,
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
                    log::info!("Loaded config from {config_path}");
                    Self::merge_default_launchers(&mut config.launchers);
                    config
                }
                Err(e) => {
                    log::warn!("Failed to parse config file {config_path}: {e}. Using defaults.");
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("No config file found at {config_path}. Using defaults.");
                Self::default()
            }
        }
    }
}

fn config_file_path() -> String {
    if let Ok(appdata) = std::env::var("APPDATA") {
        format!("{appdata}\\ds4cc\\config.toml")
    } else {
        "ds4cc.toml".into()
    }
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
