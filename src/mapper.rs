/// Shortcut **mapper**: [`UnifiedInput`] → [`Action`] (then OS inject).
///
/// **Does not** open HID, validate BT CRC, or know USB vs Bluetooth. That
/// lives in `crate::state` + `crate::input` + `crate::hid`. The only contract
/// from the controller is [`crate::input::UnifiedInput`].
///
/// ## Profiles (PS button)
///
/// Four input profiles (P1–P4). **PS** rising edge cycles
/// `0 → 1 → 2 → 3 → 0`. DualSense player-indicator LEDs (five dots under the
/// touchpad) show the active profile — same scheme as the original 2-profile
/// design, extended to four masks in [`PROFILE_PLAYER_LEDS`].
///
/// Profile 0 button map comes from `[buttons]`; profiles 1–3 from optional
/// `[profile_1]` / `[profile_2]` / `[profile_3]` (else ship defaults).
///
/// Fixed mappings (always active, all profiles):
///   D-pad, left/right stick mouse/scroll, L2 hold Ctrl+Super
///
/// Combos are delivered atomically by the platform injector (a single
/// `SendInput` batch on Windows; one uinput event burst + SYN on Linux).
use crate::config::{ButtonsConfig, LauncherAction, TmuxConfig};
use crate::detect::Detected;
use crate::input::{ButtonState, DPad, UnifiedInput};
use crate::keys::Key;
use crate::platform::Injector;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

/// Number of input profiles cycled by the PS button.
pub const PROFILE_COUNT: usize = 4;

/// DualSense player-indicator LED masks (bits 0–4 = five dots left→right).
///
/// Matches the original elegant 2-profile design and extends it to P3/P4
/// using the same bit layout Sony uses for player assignment:
/// - P1 `0x04` — center only  
/// - P2 `0x0A` — inner two (center-left + center-right)  
/// - P3 `0x1B` — outer four (no center)  
/// - P4 `0x1F` — all five  
pub const PROFILE_PLAYER_LEDS: [u8; PROFILE_COUNT] = [0x04, 0x0A, 0x1B, 0x1F];

/// Active profile index `0..PROFILE_COUNT` (P1..=P4 on the controller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile(pub u8);

impl Profile {
    pub const P1: Self = Self(0);
    pub fn index(self) -> usize {
        self.0 as usize % PROFILE_COUNT
    }
    pub fn led_mask(self) -> u8 {
        PROFILE_PLAYER_LEDS[self.index()]
    }
    pub fn next(self) -> Self {
        Self(((self.0 as usize + 1) % PROFILE_COUNT) as u8)
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "P{}", self.index() + 1)
    }
}

/// Milliseconds to wait after the full launcher text has been injected, before
/// pressing Return. Direct clone of claude-launcher's two-phase submit: the text
/// is delivered as one instantaneous batch (zero per-character delay), then—after
/// this guard—Return fires, so the focused app never sees the text and Enter
/// racing. Shared verbatim by the Windows `SendInput` and Linux `wtype`/`xdotool`
/// paths so submit timing is identical on every platform.
pub const ENTER_DELAY_MS: u64 = 16;

/// After all keys in a multi-key combo are down, hold before releasing.
/// Example for Triangle → `ctrl+n`:
/// `Ctrl↓` + `n↓` → 19 ms → `Ctrl↑` + `n↑`.
pub const COMBO_HOLD_MS: u64 = 19;

/// Gap between releasing modifiers and releasing the main key when a combo
/// uses staggered release (e.g. Options `ctrl+\`: mods up, wait, main up).
/// Enter after launcher text also reuses this (10 ms).
pub const COMBO_MAIN_RELEASE_GAP_MS: u64 = 10;

#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VK_SHIFT,
};

/// Virtual key codes we use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VKey {
    Return,
    Escape,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Alt,
    Shift,
    Control,
    Win,
    // Letter keys
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // Digit keys
    D0,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    // Punctuation / symbols
    Semicolon,    // VK_OEM_1 (;:)
    LeftBracket,  // VK_OEM_4 ([{)
    RightBracket, // VK_OEM_6 (]})
    Backslash,    // VK_OEM_5 (\|)
    Quote,        // VK_OEM_7 ('")
    Slash,        // VK_OEM_2 (/?)
    Minus,        // VK_OEM_MINUS (-_)
    Equals,       // VK_OEM_PLUS (=+)
    Comma,        // VK_OEM_COMMA (,<)
    Period,       // VK_OEM_PERIOD (.>)
    Backtick,     // VK_OEM_3 (`~)
    Space,        // VK_SPACE
    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl VKey {
    /// Parse a key name string into a VKey.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "return" | "enter" => Some(VKey::Return),
            "escape" | "esc" => Some(VKey::Escape),
            "tab" => Some(VKey::Tab),
            "up" => Some(VKey::Up),
            "down" => Some(VKey::Down),
            "left" => Some(VKey::Left),
            "right" => Some(VKey::Right),
            "alt" => Some(VKey::Alt),
            "shift" => Some(VKey::Shift),
            "ctrl" | "control" => Some(VKey::Control),
            "win" | "windows" | "super" | "meta" => Some(VKey::Win),
            "a" => Some(VKey::A),
            "b" => Some(VKey::B),
            "c" => Some(VKey::C),
            "d" => Some(VKey::D),
            "e" => Some(VKey::E),
            "f" => Some(VKey::F),
            "g" => Some(VKey::G),
            "h" => Some(VKey::H),
            "i" => Some(VKey::I),
            "j" => Some(VKey::J),
            "k" => Some(VKey::K),
            "l" => Some(VKey::L),
            "m" => Some(VKey::M),
            "n" => Some(VKey::N),
            "o" => Some(VKey::O),
            "p" => Some(VKey::P),
            "q" => Some(VKey::Q),
            "r" => Some(VKey::R),
            "s" => Some(VKey::S),
            "t" => Some(VKey::T),
            "u" => Some(VKey::U),
            "v" => Some(VKey::V),
            "w" => Some(VKey::W),
            "x" => Some(VKey::X),
            "y" => Some(VKey::Y),
            "z" => Some(VKey::Z),
            "0" => Some(VKey::D0),
            "1" => Some(VKey::D1),
            "2" => Some(VKey::D2),
            "3" => Some(VKey::D3),
            "4" => Some(VKey::D4),
            "5" => Some(VKey::D5),
            "6" => Some(VKey::D6),
            "7" => Some(VKey::D7),
            "8" => Some(VKey::D8),
            "9" => Some(VKey::D9),
            ";" | "semicolon" => Some(VKey::Semicolon),
            "[" | "leftbracket" => Some(VKey::LeftBracket),
            "]" | "rightbracket" => Some(VKey::RightBracket),
            "\\" | "backslash" => Some(VKey::Backslash),
            "'" | "quote" => Some(VKey::Quote),
            "/" | "slash" => Some(VKey::Slash),
            "-" | "minus" => Some(VKey::Minus),
            "=" | "equals" => Some(VKey::Equals),
            "," | "comma" => Some(VKey::Comma),
            "." | "period" => Some(VKey::Period),
            "`" | "backtick" => Some(VKey::Backtick),
            "space" => Some(VKey::Space),
            "f1" => Some(VKey::F1),
            "f2" => Some(VKey::F2),
            "f3" => Some(VKey::F3),
            "f4" => Some(VKey::F4),
            "f5" => Some(VKey::F5),
            "f6" => Some(VKey::F6),
            "f7" => Some(VKey::F7),
            "f8" => Some(VKey::F8),
            "f9" => Some(VKey::F9),
            "f10" => Some(VKey::F10),
            "f11" => Some(VKey::F11),
            "f12" => Some(VKey::F12),
            _ => None,
        }
    }
}

/// Parse a key combo string like "Ctrl+B" or "p" into a Vec<VKey>.
pub fn parse_key_combo(s: &str) -> Option<Vec<VKey>> {
    s.split('+')
        .map(|part| VKey::from_name(part.trim()))
        .collect()
}

/// An action the mapper can produce.
#[derive(Debug, Clone)]
pub enum Action {
    /// Press and release a key combo (modifiers held, main key pressed+released, modifiers released).
    KeyCombo(Vec<VKey>),
    /// Hold keys down (sent on button press, released on button release).
    KeyDown(Vec<VKey>),
    /// Release held keys.
    KeyUp(Vec<VKey>),
    /// Sequence of key combos with a delay between each (for tmux prefix+key).
    KeySequence(Vec<Vec<VKey>>),
    /// Mouse scroll event. Values in wheel-delta units (positive = up/right).
    Scroll { horizontal: i32, vertical: i32 },
    /// Relative mouse cursor movement (screen pixels). Emitted by touchpad touch.
    MouseMove { dx: i32, dy: i32 },
    /// Left mouse button click (press + release). Emitted by touchpad physical click.
    MouseClick,
    /// Emit Unicode text and optionally submit Enter.
    LauncherText { text: String, enter: bool },
}

impl Action {
    /// Releases must never compete with droppable motion/repeat traffic.
    pub fn is_safety_release(&self) -> bool {
        matches!(self, Self::KeyUp(_))
    }
}

/// Key repeat timing.
const REPEAT_DELAY_MS: u64 = 300; // hold before repeating
const REPEAT_RATE_MS: u64 = 100; // interval between repeats

/// Scroll timing.
const SCROLL_MIN_INTERVAL_MS: u64 = 30; // fastest scroll at full deflection
const SCROLL_MAX_INTERVAL_MS: u64 = 200; // slowest scroll near dead zone edge
const WHEEL_DELTA: i32 = 120; // Windows standard per notch

/// Per-button repeat tracking with two-frame confirmation.
/// First frame of a new press is "pending" — only fires if still held next frame.
/// Filters single-frame hat switch glitches (~8ms latency, unnoticeable).
#[derive(Clone, Default)]
struct RepeatTimer {
    pending_since: Option<Instant>,
    pressed_at: Option<Instant>,
    last_fired: Option<Instant>,
}

impl RepeatTimer {
    fn on_press(&mut self, now: Instant) {
        self.pending_since = Some(now);
    }

    fn on_hold(&mut self, now: Instant) -> bool {
        if let Some(pending) = self.pending_since.take() {
            self.pressed_at = Some(pending);
            self.last_fired = Some(now);
            return true;
        }
        let pressed_at = match self.pressed_at {
            Some(t) => t,
            None => return false,
        };
        let held_ms = now.duration_since(pressed_at).as_millis() as u64;
        if held_ms < REPEAT_DELAY_MS {
            return false;
        }
        let last = self.last_fired.unwrap_or(pressed_at);
        if now.duration_since(last).as_millis() as u64 >= REPEAT_RATE_MS {
            self.last_fired = Some(now);
            return true;
        }
        false
    }

    fn on_release(&mut self) {
        self.pressed_at = None;
        self.pending_since = None;
    }
}

/// Resolved configurable button mappings (computed once at startup).
/// None = unmapped — the button does nothing.
#[derive(Clone)]
struct ButtonMap {
    l1: Option<Action>,
    r1: Option<Action>,
    r2: Option<Action>,
    square: Option<Action>,
    share: Option<Action>,
    options: Option<Action>,
    touchpad: Option<Action>,
    cross: Option<Action>,
    circle: Option<Action>,
    triangle: Option<Action>,
    l3: Option<Action>,
    r3: Option<Action>,
}

impl Default for ButtonMap {
    fn default() -> Self {
        let launchers = crate::config::Config::default().launchers;
        Self::resolve(
            &ButtonsConfig::default(),
            &TmuxConfig::default(),
            &Detected::default(),
            &launchers,
        )
    }
}

/// Well-known tmux action → default key mapping (tmux defaults).
/// Used as fallback when auto-detection is unavailable.
fn default_key_for_action(action: &str) -> Option<Vec<VKey>> {
    match action {
        "previous-window" => Some(vec![VKey::P]),
        "next-window" => Some(vec![VKey::N]),
        // `new-window -c ~` variants share the default `c` bind; for a true
        // home cwd the user's tmux bind (or detected full command) should be
        // `new-window -c ~` / `$HOME`. Key fallback still opens a new window.
        "new-window"
        | "new-window -c ~"
        | "new-window -c ~/"
        | "new-window -c $HOME"
        | "new-window -c \"$HOME\"" => Some(vec![VKey::C]),
        "kill-window" => Some(vec![VKey::Shift, VKey::D7]), // &
        "copy-mode" => Some(vec![VKey::LeftBracket]),
        "resize-pane -Z" => Some(vec![VKey::Z]), // zoom toggle
        "last-pane" => Some(vec![VKey::Semicolon]),
        "select-pane" => Some(vec![VKey::O]), // next pane
        "last-window" => Some(vec![VKey::L]),
        "detach-client" => Some(vec![VKey::D]),
        "split-window -h" => Some(vec![VKey::Shift, VKey::D5]), // %
        "split-window -v" => Some(vec![VKey::Shift, VKey::Quote]), // "
        // Common partials used in configs (path arg may be omitted or custom).
        "split-window -h -c" => Some(vec![VKey::Shift, VKey::D5]),
        "split-window -v -c" => Some(vec![VKey::Shift, VKey::Quote]),
        _ => None,
    }
}

impl ButtonMap {
    /// Resolve every configurable button from its config string.
    ///
    /// Resolution order per button:
    /// 1. Empty string → None (unmapped)
    /// 2. Tmux action name (detected bindings, then hardcoded tmux defaults) → prefix + key
    /// 3. Claude Code action name (detected from ~/.claude/keybindings.json) → key sequence
    /// 4. Launcher action string ("launcher:<name>") → configured Unicode text
    /// 5. Direct key combo string (e.g. "ctrl+g") → single combo
    fn resolve(
        buttons: &ButtonsConfig,
        tmux: &TmuxConfig,
        detected: &Detected,
        launchers: &std::collections::HashMap<String, LauncherAction>,
    ) -> Self {
        // Tmux prefix: prefer detected, fall back to config, then hardcoded default
        let tmux_detected = if tmux.auto_detect {
            detected.tmux.as_ref()
        } else {
            None
        };
        let prefix = tmux_detected
            .and_then(|d| d.prefix.clone())
            .unwrap_or_else(|| {
                parse_key_combo(&tmux.prefix).unwrap_or_else(|| vec![VKey::Control, VKey::B])
            });
        log::info!("Tmux prefix resolved to: {prefix:?}");

        let resolve = |value: &str| -> Option<Action> {
            if value.is_empty() {
                return None;
            }
            // Tmux action → prefix + key sequence.
            // Prefer exact command match, then base command (first token), then
            // hardcoded tmux-default keys for well-known action strings.
            if let Some(keys) = tmux_detected
                .and_then(|d| d.key_for_action(value).cloned())
                .or_else(|| {
                    let base = value.split_whitespace().next().unwrap_or(value);
                    if base != value {
                        tmux_detected.and_then(|d| d.key_for_action(base).cloned())
                    } else {
                        None
                    }
                })
                .or_else(|| default_key_for_action(value))
                .or_else(|| {
                    let base = value.split_whitespace().next().unwrap_or(value);
                    if base != value {
                        default_key_for_action(base)
                    } else {
                        None
                    }
                })
            {
                log::debug!("Resolved '{value}' as tmux action");
                return Some(Action::KeySequence(vec![prefix.clone(), keys]));
            }
            // Claude Code action → detected key sequence.
            // Single-chord bindings become KeyCombo (no unnecessary inter-key delay);
            // multi-chord sequences (e.g. ctrl+x ctrl+k) stay as KeySequence.
            if let Some(seq) = detected.claude_binding(value) {
                log::debug!("Resolved '{value}' as Claude Code action");
                return if seq.len() == 1 {
                    Some(Action::KeyCombo(seq[0].clone()))
                } else {
                    Some(Action::KeySequence(seq.clone()))
                };
            }
            // Launcher action → configured Unicode text
            // Resolution: user config first (can override built-ins), then built-in catalog.
            if let Some(name) = value.strip_prefix("launcher:") {
                let resolved = launchers
                    .get(name)
                    .cloned()
                    .or_else(|| crate::launcher::builtin_action(name));
                if let Some(action) = resolved {
                    log::debug!("Resolved 'launcher:{name}' as launcher action");
                    if action.text.is_empty() {
                        return None;
                    }
                    return Some(Action::LauncherText {
                        text: action.text,
                        enter: action.enter,
                    });
                } else {
                    log::warn!("launcher:{name} — no such action (built-in or config)");
                    return None;
                }
            }
            // Direct key combo
            parse_key_combo(value).map(Action::KeyCombo)
        };

        Self {
            l1: resolve(&buttons.l1),
            r1: resolve(&buttons.r1),
            r2: resolve(&buttons.r2),
            square: resolve(&buttons.square),
            share: resolve(&buttons.share),
            options: resolve(&buttons.options),
            touchpad: resolve(&buttons.touchpad),
            cross: resolve(&buttons.cross),
            circle: resolve(&buttons.circle),
            triangle: resolve(&buttons.triangle),
            l3: resolve(&buttons.l3),
            r3: resolve(&buttons.r3),
        }
    }
}

/// Main mapper state.
pub struct MapperState {
    prev: ButtonState,
    // D-pad repeat timers
    repeat_up: RepeatTimer,
    repeat_down: RepeatTimer,
    repeat_left: RepeatTimer,
    repeat_right: RepeatTimer,
    // Scroll state
    last_scroll_at: Option<Instant>,
    scroll_dead_zone: i16,
    scroll_sensitivity: f32,
    scroll_horizontal: bool,
    // Left stick as mouse cursor state
    stick_mouse_enabled: bool,
    stick_mouse_sensitivity: f32,
    stick_mouse_dead_zone: i16,
    stick_acc_x: f32,
    stick_acc_y: f32,
    // Mouse mode toggle: shared with tray thread.
    // false = touchpad touch moves cursor; true = left stick moves cursor.
    // Touchpad click (press) fires regardless of mode.
    mouse_stick_active: Arc<AtomicBool>,
    // Touchpad-as-mouse state
    prev_touch: Option<(u16, u16)>,
    touchpad_enabled: bool,
    touchpad_sensitivity: f32,
    /// Four resolved button maps (index = active profile).
    profiles: [ButtonMap; PROFILE_COUNT],
    active_profile: Profile,
}

impl Default for MapperState {
    fn default() -> Self {
        let map = ButtonMap::default();
        Self {
            prev: ButtonState::default(),
            repeat_up: RepeatTimer::default(),
            repeat_down: RepeatTimer::default(),
            repeat_left: RepeatTimer::default(),
            repeat_right: RepeatTimer::default(),
            last_scroll_at: None,
            scroll_dead_zone: 20,
            scroll_sensitivity: 1.0,
            scroll_horizontal: true,
            stick_mouse_enabled: true,
            stick_mouse_sensitivity: 8.0,
            stick_mouse_dead_zone: 15,
            stick_acc_x: 0.0,
            stick_acc_y: 0.0,
            mouse_stick_active: Arc::new(AtomicBool::new(false)),
            prev_touch: None,
            touchpad_enabled: true,
            touchpad_sensitivity: 1.5,
            profiles: [map.clone(), map.clone(), map.clone(), map],
            active_profile: Profile::P1,
        }
    }
}

impl MapperState {
    /// Create a mapper with config-driven settings.
    /// Detected keybinds are used to resolve action-name → key bindings.
    pub fn new(
        cfg: &crate::config::Config,
        detected: &Detected,
        mouse_stick_active: Arc<AtomicBool>,
    ) -> Self {
        let resolve = |b: &ButtonsConfig| {
            ButtonMap::resolve(b, &cfg.tmux, detected, &cfg.launchers)
        };
        let p0 = resolve(&cfg.buttons);
        let p1 = resolve(cfg.profile_1.as_ref().unwrap_or(&ButtonsConfig::default()));
        let p2 = resolve(cfg.profile_2.as_ref().unwrap_or(&ButtonsConfig::default()));
        let p3 = resolve(cfg.profile_3.as_ref().unwrap_or(&ButtonsConfig::default()));
        log::info!(
            "Profiles ready: P1–P4 (PS cycles); LEDs {:02X}/{:02X}/{:02X}/{:02X}",
            PROFILE_PLAYER_LEDS[0],
            PROFILE_PLAYER_LEDS[1],
            PROFILE_PLAYER_LEDS[2],
            PROFILE_PLAYER_LEDS[3],
        );
        Self {
            scroll_dead_zone: cfg.scroll.dead_zone as i16,
            scroll_sensitivity: cfg.scroll.sensitivity,
            scroll_horizontal: cfg.scroll.horizontal,
            stick_mouse_enabled: cfg.stick_mouse.enabled,
            stick_mouse_sensitivity: cfg.stick_mouse.sensitivity,
            stick_mouse_dead_zone: cfg.stick_mouse.dead_zone as i16,
            mouse_stick_active,
            touchpad_enabled: cfg.touchpad.enabled,
            touchpad_sensitivity: cfg.touchpad.sensitivity,
            profiles: [p0, p1, p2, p3],
            active_profile: Profile::P1,
            ..Default::default()
        }
    }

    /// Currently active profile (P1–P4).
    pub fn profile(&self) -> Profile {
        self.active_profile
    }

    /// Player-indicator LED bitmask for the active profile.
    pub fn profile_led_mask(&self) -> u8 {
        self.active_profile.led_mask()
    }

    fn active_map(&self) -> &ButtonMap {
        &self.profiles[self.active_profile.index()]
    }

    /// Test helper: clear touchpad mapping on every profile (restore click fallback).
    #[cfg(test)]
    fn clear_touchpad_maps(&mut self) {
        for p in &mut self.profiles {
            p.touchpad = None;
        }
    }

    /// Test helper: whether the active profile maps the touchpad press.
    #[cfg(test)]
    fn active_touchpad_mapped(&self) -> bool {
        self.active_map().touchpad.is_some()
    }

    /// Given current input, return actions for newly pressed buttons and analog input.
    pub fn update(&mut self, input: &UnifiedInput) -> Vec<Action> {
        let current = &input.buttons;
        let mut actions = Vec::new();
        let now = Instant::now();

        // --- Touchpad: touch → cursor movement, click → left mouse button (always active) ---
        self.process_touchpad(input, &mut actions);

        // --- Left stick → mouse cursor (always active) ---
        self.process_stick_mouse(input, &mut actions);

        // --- PS: cycle profiles (elegant original design, 4 slots) ---
        if current.ps && !self.prev.ps {
            self.active_profile = self.active_profile.next();
            log::info!(
                "Profile → {} (player LEDs 0x{:02X})",
                self.active_profile,
                self.profile_led_mask()
            );
        }

        // --- Fixed key mappings ---
        // L2: hold Ctrl+Win while button is held.
        // Nested KeyCombo/KeySequence that also include Ctrl (e.g. default
        // L1 → prefix Ctrl+B) must not release the L2-held modifiers — that
        // is handled by [`RefcountInjector`] around the OS injector.
        if current.l2 && !self.prev.l2 {
            actions.push(Action::KeyDown(vec![VKey::Control, VKey::Win]));
        } else if !current.l2 && self.prev.l2 {
            actions.push(Action::KeyUp(vec![VKey::Control, VKey::Win]));
        }

        // --- Active-profile configurable button mappings ---
        let map = self.active_map().clone();
        macro_rules! on_press_mapped {
            ($field:ident) => {
                if current.$field && !self.prev.$field {
                    if let Some(action) = map.$field.as_ref() {
                        actions.push(action.clone());
                    }
                }
            };
        }

        on_press_mapped!(l1);
        on_press_mapped!(r1);
        on_press_mapped!(r2);
        on_press_mapped!(square);
        on_press_mapped!(share);
        on_press_mapped!(options);
        on_press_mapped!(cross);
        on_press_mapped!(circle);
        on_press_mapped!(triangle);
        on_press_mapped!(l3);
        on_press_mapped!(r3);
        // Touchpad press: profile map wins when set; otherwise mouse click
        // while touchpad cursor mode is enabled.
        if current.touchpad && !self.prev.touchpad {
            if let Some(action) = map.touchpad.as_ref() {
                actions.push(action.clone());
            } else if self.touchpad_enabled {
                actions.push(Action::MouseClick);
            }
        }

        // --- D-pad with two-frame confirm + repeat ---
        let up_held = matches!(current.dpad, DPad::Up | DPad::UpLeft | DPad::UpRight);
        let down_held = matches!(current.dpad, DPad::Down | DPad::DownLeft | DPad::DownRight);
        let left_held = matches!(current.dpad, DPad::Left | DPad::UpLeft | DPad::DownLeft);
        let right_held = matches!(current.dpad, DPad::Right | DPad::UpRight | DPad::DownRight);

        let prev_up = matches!(self.prev.dpad, DPad::Up | DPad::UpLeft | DPad::UpRight);
        let prev_down = matches!(
            self.prev.dpad,
            DPad::Down | DPad::DownLeft | DPad::DownRight
        );
        let prev_left = matches!(self.prev.dpad, DPad::Left | DPad::UpLeft | DPad::DownLeft);
        let prev_right = matches!(
            self.prev.dpad,
            DPad::Right | DPad::UpRight | DPad::DownRight
        );

        macro_rules! dpad {
            ($held:expr, $prev:expr, $timer:expr, $key:expr) => {
                if $held && !$prev {
                    $timer.on_press(now);
                } else if $held {
                    if $timer.on_hold(now) {
                        actions.push(Action::KeyCombo(vec![$key]));
                    }
                } else {
                    $timer.on_release();
                }
            };
        }

        dpad!(up_held, prev_up, self.repeat_up, VKey::Up);
        dpad!(down_held, prev_down, self.repeat_down, VKey::Down);
        dpad!(left_held, prev_left, self.repeat_left, VKey::Left);
        dpad!(right_held, prev_right, self.repeat_right, VKey::Right);

        // --- Right stick → scroll ---
        self.process_scroll(input.right_stick, now, &mut actions);

        self.prev = *current;
        actions
    }

    /// Synthesize safety releases for held fixed mappings (L2 → Ctrl+Win).
    ///
    /// Call when the controller link drops mid-hold so modifiers do not stick.
    /// Emits [`Action::KeyUp`] which is a safety release (queue-bypass eligible).
    pub fn force_release_holds(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.prev.l2 {
            actions.push(Action::KeyUp(vec![VKey::Control, VKey::Win]));
            self.prev.l2 = false;
        }
        actions
    }

    /// Process right stick into scroll actions with dead zone and rate limiting.
    fn process_scroll(&mut self, stick: (u8, u8), now: Instant, actions: &mut Vec<Action>) {
        let (rx, ry) = stick;
        let dx = rx as i16 - 128;
        let dy = ry as i16 - 128;

        // Apply dead zone
        let dx = if dx.abs() < self.scroll_dead_zone {
            0
        } else {
            dx
        };
        let dy = if dy.abs() < self.scroll_dead_zone {
            0
        } else {
            dy
        };

        // Ignore horizontal if disabled
        let dx = if self.scroll_horizontal { dx } else { 0 };

        if dx == 0 && dy == 0 {
            self.last_scroll_at = None;
            return;
        }

        // Deflection magnitude (0.0 to 1.0)
        let max_deflection = (dx.abs().max(dy.abs()) as f32 / 127.0).min(1.0);

        // Rate limiting: more deflection → shorter interval → faster scrolling
        let interval_ms = SCROLL_MAX_INTERVAL_MS
            - ((SCROLL_MAX_INTERVAL_MS - SCROLL_MIN_INTERVAL_MS) as f32 * max_deflection) as u64;

        if let Some(last) = self.last_scroll_at
            && now.duration_since(last).as_millis() < interval_ms as u128
        {
            return;
        }

        // Y: stick up (dy < 0) → scroll up (positive vertical wheel delta)
        let vertical = if dy != 0 {
            let norm = (dy as f32 / -127.0).clamp(-1.0, 1.0);
            (norm * self.scroll_sensitivity * WHEEL_DELTA as f32) as i32
        } else {
            0
        };

        // X: stick right (dx > 0) → scroll right (positive horizontal)
        let horizontal = if dx != 0 {
            let norm = (dx as f32 / 127.0).clamp(-1.0, 1.0);
            (norm * self.scroll_sensitivity * WHEEL_DELTA as f32) as i32
        } else {
            0
        };

        if vertical != 0 || horizontal != 0 {
            actions.push(Action::Scroll {
                horizontal,
                vertical,
            });
            self.last_scroll_at = Some(now);
        }
    }

    /// Translate touchpad *swipe* coordinates into relative mouse movement.
    ///
    /// The physical touchpad **press** is handled in [`MapperState::update`]:
    /// a non-empty `[buttons].touchpad` mapping takes priority; otherwise a
    /// left-click is emitted when touchpad cursor mode is enabled.
    fn process_touchpad(&mut self, input: &UnifiedInput, actions: &mut Vec<Action>) {
        if !self.touchpad_enabled {
            return; // config-level disable: suppresses swipe movement
        }

        // ── Touch movement: only in touchpad mode (not when left stick drives cursor) ──
        let stick_active = self.mouse_stick_active.load(Ordering::Relaxed);
        let tp = &input.touchpad[0];
        if tp.active && !stick_active {
            if let Some((px, py)) = self.prev_touch {
                let raw_dx = tp.x as i32 - px as i32;
                let raw_dy = tp.y as i32 - py as i32;
                let dx = (raw_dx as f32 * self.touchpad_sensitivity) as i32;
                let dy = (raw_dy as f32 * self.touchpad_sensitivity) as i32;
                if dx != 0 || dy != 0 {
                    log::debug!("TouchpadMove raw=({raw_dx},{raw_dy}) scaled=({dx},{dy})");
                    actions.push(Action::MouseMove { dx, dy });
                }
            }
            self.prev_touch = Some((tp.x, tp.y));
        } else {
            // Clear prev_touch so switching back to touchpad mode doesn't
            // produce a spurious large jump.
            self.prev_touch = None;
        }
    }

    /// Translate left analog stick deflection into relative mouse movement.
    ///
    /// Velocity-based: stick position → cursor speed per frame.
    /// A sub-pixel accumulator (`stick_acc_x/y`) carries fractional pixels
    /// across frames so slow, precise movements don't stutter.
    fn process_stick_mouse(&mut self, input: &UnifiedInput, actions: &mut Vec<Action>) {
        if !self.stick_mouse_enabled || !self.mouse_stick_active.load(Ordering::Relaxed) {
            return;
        }

        let (lx, ly) = input.left_stick;
        let dx_raw = lx as i16 - 128;
        let dy_raw = ly as i16 - 128;

        // Apply dead zone per axis
        let dx_raw = if dx_raw.abs() < self.stick_mouse_dead_zone {
            0
        } else {
            dx_raw
        };
        let dy_raw = if dy_raw.abs() < self.stick_mouse_dead_zone {
            0
        } else {
            dy_raw
        };

        if dx_raw == 0 && dy_raw == 0 {
            // Reset accumulators when stick returns to center so no phantom move
            // fires when the stick is next pushed.
            self.stick_acc_x = 0.0;
            self.stick_acc_y = 0.0;
            return;
        }

        // Normalize to -1.0..1.0 and scale by sensitivity (pixels/frame at full deflection)
        let vx = (dx_raw as f32 / 127.0).clamp(-1.0, 1.0) * self.stick_mouse_sensitivity;
        let vy = (dy_raw as f32 / 127.0).clamp(-1.0, 1.0) * self.stick_mouse_sensitivity;

        // Accumulate; extract whole pixels; keep remainder for next frame
        self.stick_acc_x += vx;
        self.stick_acc_y += vy;

        let dx = self.stick_acc_x as i32;
        let dy = self.stick_acc_y as i32;

        if dx != 0 || dy != 0 {
            self.stick_acc_x -= dx as f32;
            self.stick_acc_y -= dy as f32;
            log::debug!(
                "StickMouse move=({dx},{dy}) acc=({:.2},{:.2})",
                self.stick_acc_x,
                self.stick_acc_y
            );
            actions.push(Action::MouseMove { dx, dy });
        }
    }
}

// ── Windows injector: platform::Injector over the original SendInput logic ──

/// Submit one atomic SendInput batch (shared by every WinInjector method).
#[cfg(windows)]
fn send_inputs(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Lower a portable [`Key`] to a Windows VK code plus whether Shift must be
/// held for it (e.g. a shifted character like '&' → VK_7 + Shift).
#[cfg(windows)]
fn lower_vk(key: Key) -> (u16, bool) {
    let (vk, shift) = key.to_win_vk();
    (vk, shift.is_some())
}

/// Windows [`Injector`] wrapping the exact SendInput behavior this mapper has
/// always had. Constructed directly by [`injector`] on Windows (no uinput
/// involved); also re-exportable by `platform::win_impl` if desired.
#[cfg(windows)]
pub struct WinInjector;

#[cfg(windows)]
impl Injector for WinInjector {
    /// Press every key in order, release in reverse — for `[modifiers…, main]`
    /// this is exactly the legacy `send_key_combo`: modifiers held, main key
    /// pressed+released, modifiers released. All events go out in one atomic
    /// SendInput batch, as before.
    fn combo(&mut self, keys: &[Key]) {
        if keys.is_empty() {
            return;
        }
        let mut inputs: Vec<INPUT> = Vec::with_capacity(keys.len() * 2 + 2);
        for &k in keys {
            let (vk, shifted) = lower_vk(k);
            if shifted {
                inputs.push(make_key_input(VK_SHIFT, 0));
            }
            inputs.push(make_key_input(vk, 0));
        }
        for &k in keys.iter().rev() {
            let (vk, shifted) = lower_vk(k);
            inputs.push(make_key_input(vk, KEYEVENTF_KEYUP));
            if shifted {
                inputs.push(make_key_input(VK_SHIFT, KEYEVENTF_KEYUP));
            }
        }
        send_inputs(&inputs);
    }

    /// Press a key down (hold). Pair with [`Injector::key_up`] to release.
    fn key_down(&mut self, k: Key) {
        let (vk, shifted) = lower_vk(k);
        let mut inputs: Vec<INPUT> = Vec::with_capacity(2);
        if shifted {
            inputs.push(make_key_input(VK_SHIFT, 0));
        }
        inputs.push(make_key_input(vk, 0));
        send_inputs(&inputs);
    }

    fn key_up(&mut self, k: Key) {
        let (vk, shifted) = lower_vk(k);
        let mut inputs: Vec<INPUT> = Vec::with_capacity(2);
        inputs.push(make_key_input(vk, KEYEVENTF_KEYUP));
        if shifted {
            inputs.push(make_key_input(VK_SHIFT, KEYEVENTF_KEYUP));
        }
        send_inputs(&inputs);
    }

    /// Move the mouse cursor by a relative offset.
    fn mouse_rel(&mut self, dx: i32, dy: i32) {
        let input = make_mouse_move_input(dx, dy);
        send_inputs(&[input]);
    }

    /// Scroll. `vertical`: positive = up; `horizontal`: positive = right
    /// (mapper's wheel-delta convention, unchanged).
    fn wheel(&mut self, vertical: i32, horizontal: i32) {
        let mut inputs: Vec<INPUT> = Vec::new();
        if vertical != 0 {
            inputs.push(make_mouse_input(MOUSEEVENTF_WHEEL, vertical));
        }
        if horizontal != 0 {
            inputs.push(make_mouse_input(MOUSEEVENTF_HWHEEL, horizontal));
        }
        send_inputs(&inputs);
    }

    /// Left mouse button click (down + up).
    fn click(&mut self) {
        let inputs = [
            make_mouse_flag_input(MOUSEEVENTF_LEFTDOWN),
            make_mouse_flag_input(MOUSEEVENTF_LEFTUP),
        ];
        send_inputs(&inputs);
    }
}

/// Inject Unicode text into the active window, then optionally press Enter.
///
/// Matches claude-launcher's proven approach:
///   1. Build down+up KEYEVENTF_UNICODE pairs for all UTF-16 code units.
///   2. Send all characters in a single `SendInput` call (atomic to Windows,
///      zero per-character delay).
///   3. If `submit_enter`, sleep `ENTER_DELAY_MS` so the text lands before Enter.
#[cfg(windows)]
pub fn send_launcher_text(text: &str, submit_enter: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;

    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 4);
    for cu in text.encode_utf16() {
        inputs.push(make_unicode_input(cu, KEYEVENTF_UNICODE));
        inputs.push(make_unicode_input(cu, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }

    if !inputs.is_empty() {
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }

    if submit_enter {
        // 16 ms settle, then staged Return: down → 10 ms → up (same as Linux).
        std::thread::sleep(std::time::Duration::from_millis(ENTER_DELAY_MS));
        unsafe {
            SendInput(
                1,
                &make_key_input(VK_RETURN, 0),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(COMBO_MAIN_RELEASE_GAP_MS));
        unsafe {
            SendInput(
                1,
                &make_key_input(VK_RETURN, KEYEVENTF_KEYUP),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }
}

#[cfg(windows)]
fn make_key_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn make_unicode_input(ch: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: ch,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn make_mouse_input(flags: u32, wheel_delta: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: wheel_delta as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Build a relative mouse-move INPUT struct.
#[cfg(windows)]
fn make_mouse_move_input(dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Build a mouse button INPUT struct (no dx/dy, no wheel data).
#[cfg(windows)]
fn make_mouse_flag_input(flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

// ── Injector plumbing (shared) ───────────────────────────────────────

/// Process-wide injector, created lazily on first use.
///
/// Windows: [`WinInjector`] over the original SendInput code — always
/// available, behavior-identical to the pre-port mapper.
/// Linux: C1's evdev/uinput injector via [`crate::platform::new_injector`];
/// if /dev/uinput is unavailable the daemon keeps running with a logged
/// no-op injector (feature-degraded, never fatal — SPEC §4).
///
/// Both platforms wrap the OS injector in [`RefcountInjector`] so L2 hold
/// (Ctrl+Super) survives nested `combo` releases that also press Ctrl.
static INJECTOR: OnceLock<Mutex<Box<dyn Injector>>> = OnceLock::new();

fn injector() -> &'static Mutex<Box<dyn Injector>> {
    INJECTOR.get_or_init(|| Mutex::new(create_injector()))
}

/// Eagerly create the process-wide injector at startup so permission errors
/// (and their remediation logs) surface immediately instead of on first key.
/// Never fails: on error the injector is a logged no-op (SPEC §4).
pub fn init_injector() {
    let _ = injector();
}

/// Reference-counted key holds around an inner [`Injector`].
///
/// Without this, `combo(&[Ctrl, B])` while L2 holds Ctrl would emit Ctrl↑
/// and leave the OS with only Super held — breaking L2-as-PTT/mod chord.
struct RefcountInjector {
    inner: Box<dyn Injector>,
    counts: HashMap<Key, u32>,
}

impl RefcountInjector {
    fn wrap(inner: Box<dyn Injector>) -> Box<dyn Injector> {
        Box::new(Self {
            inner,
            counts: HashMap::new(),
        })
    }
}

impl Injector for RefcountInjector {
    fn combo(&mut self, keys: &[Key]) {
        if keys.is_empty() {
            return;
        }
        for &k in keys {
            self.key_down(k);
        }
        for &k in keys.iter().rev() {
            self.key_up(k);
        }
    }

    fn key_down(&mut self, k: Key) {
        let n = self.counts.entry(k).or_insert(0);
        if *n == 0 {
            self.inner.key_down(k);
        }
        *n = n.saturating_add(1);
    }

    fn key_up(&mut self, k: Key) {
        let Some(n) = self.counts.get_mut(&k) else {
            // Already released (or never held) — do not emit a spurious OS up.
            return;
        };
        *n = n.saturating_sub(1);
        if *n == 0 {
            self.counts.remove(&k);
            self.inner.key_up(k);
        }
    }

    fn mouse_rel(&mut self, dx: i32, dy: i32) {
        self.inner.mouse_rel(dx, dy);
    }

    fn wheel(&mut self, vertical: i32, horizontal: i32) {
        self.inner.wheel(vertical, horizontal);
    }

    fn click(&mut self) {
        self.inner.click();
    }
}

#[cfg(windows)]
fn create_injector() -> Box<dyn Injector> {
    RefcountInjector::wrap(Box::new(WinInjector))
}

#[cfg(not(windows))]
fn create_injector() -> Box<dyn Injector> {
    let raw: Box<dyn Injector> = match crate::platform::new_injector() {
        Ok(inj) => inj,
        Err(e) => {
            log::error!("input injection unavailable: {e}");
            #[cfg(target_os = "linux")]
            log::error!(
                "remediation: modprobe uinput; install packaging/linux/99-ds4cc.rules; \
                 add user to uinput group; re-login"
            );
            Box::new(NullInjector)
        }
    };
    RefcountInjector::wrap(raw)
}

/// No-op injector used when the platform injector cannot be created.
/// Every call is a logged no-op so the daemon keeps running (and keeps
/// reading the controller) even without injection permissions.
#[cfg(not(windows))]
struct NullInjector;

#[cfg(not(windows))]
impl Injector for NullInjector {
    fn combo(&mut self, keys: &[Key]) {
        log::debug!("injector unavailable; dropping {}-key combo", keys.len());
    }
    fn key_down(&mut self, _k: Key) {
        log::debug!("injector unavailable; dropping key_down");
    }
    fn key_up(&mut self, _k: Key) {
        log::debug!("injector unavailable; dropping key_up");
    }
    fn mouse_rel(&mut self, _dx: i32, _dy: i32) {
        log::trace!("injector unavailable; dropping mouse move");
    }
    fn wheel(&mut self, _vertical: i32, _horizontal: i32) {
        log::trace!("injector unavailable; dropping scroll");
    }
    fn click(&mut self) {
        log::debug!("injector unavailable; dropping mouse click");
    }
}

/// Lower a mapper [`VKey`] to the portable [`Key`] used by the platform
/// injector. Pure and total (every VKey variant has a Key counterpart).
/// Letters/digits/punct lower to their unshifted `Char` form — shifted
/// semantics stay carried by an explicit `VKey::Shift` in the combo, exactly
/// as the Windows path has always worked (e.g. "kill-window" = Shift+7 → '&').
pub fn to_key(v: VKey) -> Key {
    match v {
        VKey::Return => Key::Enter,
        VKey::Escape => Key::Escape,
        VKey::Tab => Key::Tab,
        VKey::Up => Key::Up,
        VKey::Down => Key::Down,
        VKey::Left => Key::Left,
        VKey::Right => Key::Right,
        VKey::Alt => Key::Alt,
        VKey::Shift => Key::Shift,
        VKey::Control => Key::Ctrl,
        VKey::Win => Key::Super,
        VKey::Space => Key::Space,
        VKey::A => Key::Char('a'),
        VKey::B => Key::Char('b'),
        VKey::C => Key::Char('c'),
        VKey::D => Key::Char('d'),
        VKey::E => Key::Char('e'),
        VKey::F => Key::Char('f'),
        VKey::G => Key::Char('g'),
        VKey::H => Key::Char('h'),
        VKey::I => Key::Char('i'),
        VKey::J => Key::Char('j'),
        VKey::K => Key::Char('k'),
        VKey::L => Key::Char('l'),
        VKey::M => Key::Char('m'),
        VKey::N => Key::Char('n'),
        VKey::O => Key::Char('o'),
        VKey::P => Key::Char('p'),
        VKey::Q => Key::Char('q'),
        VKey::R => Key::Char('r'),
        VKey::S => Key::Char('s'),
        VKey::T => Key::Char('t'),
        VKey::U => Key::Char('u'),
        VKey::V => Key::Char('v'),
        VKey::W => Key::Char('w'),
        VKey::X => Key::Char('x'),
        VKey::Y => Key::Char('y'),
        VKey::Z => Key::Char('z'),
        VKey::D0 => Key::Char('0'),
        VKey::D1 => Key::Char('1'),
        VKey::D2 => Key::Char('2'),
        VKey::D3 => Key::Char('3'),
        VKey::D4 => Key::Char('4'),
        VKey::D5 => Key::Char('5'),
        VKey::D6 => Key::Char('6'),
        VKey::D7 => Key::Char('7'),
        VKey::D8 => Key::Char('8'),
        VKey::D9 => Key::Char('9'),
        VKey::Semicolon => Key::Char(';'),
        VKey::LeftBracket => Key::Char('['),
        VKey::RightBracket => Key::Char(']'),
        VKey::Backslash => Key::Char('\\'),
        VKey::Quote => Key::Char('\''),
        VKey::Slash => Key::Char('/'),
        VKey::Minus => Key::Char('-'),
        VKey::Equals => Key::Char('='),
        VKey::Comma => Key::Char(','),
        VKey::Period => Key::Char('.'),
        VKey::Backtick => Key::Char('`'),
        VKey::F1 => Key::F(1),
        VKey::F2 => Key::F(2),
        VKey::F3 => Key::F(3),
        VKey::F4 => Key::F(4),
        VKey::F5 => Key::F(5),
        VKey::F6 => Key::F(6),
        VKey::F7 => Key::F(7),
        VKey::F8 => Key::F(8),
        VKey::F9 => Key::F(9),
        VKey::F10 => Key::F(10),
        VKey::F11 => Key::F(11),
        VKey::F12 => Key::F(12),
    }
}

fn to_keys(keys: &[VKey]) -> Vec<Key> {
    keys.iter().map(|&v| to_key(v)).collect()
}

/// One step of a staged multi-key combo inject plan (pure; used by tests + executor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboStep {
    Down(Key),
    Up(Key),
    WaitMs(u64),
}

/// Build the inject plan for a key combo.
///
/// Multi-key chord (e.g. Triangle → `Ctrl+n`):
/// ```text
/// Ctrl↓ + n↓ → COMBO_HOLD_MS (19) → Ctrl↑ + n↑  (same order as press, no gap)
/// ```
/// Single key: `K↓` then immediately `K↑`.
/// Empty: no steps.
pub fn staged_combo_plan(keys: &[Key]) -> Vec<ComboStep> {
    if keys.is_empty() {
        return Vec::new();
    }
    let mut steps = Vec::with_capacity(keys.len() * 2 + 2);
    for &k in keys {
        steps.push(ComboStep::Down(k));
    }
    if keys.len() == 1 {
        steps.push(ComboStep::Up(keys[0]));
        return steps;
    }
    steps.push(ComboStep::WaitMs(COMBO_HOLD_MS));
    // Release in press order with no inter-key gap (Ctrl↑ then n↑ for ctrl+n).
    for &k in keys {
        steps.push(ComboStep::Up(k));
    }
    steps
}

fn execute_staged_combo(keys: &[Key]) {
    for step in staged_combo_plan(keys) {
        match step {
            ComboStep::Down(k) => {
                injector()
                    .lock()
                    .expect("injector poisoned")
                    .key_down(k);
            }
            ComboStep::Up(k) => {
                injector().lock().expect("injector poisoned").key_up(k);
            }
            // Sleep without holding the injector mutex.
            ComboStep::WaitMs(ms) => {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
        }
    }
}

/// Execute an action (send keystrokes, scroll, or mouse movement/click).
///
/// All key/mouse actions go through the shared [`Injector`] trait object.
/// `LauncherText` (arbitrary Unicode) is not representable in the Injector
/// trait surface, so it keeps its platform-specific text path: SendInput
/// KEYEVENTF_UNICODE on Windows, wtype/xdotool on Linux.
pub fn execute_action(action: &Action) {
    match action {
        Action::KeyCombo(keys) => {
            let keys = to_keys(keys);
            execute_staged_combo(&keys);
        }
        Action::KeyDown(keys) => {
            let mut inj = injector().lock().expect("injector poisoned");
            for &v in keys {
                inj.key_down(to_key(v));
            }
        }
        // Release in reverse order for proper modifier release (legacy behavior).
        Action::KeyUp(keys) => {
            let mut inj = injector().lock().expect("injector poisoned");
            for &v in keys.iter().rev() {
                inj.key_up(to_key(v));
            }
        }
        // Chord sequence (e.g. tmux prefix + action key): staged combo, 10 ms, combo.
        Action::KeySequence(combos) => {
            for (i, combo) in combos.iter().enumerate() {
                let keys = to_keys(combo);
                execute_staged_combo(&keys);
                if i < combos.len() - 1 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
        Action::Scroll {
            horizontal,
            vertical,
        } => injector()
            .lock()
            .expect("injector poisoned")
            .wheel(*vertical, *horizontal),
        Action::MouseMove { dx, dy } => injector()
            .lock()
            .expect("injector poisoned")
            .mouse_rel(*dx, *dy),
        Action::MouseClick => injector().lock().expect("injector poisoned").click(),
        Action::LauncherText { text, enter } => send_launcher_text(text, *enter),
    }
}

/// Linux text-injection backend, chosen per session type.
///
/// Wayland-first: `wtype` speaks the Wayland virtual-keyboard protocol and works
/// natively on compositors (Hyprland, Sway, GNOME, KDE) where the legacy X11
/// `xdotool` path is a silent no-op. `xdotool` remains the explicit fallback for
/// real X11 / XWayland-only sessions.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxBackend {
    /// Wayland compositors — `wtype` (virtual keyboard protocol).
    Wayland,
    /// X11 sessions — `xdotool` (XTest).
    X11,
}

#[cfg(target_os = "linux")]
impl LinuxBackend {
    /// Executable resolved via `PATH` for this backend.
    pub fn program(self) -> &'static str {
        match self {
            LinuxBackend::Wayland => "wtype",
            LinuxBackend::X11 => "xdotool",
        }
    }
}

/// Select the injection backend from `XDG_SESSION_TYPE` (passed as `session_type`).
///
/// Only an explicit `wayland` session (case-insensitive) selects `wtype`; every
/// other value — `x11`, `tty`, empty, or unset — falls back to `xdotool`. Passing
/// the value in (rather than reading the env here) keeps selection pure/testable.
#[cfg(target_os = "linux")]
pub fn select_backend(session_type: Option<&str>) -> LinuxBackend {
    match session_type {
        Some(s) if s.eq_ignore_ascii_case("wayland") => LinuxBackend::Wayland,
        _ => LinuxBackend::X11,
    }
}

/// Exact argv (after the program name) to type `text` without submitting.
///
/// Both backends receive `text` as a single discrete argument after a `--`
/// end-of-options separator, so it is typed literally — no shell interpolation
/// and no option injection even when the payload begins with `-`.
#[cfg(target_os = "linux")]
pub fn linux_text_args(backend: LinuxBackend, text: &str) -> Vec<String> {
    match backend {
        LinuxBackend::Wayland => vec!["--".to_string(), text.to_string()],
        LinuxBackend::X11 => vec![
            "type".to_string(),
            "--clearmodifiers".to_string(),
            "--delay".to_string(),
            "0".to_string(),
            "--".to_string(),
            text.to_string(),
        ],
    }
}

/// Exact argv (after the program name) to press and release Enter.
#[cfg(target_os = "linux")]
pub fn linux_enter_args(backend: LinuxBackend) -> Vec<String> {
    match backend {
        // `-k <keysym>` presses and releases a named key (xkb keysym `Return`).
        LinuxBackend::Wayland => vec!["-k".to_string(), "Return".to_string()],
        LinuxBackend::X11 => vec!["key".to_string(), "Return".to_string()],
    }
}

/// Build a pure injection plan for the **text** phase only (wtype/xdotool argv).
///
/// Enter/Return is **not** in this plan: after text + [`ENTER_DELAY_MS`],
/// [`submit_enter_via_injector`] stages `Enter↓ → 10 ms → Enter↑` on uinput.
#[cfg(target_os = "linux")]
pub fn linux_injection_plan(
    backend: LinuxBackend,
    text: &str,
    _submit_enter: bool,
) -> Vec<Vec<String>> {
    vec![linux_text_args(backend, text)]
}

/// Staged Return after launcher text: `Enter↓` → [`COMBO_MAIN_RELEASE_GAP_MS`] → `Enter↑`.
pub fn staged_enter_plan() -> Vec<ComboStep> {
    vec![
        ComboStep::Down(Key::Enter),
        ComboStep::WaitMs(COMBO_MAIN_RELEASE_GAP_MS),
        ComboStep::Up(Key::Enter),
    ]
}

fn submit_enter_via_injector() {
    for step in staged_enter_plan() {
        match step {
            ComboStep::Down(k) => {
                injector()
                    .lock()
                    .expect("injector poisoned")
                    .key_down(k);
            }
            ComboStep::Up(k) => {
                injector().lock().expect("injector poisoned").key_up(k);
            }
            ComboStep::WaitMs(ms) => {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
        }
    }
}

/// Spawn `prog` with `args` as discrete process arguments (never a shell string).
///
/// Returns `true` if the process was spawned **and exited successfully** (exit code 0),
/// `false` if it could not be started (e.g. the backend executable is not installed)
/// or exited with a non-zero status (e.g. compositor rejected injection).
#[cfg(target_os = "linux")]
fn run_injector(prog: &str, args: &[String]) -> bool {
    match std::process::Command::new(prog).args(args).status() {
        Ok(status) => {
            if !status.success() {
                log::warn!(
                    "launcher: '{prog}' exited with {status} — text injection may have failed"
                );
            }
            status.success()
        }
        Err(e) => {
            log::warn!("launcher: could not run '{prog}': {e} (is it installed and on PATH?)");
            false
        }
    }
}

/// Inject **literal** text on Linux, then optionally press Enter.
///
/// Design (omegaG product rule):
/// - **Do not** synthesize characters by faking US (or any) keyboard layouts
///   through uinput. That fights the user's real xkb map and breaks `|`, `\`, `/`.
/// - **Do** send the payload as text: `wtype` on Wayland, `xdotool type` on X11
///   (same tools listed in the Linux installer). Install-time / detect-time
///   binding discovery stays separate — that path reads tmux/Claude binds and
///   maps buttons; this path only dumps a configured string into the focused app.
///
/// Enter (when `submit_enter`): after [`ENTER_DELAY_MS`] (16), stage
/// `Enter↓` → [`COMBO_MAIN_RELEASE_GAP_MS`] (10) → `Enter↑` via the virtual
/// keyboard (Return is layout-stable; text is not).
#[cfg(target_os = "linux")]
pub fn send_launcher_text(text: &str, submit_enter: bool) {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let backend = select_backend(session_type.as_deref());
    let prog = backend.program();

    let plan = linux_injection_plan(backend, text, submit_enter);
    let text_ok = plan
        .first()
        .map(|args| run_injector(prog, args))
        .unwrap_or(false);
    if !text_ok {
        log::error!(
            "launcher: failed to inject text {text:?} via '{prog}'. \
             Install the text injector (Wayland: `wtype`, X11: `xdotool`) — \
             omegaG will not fake characters via uinput keycodes."
        );
        return;
    }

    if submit_enter {
        std::thread::sleep(std::time::Duration::from_millis(ENTER_DELAY_MS));
        submit_enter_via_injector();
    }
}

/// Text-injection stub for platforms with no launcher-text backend.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn send_launcher_text(_text: &str, _submit_enter: bool) {}

// ── Linux injection backend tests ─────────────────────────────────────
//
// These cover the Wayland-first (wtype) / X11-fallback (xdotool) backend:
// selection from XDG_SESSION_TYPE, exact argv construction, Unicode payload,
// optional Enter, graceful missing-executable behavior, and FIFO queue
// preservation across a batch of queued launcher actions.
#[cfg(all(test, target_os = "linux"))]
mod linux_inject_tests {
    use super::{
        COMBO_MAIN_RELEASE_GAP_MS, ComboStep, ENTER_DELAY_MS, LinuxBackend, linux_enter_args,
        linux_injection_plan, linux_text_args, run_injector, select_backend, staged_enter_plan,
    };
    use crate::keys::Key;

    // ── Two-phase submit timing / ordering ────────────────────────────

    #[test]
    fn enter_delay_is_exactly_16ms() {
        // Post-text submit guard: exactly 16 ms after the text batch, before Return.
        assert_eq!(ENTER_DELAY_MS, 16);
    }

    #[test]
    fn injection_plan_types_text_only_enter_is_uinput_staged() {
        // Text phase only in the external-tool plan; Return is staged on uinput.
        let plan = linux_injection_plan(LinuxBackend::Wayland, "| godspeed", true);
        assert_eq!(plan.len(), 1, "text-only external plan; Enter via injector");
        assert_eq!(plan[0], vec!["--".to_string(), "| godspeed".to_string()]);
        assert_eq!(
            staged_enter_plan(),
            vec![
                ComboStep::Down(Key::Enter),
                ComboStep::WaitMs(COMBO_MAIN_RELEASE_GAP_MS),
                ComboStep::Up(Key::Enter),
            ]
        );
    }

    #[test]
    fn injection_plan_omits_enter_when_not_submitting() {
        // No Return phase when enter = false — the text batch is the only phase.
        let plan = linux_injection_plan(LinuxBackend::X11, "hello", false);
        assert_eq!(plan.len(), 1, "no submit = text phase only");
        assert_eq!(plan[0], linux_text_args(LinuxBackend::X11, "hello"));
    }

    #[test]
    fn injection_plan_uses_zero_per_character_delay() {
        // The X11 backend types the whole payload in one xdotool invocation with
        // an explicit `--delay 0` — instantaneous, no per-character pacing.
        let plan = linux_injection_plan(LinuxBackend::X11, "abc", false);
        assert_eq!(
            plan[0],
            vec![
                "type".to_string(),
                "--clearmodifiers".to_string(),
                "--delay".to_string(),
                "0".to_string(),
                "--".to_string(),
                "abc".to_string(),
            ],
        );
        // Wayland types in a single `wtype -- TEXT` with no delay flag at all.
        let wl = linux_injection_plan(LinuxBackend::Wayland, "abc", false);
        assert_eq!(wl[0], vec!["--".to_string(), "abc".to_string()]);
        assert!(
            !wl[0].iter().any(|a| a == "-d" || a == "--delay"),
            "Wayland text phase must carry no per-character delay flag",
        );
    }

    // ── Backend selection ─────────────────────────────────────────────

    #[test]
    fn selects_wayland_only_for_wayland_session() {
        assert_eq!(select_backend(Some("wayland")), LinuxBackend::Wayland);
        // Case-insensitive: XDG_SESSION_TYPE may be reported with varied case.
        assert_eq!(select_backend(Some("Wayland")), LinuxBackend::Wayland);
        assert_eq!(select_backend(Some("WAYLAND")), LinuxBackend::Wayland);
    }

    #[test]
    fn falls_back_to_x11_for_non_wayland_sessions() {
        assert_eq!(select_backend(Some("x11")), LinuxBackend::X11);
        assert_eq!(select_backend(Some("tty")), LinuxBackend::X11);
        assert_eq!(select_backend(Some("")), LinuxBackend::X11);
        // Unset session type (headless / login before session type known) → X11.
        assert_eq!(select_backend(None), LinuxBackend::X11);
    }

    #[test]
    fn backend_program_names_are_stable() {
        assert_eq!(LinuxBackend::Wayland.program(), "wtype");
        assert_eq!(LinuxBackend::X11.program(), "xdotool");
    }

    // ── Exact argument construction ───────────────────────────────────

    #[test]
    fn wtype_text_args_use_end_of_options_separator() {
        // `--` MUST precede the text so payloads starting with '-' are typed
        // literally instead of being parsed as wtype flags.
        assert_eq!(
            linux_text_args(LinuxBackend::Wayland, "hello"),
            vec!["--".to_string(), "hello".to_string()],
        );
    }

    #[test]
    fn wtype_text_args_pass_leading_dash_payload_literally() {
        assert_eq!(
            linux_text_args(LinuxBackend::Wayland, "-rf /"),
            vec!["--".to_string(), "-rf /".to_string()],
        );
    }

    #[test]
    fn wtype_enter_args_press_return_keysym() {
        assert_eq!(
            linux_enter_args(LinuxBackend::Wayland),
            vec!["-k".to_string(), "Return".to_string()],
        );
    }

    #[test]
    fn xdotool_text_args_match_shipped_x11_form() {
        assert_eq!(
            linux_text_args(LinuxBackend::X11, "hi"),
            vec![
                "type".to_string(),
                "--clearmodifiers".to_string(),
                "--delay".to_string(),
                "0".to_string(),
                "--".to_string(),
                "hi".to_string(),
            ],
        );
    }

    #[test]
    fn xdotool_enter_args_match_shipped_x11_form() {
        assert_eq!(
            linux_enter_args(LinuxBackend::X11),
            vec!["key".to_string(), "Return".to_string()],
        );
    }

    // ── Unicode payload ───────────────────────────────────────────────

    #[test]
    fn unicode_payload_preserved_in_args_both_backends() {
        let payload = "🌊 héllo wörld 😀 | godspeed";
        // Payload is passed as a single discrete arg (no shell), byte-for-byte.
        assert_eq!(
            linux_text_args(LinuxBackend::Wayland, payload)
                .last()
                .unwrap(),
            payload,
        );
        assert_eq!(
            linux_text_args(LinuxBackend::X11, payload).last().unwrap(),
            payload,
        );
    }

    // ── Missing executable behavior ───────────────────────────────────

    #[test]
    fn missing_executable_is_graceful_not_panic() {
        // A non-existent program must return false (spawn failed) rather than
        // panicking, so a missing wtype/xdotool never crashes the worker.
        let ok = run_injector(
            "ds4cc-nonexistent-injector-binary-zzz",
            &["--".to_string(), "x".to_string()],
        );
        assert!(!ok, "missing executable must report failure, not spawn");
    }

    // ── Non-zero exit behavior (regression for blocker: Ok(_) => true) ───

    #[test]
    fn run_injector_nonzero_exit_returns_false() {
        // Regression: before the fix, Ok(_) => true swallowed non-zero exit codes.
        // `false` is a standard POSIX program that always exits with status 1.
        // run_injector must now return false so the early-exit guard in
        // send_launcher_text fires and Return is not emitted.
        let result = run_injector("false", &[]);
        assert!(
            !result,
            "`false` exits 1 — run_injector must return false to suppress spurious Return"
        );
    }

    #[test]
    fn text_tool_failure_is_detectable_before_enter() {
        // External text tool failure returns false — send_launcher_text then
        // tries uinput ASCII fallback (not exercised here). Plan is text-only.
        let plan = linux_injection_plan(LinuxBackend::Wayland, "test payload", true);
        assert_eq!(plan.len(), 1);
        assert!(
            !run_injector("false", &plan[0]),
            "failed text tool must report false so caller can skip or fall back"
        );
    }

    // ── Queue preservation ────────────────────────────────────────────

    #[test]
    fn fifo_queue_preserves_order_and_payloads() {
        // Simulate the bounded FIFO worker draining queued LauncherText actions:
        // argv construction is pure, so a batch maps 1:1 in order with no reorder,
        // drop, or payload mutation.
        let queued = [("first", false), ("séçond 🎮", true), ("-third", false)];
        let built: Vec<Vec<String>> = queued
            .iter()
            .map(|(t, _)| linux_text_args(LinuxBackend::Wayland, t))
            .collect();
        assert_eq!(built.len(), queued.len(), "no queued action dropped");
        for (built_args, (text, _)) in built.iter().zip(queued.iter()) {
            assert_eq!(
                built_args.last().unwrap(),
                text,
                "payload preserved in order"
            );
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(f: impl FnOnce(&mut UnifiedInput)) -> UnifiedInput {
        let mut input = UnifiedInput::default();
        f(&mut input);
        input
    }

    #[test]
    fn detects_rising_edge() {
        let mut mapper = MapperState::default();

        let actions = mapper.update(&UnifiedInput::default());
        assert!(actions.is_empty());

        let input = input_with(|i| i.buttons.cross = true);
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::KeyCombo(keys) => assert_eq!(keys, &[VKey::Return]),
            _ => panic!("Expected KeyCombo"),
        }

        // Hold: no new action
        let actions = mapper.update(&input);
        assert!(actions.is_empty());

        // Release
        let actions = mapper.update(&UnifiedInput::default());
        assert!(actions.is_empty());
    }

    #[test]
    fn dpad_two_frame_confirm() {
        let mut mapper = MapperState::default();

        // Frame 1: press Up — pending, no fire
        let input = input_with(|i| i.buttons.dpad = DPad::Up);
        let actions = mapper.update(&input);
        assert!(actions.is_empty(), "Should not fire on first frame");

        // Frame 2: still held — confirmed, fires
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::KeyCombo(keys) => assert_eq!(keys, &[VKey::Up]),
            _ => panic!("Expected KeyCombo"),
        }

        // Frame 3: still held — no repeat yet
        let actions = mapper.update(&input);
        assert!(actions.is_empty());

        // Release
        let actions = mapper.update(&UnifiedInput::default());
        assert!(actions.is_empty());
    }

    #[test]
    fn dpad_single_frame_glitch_filtered() {
        let mut mapper = MapperState::default();

        let input = input_with(|i| i.buttons.dpad = DPad::Up);
        let actions = mapper.update(&input);
        assert!(actions.is_empty(), "Pending, not fired");

        let actions = mapper.update(&UnifiedInput::default());
        assert!(actions.is_empty(), "Single-frame glitch should not fire");
    }

    #[test]
    fn l1_produces_tmux_prev_window() {
        // L1 → tmux prefix + previous-window key
        let mut mapper = MapperState::default();
        let input = input_with(|i| i.buttons.l1 = true);
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::KeySequence(seq) => {
                assert_eq!(seq[0], vec![VKey::Control, VKey::B]);
                assert_eq!(seq[1], vec![VKey::P]);
            }
            _ => panic!("Expected KeySequence"),
        }
    }

    #[test]
    fn scroll_dead_zone_no_action() {
        let mut mapper = MapperState::default();

        // Center stick
        let input = input_with(|i| i.right_stick = (128, 128));
        let actions = mapper.update(&input);
        assert!(!actions.iter().any(|a| matches!(a, Action::Scroll { .. })));

        // Small deflection within dead zone (±20)
        let input = input_with(|i| i.right_stick = (138, 138));
        let actions = mapper.update(&input);
        assert!(!actions.iter().any(|a| matches!(a, Action::Scroll { .. })));
    }

    #[test]
    fn scroll_beyond_dead_zone_fires() {
        let mut mapper = MapperState::default();

        // Stick up (ry=80, deflection=48 > dead_zone=20)
        let input = input_with(|i| i.right_stick = (128, 80));
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Scroll { vertical, .. } if *vertical > 0)),
            "Expected positive vertical scroll for stick-up"
        );
    }

    #[test]
    fn scroll_rate_limited() {
        let mut mapper = MapperState::default();

        let input = input_with(|i| i.right_stick = (128, 0));
        let a1 = mapper.update(&input);
        assert!(a1.iter().any(|a| matches!(a, Action::Scroll { .. })));

        // Immediate second call: rate-limited
        let a2 = mapper.update(&input);
        assert!(!a2.iter().any(|a| matches!(a, Action::Scroll { .. })));
    }

    #[test]
    fn scroll_down_negative_vertical() {
        let mut mapper = MapperState::default();

        // Stick down (ry=200, deflection=72 > dead_zone=20)
        let input = input_with(|i| i.right_stick = (128, 200));
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Scroll { vertical, .. } if *vertical < 0)),
            "Expected negative vertical scroll for stick-down"
        );
    }

    #[test]
    fn ps_cycles_four_profiles_and_leds() {
        let mut mapper = MapperState::default();
        assert_eq!(mapper.profile(), Profile::P1);
        assert_eq!(mapper.profile_led_mask(), PROFILE_PLAYER_LEDS[0]);

        for expected in [1u8, 2, 3, 0] {
            let press = input_with(|i| i.buttons.ps = true);
            let actions = mapper.update(&press);
            assert!(
                actions.is_empty(),
                "profile switch is state-only (LEDs via main), no inject action"
            );
            assert_eq!(mapper.profile().0, expected);
            assert_eq!(
                mapper.profile_led_mask(),
                PROFILE_PLAYER_LEDS[expected as usize]
            );
            // release PS
            let _ = mapper.update(&input_with(|_| {}));
        }
        assert_eq!(
            PROFILE_PLAYER_LEDS,
            [0x04, 0x0A, 0x1B, 0x1F],
            "P1 center, P2 inner two, P3 outer four, P4 all five"
        );
    }

    #[test]
    fn unmapped_config_button_does_nothing() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.l1 = "".into();
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.l1 = true);
        let actions = mapper.update(&input);
        assert!(actions.is_empty(), "Unmapped button should do nothing");
    }

    #[test]
    fn claude_action_resolves_to_direct_combo() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.share = "chat:externalEditor".into();
        let mut detected = Detected::default();
        detected.claude.insert(
            "chat:externalEditor".into(),
            vec![vec![VKey::Control, VKey::G]],
        );
        let mut mapper = MapperState::new(&cfg, &detected, Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.share = true);
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::G])),
            "Claude Code action should resolve to its detected combo"
        );
    }

    #[test]
    fn direct_combo_config_value() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.options = "ctrl+shift+b".into();
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.options = true);
        let actions = mapper.update(&input);
        assert!(
            actions.iter().any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::Shift, VKey::B])),
            "Direct combo string should resolve as-is"
        );
    }

    #[test]
    fn unknown_named_action_is_unmapped() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.share = "launcher:missing".into();
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));
        let input = input_with(|i| i.buttons.share = true);
        let actions = mapper.update(&input);
        assert!(
            actions.is_empty(),
            "Unknown launcher action should be unmapped"
        );
    }

    #[test]
    fn launcher_action_emits_configured_unicode_and_enter() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.share = "launcher:godspeed".into();
        cfg.launchers.insert(
            "godspeed".into(),
            crate::config::LauncherAction {
                text: "godspeed ✨".into(),
                enter: true,
            },
        );
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.share = true);
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::LauncherText { text, enter } => {
                assert_eq!(text, "godspeed ✨");
                assert!(*enter);
            }
            _ => panic!("Expected LauncherText"),
        }
    }

    #[test]
    fn launcher_action_order_preserved() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.cross = "launcher:godspeed".into();
        cfg.buttons.circle = "ctrl+g".into();
        cfg.launchers.insert(
            "godspeed".into(),
            crate::config::LauncherAction {
                text: "x".into(),
                enter: false,
            },
        );
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| {
            i.buttons.cross = true;
            i.buttons.circle = true;
        });
        let actions = mapper.update(&input);
        assert_eq!(actions.len(), 2);
        assert!(matches!(&actions[0], Action::LauncherText { .. }));
        assert!(matches!(&actions[1], Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::G]));
    }

    #[test]
    fn rapid_named_launcher_presses_are_queued_in_order() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.cross = "launcher:godspeed".into();
        cfg.launchers.insert(
            "godspeed".into(),
            crate::config::LauncherAction {
                text: "x".into(),
                enter: false,
            },
        );
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let press_one = input_with(|i| i.buttons.cross = true);
        let release = input_with(|i| i.buttons.cross = false);
        let press_two = input_with(|i| i.buttons.cross = true);

        let actions = mapper.update(&press_one);
        assert_eq!(actions.len(), 1);
        let _ = mapper.update(&release);
        let actions = mapper.update(&press_two);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn tmux_mapped_buttons() {
        type ButtonTest = (fn(&mut UnifiedInput), Vec<VKey>);
        let tests: Vec<ButtonTest> = vec![
            (|i| i.buttons.l1 = true, vec![VKey::P]), // prev window
            (|i| i.buttons.r1 = true, vec![VKey::N]), // next window
            (|i| i.buttons.r2 = true, vec![VKey::Shift, VKey::D7]), // kill window (&)
            (|i| i.buttons.square = true, vec![VKey::C]), // new window
            (|i| i.buttons.touchpad = true, vec![VKey::C]), // new-window -c ~ → default `c`
        ];

        for (setup, expected_action) in tests {
            let mut mapper = MapperState::default();
            let input = input_with(setup);
            let actions = mapper.update(&input);
            let seq: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::KeySequence(s) => Some(s),
                    _ => None,
                })
                .collect();
            assert_eq!(seq.len(), 1, "Expected 1 KeySequence for button");
            assert_eq!(seq[0][0], vec![VKey::Control, VKey::B], "Wrong prefix");
            assert_eq!(seq[0][1], expected_action, "Wrong action key");
        }
    }

    #[test]
    fn tmux_unmapped_buttons_do_nothing() {
        // Share / Options stay unmapped in ship defaults (touchpad is mapped).
        let unmapped: Vec<fn(&mut UnifiedInput)> = vec![
            |i| i.buttons.share = true,
            |i| i.buttons.options = true,
        ];

        for setup in unmapped {
            let mut mapper = MapperState::default();
            let input = input_with(setup);
            let actions = mapper.update(&input);
            assert!(
                !actions.iter().any(|a| matches!(a, Action::KeySequence(_))),
                "Unmapped button should not fire KeySequence"
            );
        }
    }

    #[test]
    fn r3_sends_ctrl_u() {
        let mut mapper = MapperState::default();
        let input = input_with(|i| i.buttons.r3 = true);
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::U])),
            "R3 should send Ctrl+U"
        );
    }

    #[test]
    fn l3_sends_godspeed_launcher_text() {
        let mut mapper = MapperState::default();
        let input = input_with(|i| i.buttons.l3 = true);
        let actions = mapper.update(&input);
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::LauncherText { text, enter }
                    if text == "| godspeed" && *enter
            )),
            "L3 should emit launcher:godspeed (| godspeed + Enter), got: {actions:?}"
        );
    }

    #[test]
    fn cross_default_sends_enter_through_mapped_path() {
        let mut mapper = MapperState::default();
        let input = input_with(|i| i.buttons.cross = true);
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Return])),
            "Default cross should resolve to Enter via the configurable mapping"
        );
    }

    #[test]
    fn cross_override_resolves_to_direct_combo() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.cross = "ctrl+g".into();
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.cross = true);
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::G])),
            "Overridden cross mapping should resolve to Ctrl+G"
        );
    }

    #[test]
    fn cross_empty_string_is_unmapped() {
        let mut cfg = crate::config::Config::default();
        cfg.buttons.cross = "".into();
        let mut mapper =
            MapperState::new(&cfg, &Detected::default(), Arc::new(AtomicBool::new(false)));

        let input = input_with(|i| i.buttons.cross = true);
        let actions = mapper.update(&input);
        assert!(actions.is_empty(), "Empty cross config should be unmapped");
    }

    #[test]
    fn parse_combo_ctrl_b() {
        let combo = parse_key_combo("Ctrl+B").unwrap();
        assert_eq!(combo, vec![VKey::Control, VKey::B]);
    }

    #[test]
    fn parse_single_key() {
        let combo = parse_key_combo("p").unwrap();
        assert_eq!(combo, vec![VKey::P]);
    }

    // ── Touchpad tests ────────────────────────────────────────────────

    fn input_with_touch(x: u16, y: u16, click: bool) -> UnifiedInput {
        let mut i = UnifiedInput::default();
        i.touchpad[0] = crate::input::TouchPoint { active: true, x, y };
        i.buttons.touchpad = click;
        i
    }

    #[test]
    fn touchpad_first_frame_no_move() {
        let mut mapper = MapperState::default();
        // First frame of active touch: no prev → no MouseMove emitted
        let input = input_with_touch(500, 300, false);
        let actions = mapper.update(&input);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. })),
            "No MouseMove on first touch frame"
        );
    }

    #[test]
    fn touchpad_second_frame_emits_move() {
        let mut mapper = MapperState::default();
        mapper.update(&input_with_touch(500, 300, false));
        // Second frame: moved right 10, down 5
        let actions = mapper.update(&input_with_touch(510, 305, false));
        let moves: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::MouseMove { dx, dy } => Some((*dx, *dy)),
                _ => None,
            })
            .collect();
        assert_eq!(moves.len(), 1, "Expected one MouseMove");
        // With default sensitivity 1.5: dx=(10*1.5)=15, dy=(5*1.5)=7
        assert_eq!(moves[0], (15, 7));
    }

    #[test]
    fn touchpad_lift_clears_prev() {
        let mut mapper = MapperState::default();
        mapper.update(&input_with_touch(500, 300, false));
        mapper.update(&input_with_touch(510, 305, false));
        // Lift
        mapper.update(&UnifiedInput::default());
        assert!(mapper.prev_touch.is_none(), "prev_touch cleared after lift");
        // Re-touch at a new position: should NOT emit move (no prev)
        let actions = mapper.update(&input_with_touch(900, 600, false));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. })),
            "No move on re-touch after lift"
        );
    }

    #[test]
    fn touchpad_no_move_when_stationary() {
        let mut mapper = MapperState::default();
        mapper.update(&input_with_touch(500, 300, false));
        // Same position: raw delta = 0,0 → scaled = 0,0 → no action
        let actions = mapper.update(&input_with_touch(500, 300, false));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. }))
        );
    }

    #[test]
    fn touchpad_click_rising_edge_when_unmapped() {
        // Empty touchpad mapping → mouse left-click while cursor mode is on.
        let mut mapper = MapperState::default();
        mapper.clear_touchpad_maps();
        let input = input_with_touch(500, 300, true);
        let actions = mapper.update(&input);
        assert!(
            actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "MouseClick on first press frame when touchpad is unmapped"
        );
        // Hold: no second click
        let actions = mapper.update(&input);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "No click on hold"
        );
    }

    #[test]
    fn touchpad_press_default_opens_new_window_home() {
        // Ship default: touchpad press → tmux new-window -c ~ (prefix + C).
        let mut mapper = MapperState::default();
        assert!(
            mapper.active_touchpad_mapped(),
            "default ButtonMap must map touchpad"
        );
        let input = input_with_touch(500, 300, true);
        let actions = mapper.update(&input);
        assert!(
            actions.iter().any(|a| matches!(a, Action::KeySequence(_))),
            "default touchpad press should emit tmux KeySequence, not MouseClick"
        );
        assert!(
            !actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "mapped touchpad must not also left-click"
        );
    }

    #[test]
    fn touchpad_disabled_no_swipe_but_mapped_press_still_fires() {
        let mut mapper = MapperState {
            touchpad_enabled: false,
            ..Default::default()
        };
        let input = input_with_touch(500, 300, true);
        mapper.update(&input_with_touch(400, 200, false)); // set prev_touch (should be skipped)
        let actions = mapper.update(&input);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. }))
        );
        // Swipe off does not suppress a configured touchpad *button* map.
        assert!(
            actions.iter().any(|a| matches!(a, Action::KeySequence(_))),
            "touchpad press mapping should work with [touchpad] enabled = false"
        );
    }

    // ── Left stick mouse tests ────────────────────────────────────────

    fn input_with_left_stick(lx: u8, ly: u8) -> UnifiedInput {
        UnifiedInput {
            left_stick: (lx, ly),
            ..Default::default()
        }
    }

    #[test]
    fn stick_mouse_center_no_action() {
        let mut mapper = MapperState::default();
        // Centered stick → no MouseMove
        let actions = mapper.update(&input_with_left_stick(128, 128));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. }))
        );
    }

    #[test]
    fn stick_mouse_dead_zone_no_action() {
        let mut mapper = MapperState::default();
        // Deflection of 10 < dead_zone (15) → no move
        let actions = mapper.update(&input_with_left_stick(138, 118));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. }))
        );
    }

    /// Helper: activate stick mouse mode for tests.
    fn enable_stick_mode(mapper: &MapperState) {
        mapper.mouse_stick_active.store(true, Ordering::Relaxed);
    }

    #[test]
    fn stick_mouse_beyond_dead_zone_emits_move() {
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        // Full right deflection (lx=255, dy_raw=127 > dead_zone=15)
        let actions = mapper.update(&input_with_left_stick(255, 128));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { dx, .. } if *dx > 0)),
            "Full right deflection should produce positive dx"
        );
    }

    #[test]
    fn stick_mouse_direction_up_negative_dy() {
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        // Full up deflection (ly=0, dy_raw=-128 < 0)
        let actions = mapper.update(&input_with_left_stick(128, 0));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { dy, .. } if *dy < 0)),
            "Full up deflection should produce negative dy"
        );
    }

    #[test]
    fn stick_mouse_accumulates_subpixel() {
        // sensitivity=0.3, dx_raw=64 → vx≈0.151 px/frame → needs ~7 frames to cross 1px
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        mapper.stick_mouse_sensitivity = 0.3;
        mapper.stick_mouse_dead_zone = 0;

        let input = input_with_left_stick(192, 128); // dx_raw=64
        let fired = (0..10).any(|_| {
            mapper
                .update(&input)
                .iter()
                .any(|a| matches!(a, Action::MouseMove { dx, .. } if *dx > 0))
        });
        assert!(
            fired,
            "Sub-pixel accumulator should emit move after enough frames"
        );
    }

    #[test]
    fn stick_mouse_acc_resets_at_center() {
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        // Push right, then center
        mapper.update(&input_with_left_stick(255, 128));
        mapper.update(&UnifiedInput::default()); // center
        assert_eq!(
            mapper.stick_acc_x, 0.0,
            "Accumulator should reset at center"
        );
        assert_eq!(mapper.stick_acc_y, 0.0);
    }

    #[test]
    fn stick_mouse_disabled_no_actions() {
        // stick_mouse_enabled=false overrides even if stick mode is selected
        let mut mapper = MapperState {
            stick_mouse_enabled: false,
            ..Default::default()
        };
        enable_stick_mode(&mapper);
        let actions = mapper.update(&input_with_left_stick(255, 0));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. }))
        );
    }

    // ── Mouse mode switching tests ────────────────────────────────────

    #[test]
    fn stick_mode_off_suppresses_stick_move() {
        let mut mapper = MapperState::default();
        // Default: stick mode off → full stick deflection produces no MouseMove
        let actions = mapper.update(&input_with_left_stick(255, 128));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. })),
            "Stick should not move cursor when stick mode is off"
        );
    }

    #[test]
    fn stick_mode_on_suppresses_touchpad_move() {
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        // Prime prev_touch as if we were in touchpad mode, then switch
        mapper.prev_touch = Some((500, 300));
        // Touchpad touch should NOT emit MouseMove when stick mode is active
        let actions = mapper.update(&input_with_touch(510, 305, false));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. })),
            "Touchpad touch should not move cursor when stick mode is on"
        );
    }

    #[test]
    fn touchpad_click_always_fires_in_stick_mode_when_unmapped() {
        let mut mapper = MapperState::default();
        mapper.clear_touchpad_maps();
        enable_stick_mode(&mapper);
        // Unmapped touchpad press → click must fire even in stick mode
        let actions = mapper.update(&input_with_touch(500, 300, true));
        assert!(
            actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "Unmapped touchpad click must fire regardless of mouse mode"
        );
        // But no touch movement
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::MouseMove { .. })),
            "No touch movement in stick mode"
        );
    }

    #[test]
    fn switching_modes_clears_prev_touch() {
        let mut mapper = MapperState::default();
        // Establish prev_touch in touchpad mode
        mapper.update(&input_with_touch(500, 300, false));
        assert!(mapper.prev_touch.is_some());
        // Switch to stick mode — next frame clears prev_touch
        enable_stick_mode(&mapper);
        mapper.update(&input_with_touch(510, 305, false));
        assert!(
            mapper.prev_touch.is_none(),
            "prev_touch must clear when mode switches to stick"
        );
    }

    #[test]
    fn vkey_from_name_coverage() {
        assert_eq!(VKey::from_name("enter"), Some(VKey::Return));
        assert_eq!(VKey::from_name("Ctrl"), Some(VKey::Control));
        assert_eq!(VKey::from_name(";"), Some(VKey::Semicolon));
        assert_eq!(VKey::from_name("["), Some(VKey::LeftBracket));
        assert_eq!(VKey::from_name("z"), Some(VKey::Z));
        assert_eq!(VKey::from_name("unknown"), None);
    }

    // ── VKey → portable Key lowering (feeds the platform Injector) ────

    #[test]
    fn to_key_maps_modifiers_and_named_keys() {
        assert_eq!(to_key(VKey::Control), Key::Ctrl);
        assert_eq!(to_key(VKey::Alt), Key::Alt);
        assert_eq!(to_key(VKey::Shift), Key::Shift);
        assert_eq!(to_key(VKey::Win), Key::Super);
        assert_eq!(to_key(VKey::Return), Key::Enter);
        assert_eq!(to_key(VKey::Escape), Key::Escape);
        assert_eq!(to_key(VKey::Tab), Key::Tab);
        assert_eq!(to_key(VKey::Space), Key::Space);
        assert_eq!(to_key(VKey::Up), Key::Up);
        assert_eq!(to_key(VKey::Down), Key::Down);
        assert_eq!(to_key(VKey::Left), Key::Left);
        assert_eq!(to_key(VKey::Right), Key::Right);
    }

    #[test]
    fn to_key_maps_letters_digits_punct_to_unshifted_chars() {
        assert_eq!(to_key(VKey::A), Key::Char('a'));
        assert_eq!(to_key(VKey::Z), Key::Char('z'));
        assert_eq!(to_key(VKey::D0), Key::Char('0'));
        assert_eq!(to_key(VKey::D7), Key::Char('7'));
        assert_eq!(to_key(VKey::Semicolon), Key::Char(';'));
        assert_eq!(to_key(VKey::LeftBracket), Key::Char('['));
        assert_eq!(to_key(VKey::RightBracket), Key::Char(']'));
        assert_eq!(to_key(VKey::Backslash), Key::Char('\\'));
        assert_eq!(to_key(VKey::Quote), Key::Char('\''));
        assert_eq!(to_key(VKey::Slash), Key::Char('/'));
        assert_eq!(to_key(VKey::Minus), Key::Char('-'));
        assert_eq!(to_key(VKey::Equals), Key::Char('='));
        assert_eq!(to_key(VKey::Comma), Key::Char(','));
        assert_eq!(to_key(VKey::Period), Key::Char('.'));
        assert_eq!(to_key(VKey::Backtick), Key::Char('`'));
    }

    #[test]
    fn to_key_maps_function_keys() {
        assert_eq!(to_key(VKey::F1), Key::F(1));
        assert_eq!(to_key(VKey::F12), Key::F(12));
    }

    #[test]
    fn combo_ctrl_shift_b_lowers_in_order() {
        let combo = parse_key_combo("ctrl+shift+b").expect("combo parses");
        let lowered = to_keys(&combo);
        assert_eq!(lowered, vec![Key::Ctrl, Key::Shift, Key::Char('b')]);
    }

    #[test]
    fn staged_combo_ctrl_n_timing_for_triangle() {
        // Triangle → ctrl+n :
        // Ctrl↓ + n↓ → 19ms → Ctrl↑ + n↑
        let plan = staged_combo_plan(&[Key::Ctrl, Key::Char('n')]);
        assert_eq!(
            plan,
            vec![
                ComboStep::Down(Key::Ctrl),
                ComboStep::Down(Key::Char('n')),
                ComboStep::WaitMs(COMBO_HOLD_MS),
                ComboStep::Up(Key::Ctrl),
                ComboStep::Up(Key::Char('n')),
            ]
        );
        assert_eq!(COMBO_HOLD_MS, 19);
    }

    #[test]
    fn staged_combo_single_key_no_waits() {
        let plan = staged_combo_plan(&[Key::Enter]);
        assert_eq!(
            plan,
            vec![ComboStep::Down(Key::Enter), ComboStep::Up(Key::Enter)]
        );
    }

    #[test]
    fn staged_combo_ctrl_shift_b_hold_then_press_order_release() {
        let plan = staged_combo_plan(&[Key::Ctrl, Key::Shift, Key::Char('b')]);
        assert_eq!(
            plan,
            vec![
                ComboStep::Down(Key::Ctrl),
                ComboStep::Down(Key::Shift),
                ComboStep::Down(Key::Char('b')),
                ComboStep::WaitMs(COMBO_HOLD_MS),
                ComboStep::Up(Key::Ctrl),
                ComboStep::Up(Key::Shift),
                ComboStep::Up(Key::Char('b')),
            ]
        );
    }

    #[test]
    fn shifted_char_combo_keeps_explicit_shift() {
        // Legacy tmux default "kill-window" = Shift+7 ('&'): the shift must
        // stay an explicit key so Windows VK and Linux evdev paths agree.
        let combo = default_key_for_action("kill-window").expect("tmux default");
        let lowered = to_keys(&combo);
        assert_eq!(lowered, vec![Key::Shift, Key::Char('7')]);
    }

    #[test]
    fn l2_hold_combo_lowers_to_ctrl_super() {
        // Fixed mapping: L2 hold = Ctrl+Win.
        let lowered = to_keys(&[VKey::Control, VKey::Win]);
        assert_eq!(lowered, vec![Key::Ctrl, Key::Super]);
    }

    #[test]
    fn tmux_prefix_default_lowers_to_ctrl_b() {
        // Hardcoded fallback tmux prefix is Ctrl+B on every platform.
        let lowered = to_keys(&[VKey::Control, VKey::B]);
        assert_eq!(lowered, vec![Key::Ctrl, Key::Char('b')]);
    }

    #[test]
    fn force_release_holds_emits_l2_keyup_once() {
        let mut state = MapperState::default();
        state.prev.l2 = true;
        let first = state.force_release_holds();
        assert_eq!(first.len(), 1);
        assert!(matches!(
            &first[0],
            Action::KeyUp(keys) if keys.as_slice() == [VKey::Control, VKey::Win]
        ));
        assert!(state.force_release_holds().is_empty());
    }

    /// Recording injector for pure refcount tests (no OS / uinput).
    struct RecInjector {
        down: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<Key>>>,
    }

    impl Injector for RecInjector {
        fn combo(&mut self, keys: &[Key]) {
            for &k in keys {
                self.key_down(k);
            }
            for &k in keys.iter().rev() {
                self.key_up(k);
            }
        }
        fn key_down(&mut self, k: Key) {
            self.down.lock().unwrap().insert(k);
        }
        fn key_up(&mut self, k: Key) {
            self.down.lock().unwrap().remove(&k);
        }
        fn mouse_rel(&mut self, _dx: i32, _dy: i32) {}
        fn wheel(&mut self, _v: i32, _h: i32) {}
        fn click(&mut self) {}
    }

    #[test]
    fn refcount_injector_keeps_l2_ctrl_through_nested_combo() {
        let held = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
        let mut inj = RefcountInjector {
            inner: Box::new(RecInjector {
                down: std::sync::Arc::clone(&held),
            }),
            counts: HashMap::new(),
        };
        // L2 hold
        inj.key_down(Key::Ctrl);
        inj.key_down(Key::Super);
        // Nested tmux-style combo that also uses Ctrl
        inj.combo(&[Key::Ctrl, Key::Char('b')]);
        {
            let down = held.lock().unwrap();
            assert!(
                down.contains(&Key::Ctrl),
                "Ctrl must stay down after nested combo while L2 held"
            );
            assert!(down.contains(&Key::Super));
            assert!(!down.contains(&Key::Char('b')));
        }
        // L2 release
        inj.key_up(Key::Ctrl);
        inj.key_up(Key::Super);
        let down = held.lock().unwrap();
        assert!(!down.contains(&Key::Ctrl));
        assert!(!down.contains(&Key::Super));
    }
}
