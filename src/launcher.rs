/// Launcher action catalog and utilities.
///
/// Launcher actions are addressed in config as `launcher:<name>`. When a
/// button fires, the mapper emits `Action::LauncherText { text, enter }`.
/// The action worker in `main.rs` calls `send_launcher_text` via a tokio
/// channel — **off the HID polling loop** — so the poll cadence never sleeps
/// or drops controller frames.  Rapid repeated presses are serialized through
/// the channel's FIFO order.
///
/// ## Built-in catalog
///
/// Built-in actions are available without any config entry.  They can be
/// overridden by adding a same-named entry under `[launchers]` in config.toml.
///
/// | Name        | Text            | Enter | Notes                     |
/// |-------------|-----------------|-------|---------------------------|
/// | `godspeed`  | `\| godspeed`   | yes   | Mirrors claude-launcher.  |
///
/// Stock default: **L3** maps to `launcher:godspeed`. Other buttons stay free.
/// Override or reassign:
/// ```toml
/// [buttons]
/// l3 = "launcher:godspeed"
/// share = "launcher:godspeed"
/// ```
///
/// ## User-defined actions
///
/// ```toml
/// [launchers.myaction]
/// text  = "custom text 🎮"
/// enter = true   # press Enter after text (default: false)
/// ```
use crate::config::LauncherAction;

/// Return the built-in `LauncherAction` for `name`, or `None` if not a built-in.
///
/// Users who want to customise a built-in add an entry to `[launchers]` in
/// config.toml; the mapper's resolution order (user config → built-in) means
/// their entry takes precedence, so this function is only reached when no user
/// override exists.
pub fn builtin_action(name: &str) -> Option<LauncherAction> {
    match name {
        // Replicates claude-launcher exactly: types "| godspeed" then Enter.
        "godspeed" => Some(LauncherAction {
            text: "| godspeed".to_string(),
            enter: true,
        }),
        _ => None,
    }
}

// ── Voice app launching (Linux) ───────────────────────────────────────
//
// The Windows tray has "Open Wispr Flow" (a Windows exe). On Linux there is
// no bundled voice app: the user points `[voice] app_command` at whatever
// they use, and the tray's "Open voice app" item calls this. No shell is
// involved — the command string is split into argv and spawned directly.

/// Spawn the configured voice app from `[voice] app_command`.
///
/// Returns `true` if the process was spawned. Empty/whitespace command is a
/// no-op with a log line (the tray hides the item in that case); a spawn
/// failure is logged and non-fatal.
#[cfg(target_os = "linux")]
pub fn launch_voice_app(app_command: &str) -> bool {
    let argv: Vec<&str> = app_command.split_whitespace().collect();
    let Some((prog, args)) = argv.split_first() else {
        log::debug!("[voice] app_command is empty — voice app launch is a no-op");
        return false;
    };
    match std::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_child) => {
            log::info!("Launched voice app: {app_command}");
            true
        }
        Err(e) => {
            log::warn!("Failed to launch voice app '{app_command}': {e}");
            false
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ButtonsConfig, Config, LauncherAction};
    use crate::detect::Detected;
    use crate::input::UnifiedInput;
    use crate::mapper::{Action, MapperState};
    use std::sync::{Arc, atomic::AtomicBool};

    // ── Helpers ───────────────────────────────────────────────────────

    fn cfg_with_button(button_field: &str, value: &str) -> Config {
        let mut cfg = Config::default();
        match button_field {
            "share" => cfg.buttons.share = value.into(),
            "options" => cfg.buttons.options = value.into(),
            "cross" => cfg.buttons.cross = value.into(),
            "circle" => cfg.buttons.circle = value.into(),
            _ => panic!("unknown button field in test helper: {button_field}"),
        }
        cfg
    }

    fn cfg_with_launcher(button_field: &str, action_name: &str, text: &str, enter: bool) -> Config {
        let mut cfg = cfg_with_button(button_field, &format!("launcher:{action_name}"));
        cfg.launchers.insert(
            action_name.to_string(),
            LauncherAction {
                text: text.to_string(),
                enter,
            },
        );
        cfg
    }

    fn mapper_from(cfg: &Config) -> MapperState {
        MapperState::new(cfg, &Detected::default(), Arc::new(AtomicBool::new(false)))
    }

    fn press(field: &str) -> UnifiedInput {
        let mut i = UnifiedInput::default();
        match field {
            "share" => i.buttons.share = true,
            "options" => i.buttons.options = true,
            "cross" => i.buttons.cross = true,
            "circle" => i.buttons.circle = true,
            _ => panic!("unknown field: {field}"),
        }
        i
    }

    fn launcher_text(actions: &[Action]) -> Option<(&str, bool)> {
        actions.iter().find_map(|a| match a {
            Action::LauncherText { text, enter } => Some((text.as_str(), *enter)),
            _ => None,
        })
    }

    // ── Built-in catalog ──────────────────────────────────────────────

    #[test]
    fn builtin_godspeed_text_and_enter() {
        let a = builtin_action("godspeed").expect("godspeed must be a built-in");
        assert_eq!(
            a.text, "| godspeed",
            "text must match claude-launcher exactly"
        );
        assert!(a.enter, "godspeed built-in must press Enter");
        // Sole built-in payload is pure ASCII "| godspeed" — no hidden bytes.
        assert!(a.text.is_ascii(), "built-in payload must be ASCII");
        assert_eq!(
            a.text.as_bytes(),
            b"| godspeed",
            "exact ASCII bytes, no extra payload"
        );
        // enter=true submits with a single Return (one LF); the text carries no
        // trailing newline itself, so submission adds exactly one LF and nothing else.
        assert!(
            !a.text.contains('\n'),
            "text must not embed its own newline"
        );
    }

    #[test]
    fn builtin_unknown_name_returns_none() {
        assert!(builtin_action("no-such-action").is_none());
        assert!(builtin_action("").is_none());
    }

    // ── Parsing: launcher: prefix ─────────────────────────────────────

    #[test]
    fn launcher_prefix_resolves_to_launcher_text_action() {
        let cfg = cfg_with_launcher("share", "godspeed", "| godspeed", true);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        assert_eq!(
            actions.len(),
            1,
            "exactly one action for launcher button press"
        );
        assert!(
            matches!(&actions[0], Action::LauncherText { .. }),
            "action must be LauncherText, got: {:?}",
            actions[0]
        );
    }

    #[test]
    fn launcher_prefix_extracts_correct_name() {
        // Mapper inlines the resolved text, not the name — verify via text roundtrip
        let cfg = cfg_with_launcher("share", "myaction", "hello world", false);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (text, enter) = launcher_text(&actions).expect("expected LauncherText");
        assert_eq!(text, "hello world");
        assert!(!enter);
    }

    // ── Built-in fallback (no config entry needed) ────────────────────

    #[test]
    fn builtin_godspeed_works_without_config_entry() {
        // Remove godspeed from launchers map to force built-in fallback
        let mut cfg = cfg_with_button("share", "launcher:godspeed");
        cfg.launchers.remove("godspeed");
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (text, enter) = launcher_text(&actions).expect("built-in must resolve without config");
        assert_eq!(text, "| godspeed");
        assert!(enter);
    }

    // ── Unicode text ──────────────────────────────────────────────────

    #[test]
    fn unicode_emoji_in_text() {
        let cfg = cfg_with_launcher("share", "wave", "🌊 héllo wörld 🎮", false);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (text, _) = launcher_text(&actions).expect("expected LauncherText");
        assert_eq!(text, "🌊 héllo wörld 🎮");
    }

    #[test]
    fn multibyte_surrogate_pair_text() {
        // "😀" is a surrogate pair in UTF-16 (U+1F600, code units 0xD83D 0xDE00)
        let cfg = cfg_with_launcher("share", "smiley", "😀", true);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (text, enter) = launcher_text(&actions).expect("expected LauncherText");
        assert_eq!(text, "😀");
        assert!(enter);
    }

    // ── Optional Enter ────────────────────────────────────────────────

    #[test]
    fn submit_false_does_not_press_enter() {
        let cfg = cfg_with_launcher("share", "noenter", "just text", false);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (_, enter) = launcher_text(&actions).expect("expected LauncherText");
        assert!(!enter, "submit=false must not add Enter");
    }

    #[test]
    fn submit_true_presses_enter() {
        let cfg = cfg_with_launcher("share", "withenter", "text", true);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        let (_, enter) = launcher_text(&actions).expect("expected LauncherText");
        assert!(enter, "submit=true must add Enter");
    }

    // ── Action ordering ───────────────────────────────────────────────

    #[test]
    fn launcher_action_before_regular_in_same_frame() {
        // cross=launcher, circle=ctrl+g; both pressed simultaneously
        let mut cfg = cfg_with_launcher("cross", "gs", "x", false);
        cfg.buttons.circle = "ctrl+g".into();
        let mut mapper = mapper_from(&cfg);

        let mut input = UnifiedInput::default();
        input.buttons.cross = true;
        input.buttons.circle = true;
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 2);
        // cross (LauncherText) comes before circle (KeyCombo)
        assert!(matches!(&actions[0], Action::LauncherText { .. }));
        assert!(
            matches!(&actions[1], Action::KeyCombo(k) if *k == vec![crate::mapper::VKey::Control, crate::mapper::VKey::G])
        );
    }

    // ── Rapid presses serialized ──────────────────────────────────────

    #[test]
    fn rapid_presses_each_produce_one_action() {
        let cfg = cfg_with_launcher("share", "gs", "x", false);
        let mut mapper = mapper_from(&cfg);

        let p = press("share");
        let r = UnifiedInput::default(); // release

        let a1 = mapper.update(&p);
        assert_eq!(a1.len(), 1);
        let _ = mapper.update(&r);
        let a2 = mapper.update(&p);
        assert_eq!(a2.len(), 1);
        let _ = mapper.update(&r);
        let a3 = mapper.update(&p);
        assert_eq!(a3.len(), 1);

        // All three produce identical actions
        let t1 = launcher_text(&a1).unwrap().0;
        let t2 = launcher_text(&a2).unwrap().0;
        let t3 = launcher_text(&a3).unwrap().0;
        assert_eq!(t1, t2);
        assert_eq!(t2, t3);
    }

    #[test]
    fn held_button_does_not_repeat_launcher_action() {
        let cfg = cfg_with_launcher("share", "gs", "x", false);
        let mut mapper = mapper_from(&cfg);

        let p = press("share");
        let a1 = mapper.update(&p);
        assert_eq!(a1.len(), 1, "first press fires once");

        // Hold for several frames: no repeat (not a repeating key)
        for _ in 0..5 {
            let held = mapper.update(&p);
            assert!(held.is_empty(), "held launcher button must not repeat");
        }
    }

    // ── Unknown launcher name → unmapped ─────────────────────────────

    #[test]
    fn unknown_launcher_name_produces_no_action() {
        let cfg = cfg_with_button("share", "launcher:no-such-action");
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        assert!(actions.is_empty(), "unknown launcher name must be unmapped");
    }

    #[test]
    fn empty_launcher_text_produces_no_action() {
        let cfg = cfg_with_launcher("share", "empty", "", false);
        let mut mapper = mapper_from(&cfg);
        let actions = mapper.update(&press("share"));
        assert!(actions.is_empty(), "empty text must be unmapped");
    }

    // ── Unchanged defaults ────────────────────────────────────────────

    #[test]
    fn only_l3_default_maps_to_godspeed_launcher() {
        let b = ButtonsConfig::default();
        assert_eq!(b.l3, "launcher:godspeed");
        let others = [
            &b.l1,
            &b.r1,
            &b.r2,
            &b.square,
            &b.share,
            &b.options,
            &b.touchpad,
            &b.cross,
            &b.circle,
            &b.triangle,
            &b.r3,
        ];
        let mapped = others.iter().filter(|v| v.starts_with("launcher:")).count();
        assert_eq!(
            mapped, 0,
            "only L3 is pre-wired to a launcher action (godspeed)"
        );
    }

    #[test]
    fn default_l1_still_maps_to_tmux_prev_window() {
        let mut mapper = MapperState::default();
        let mut i = UnifiedInput::default();
        i.buttons.l1 = true;
        let actions = mapper.update(&i);
        assert!(
            actions.iter().any(|a| matches!(a, Action::KeySequence(seq)
                if seq[0] == vec![crate::mapper::VKey::Control, crate::mapper::VKey::B]
                && seq[1] == vec![crate::mapper::VKey::P]
            )),
            "L1 default must still be tmux prev-window"
        );
    }

    #[test]
    fn default_cross_still_maps_to_enter() {
        let mut mapper = MapperState::default();
        let mut i = UnifiedInput::default();
        i.buttons.cross = true;
        let actions = mapper.update(&i);
        assert!(
            actions.iter().any(
                |a| matches!(a, Action::KeyCombo(k) if *k == vec![crate::mapper::VKey::Return])
            ),
            "Cross default must still be Enter"
        );
    }

    #[test]
    fn default_r3_still_maps_to_ctrl_u() {
        let mut mapper = MapperState::default();
        let mut i = UnifiedInput::default();
        i.buttons.r3 = true;
        let actions = mapper.update(&i);
        assert!(
            actions.iter().any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![crate::mapper::VKey::Control, crate::mapper::VKey::U])),
            "R3 default must still be Ctrl+U"
        );
    }

    // ── Voice app launching (Linux) ───────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn voice_app_empty_command_is_noop() {
        assert!(!launch_voice_app(""));
        assert!(!launch_voice_app("   "));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn voice_app_missing_binary_fails_gracefully() {
        assert!(!launch_voice_app("/definitely/not/a/real/voice-app --flag"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn voice_app_spawns_argv_without_shell() {
        // A script that records its argv; proves no-shell argv splitting.
        let dir = std::env::temp_dir().join(format!("ds4cc-voice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("marker");
        let script = dir.join("voice.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s' \"$1 $2\" > '{}'\n",
                marker.display()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cmd = format!("{} arg1 arg2", script.display());
        assert!(launch_voice_app(&cmd));

        // Wait briefly for the spawned script to write the marker.
        let mut contents = String::new();
        for _ in 0..50 {
            if let Ok(c) = std::fs::read_to_string(&marker) {
                contents = c;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(contents, "arg1 arg2");
    }
}
