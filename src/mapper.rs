/// Button mapper: translates UnifiedInput → keyboard/mouse events via SendInput.
///
/// Fixed mappings (always active, not user-configurable):
///   D-pad Up/Down/Left/Right → Arrow keys (two-frame confirm + repeat)
///   Left stick  → Mouse cursor (velocity-based, configurable sensitivity)
///   Right stick → Mouse scroll wheel (vertical + horizontal)
///   L2       → Ctrl+Win (hold)
///
/// Configurable mappings ([buttons] in config.toml — L1, R1, R2, Square,
/// Share, Options, Touchpad, Cross, Circle, Triangle, L3, R3) resolve at
/// startup to a key sequence, from a tmux action name (prefix + detected
/// key), a Claude Code action name (detected from ~/.claude/keybindings.json),
/// launcher action (e.g. "launcher:godspeed" -> configured Unicode text), or
/// direct key combo.
/// Defaults: L1/R1 → prev/next tmux window, R2 → kill-window, Square →
/// new-window, Cross → Enter, Circle → Escape, Triangle → Tab, L3 → Ctrl+T,
/// R3 → Ctrl+U.
///
/// Combos are sent atomically in a single SendInput call.
use crate::config::{ButtonsConfig, LauncherAction, TmuxConfig};
use crate::detect::Detected;
use crate::input::{ButtonState, DPad, UnifiedInput};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

/// Milliseconds to wait after the full launcher text has been injected, before
/// pressing Return. Direct clone of claude-launcher's two-phase submit: the text
/// is delivered as one instantaneous batch (zero per-character delay), then—after
/// this guard—Return fires, so the focused app never sees the text and Enter
/// racing. Shared verbatim by the Windows `SendInput` and Linux `wtype`/`xdotool`
/// paths so submit timing is identical on every platform.
pub const ENTER_DELAY_MS: u64 = 16;

#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_MENU,
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
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

#[cfg(windows)]
impl VKey {
    fn code(self) -> u16 {
        match self {
            VKey::Return => VK_RETURN,
            VKey::Escape => VK_ESCAPE,
            VKey::Tab => VK_TAB,
            VKey::Up => VK_UP,
            VKey::Down => VK_DOWN,
            VKey::Left => VK_LEFT,
            VKey::Right => VK_RIGHT,
            VKey::Alt => VK_MENU,
            VKey::Shift => VK_SHIFT,
            VKey::Control => VK_CONTROL,
            VKey::Win => 0x5B, // VK_LWIN
            VKey::A => 0x41,
            VKey::B => 0x42,
            VKey::C => 0x43,
            VKey::D => 0x44,
            VKey::E => 0x45,
            VKey::F => 0x46,
            VKey::G => 0x47,
            VKey::H => 0x48,
            VKey::I => 0x49,
            VKey::J => 0x4A,
            VKey::K => 0x4B,
            VKey::L => 0x4C,
            VKey::M => 0x4D,
            VKey::N => 0x4E,
            VKey::O => 0x4F,
            VKey::P => 0x50,
            VKey::Q => 0x51,
            VKey::R => 0x52,
            VKey::S => 0x53,
            VKey::T => 0x54,
            VKey::U => 0x55,
            VKey::V => 0x56,
            VKey::W => 0x57,
            VKey::X => 0x58,
            VKey::Y => 0x59,
            VKey::Z => 0x5A,
            VKey::D0 => 0x30,
            VKey::D1 => 0x31,
            VKey::D2 => 0x32,
            VKey::D3 => 0x33,
            VKey::D4 => 0x34,
            VKey::D5 => 0x35,
            VKey::D6 => 0x36,
            VKey::D7 => 0x37,
            VKey::D8 => 0x38,
            VKey::D9 => 0x39,
            VKey::Semicolon => 0xBA,    // VK_OEM_1
            VKey::LeftBracket => 0xDB,  // VK_OEM_4
            VKey::RightBracket => 0xDD, // VK_OEM_6
            VKey::Backslash => 0xDC,    // VK_OEM_5
            VKey::Quote => 0xDE,        // VK_OEM_7
            VKey::Slash => 0xBF,        // VK_OEM_2
            VKey::Minus => 0xBD,        // VK_OEM_MINUS
            VKey::Equals => 0xBB,       // VK_OEM_PLUS (unshifted =)
            VKey::Comma => 0xBC,        // VK_OEM_COMMA
            VKey::Period => 0xBE,       // VK_OEM_PERIOD
            VKey::Backtick => 0xC0,     // VK_OEM_3
            VKey::Space => 0x20,        // VK_SPACE
            VKey::F1 => 0x70,
            VKey::F2 => 0x71,
            VKey::F3 => 0x72,
            VKey::F4 => 0x73,
            VKey::F5 => 0x74,
            VKey::F6 => 0x75,
            VKey::F7 => 0x76,
            VKey::F8 => 0x77,
            VKey::F9 => 0x78,
            VKey::F10 => 0x79,
            VKey::F11 => 0x7A,
            VKey::F12 => 0x7B,
        }
    }
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
        "new-window" => Some(vec![VKey::C]),
        "kill-window" => Some(vec![VKey::Shift, VKey::D7]), // &
        "copy-mode" => Some(vec![VKey::LeftBracket]),
        "resize-pane -Z" => Some(vec![VKey::Z]), // zoom toggle
        "last-pane" => Some(vec![VKey::Semicolon]),
        "select-pane" => Some(vec![VKey::O]), // next pane
        "last-window" => Some(vec![VKey::L]),
        "detach-client" => Some(vec![VKey::D]),
        "split-window -h" => Some(vec![VKey::Shift, VKey::D5]), // %
        "split-window -v" => Some(vec![VKey::Shift, VKey::Quote]), // "
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
            // Tmux action → prefix + key sequence
            if let Some(keys) = tmux_detected
                .and_then(|d| d.key_for_action(value).cloned())
                .or_else(|| default_key_for_action(value))
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
    // Configurable button mappings (resolved once at startup)
    buttons: ButtonMap,
}

impl Default for MapperState {
    fn default() -> Self {
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
            buttons: ButtonMap::default(),
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
            buttons: ButtonMap::resolve(&cfg.buttons, &cfg.tmux, detected, &cfg.launchers),
            ..Default::default()
        }
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

        // --- Fixed key mappings ---
        // L2: hold Ctrl+Win while button is held
        if current.l2 && !self.prev.l2 {
            actions.push(Action::KeyDown(vec![VKey::Control, VKey::Win]));
        } else if !current.l2 && self.prev.l2 {
            actions.push(Action::KeyUp(vec![VKey::Control, VKey::Win]));
        }

        // --- Configurable button mappings ---
        macro_rules! on_press_mapped {
            ($field:ident) => {
                if current.$field && !self.prev.$field {
                    if let Some(action) = self.buttons.$field.as_ref() {
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
        // Touchpad press is a mouse click while touchpad-mouse is enabled;
        // with it disabled the touchpad button becomes a mappable button.
        if !self.touchpad_enabled {
            on_press_mapped!(touchpad);
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

    /// Translate touchpad touch coordinates into relative mouse movement and
    /// touchpad click into a left mouse button click.
    ///
    /// Called on every frame BEFORE profile-dependent dispatch so that the
    /// touchpad works identically in both Default and Tmux profiles.
    fn process_touchpad(&mut self, input: &UnifiedInput, actions: &mut Vec<Action>) {
        if !self.touchpad_enabled {
            return; // config-level disable: suppresses both movement and click
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

        // ── Touchpad press → left click (always active regardless of mouse mode) ──
        if input.buttons.touchpad && !self.prev.touchpad {
            log::debug!("TouchpadClick → MouseClick");
            actions.push(Action::MouseClick);
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

// ── Windows SendInput functions ──────────────────────────────────────

/// Send a key combo via Windows SendInput. Modifiers held, main key pressed+released, modifiers released.
#[cfg(windows)]
pub fn send_key_combo(keys: &[VKey]) {
    if keys.is_empty() {
        return;
    }

    let (modifiers, main_key) = keys.split_at(keys.len() - 1);
    let mut inputs: Vec<INPUT> = Vec::with_capacity(keys.len() * 2);

    for &m in modifiers {
        inputs.push(make_key_input(m.code(), 0));
    }
    inputs.push(make_key_input(main_key[0].code(), 0));
    inputs.push(make_key_input(main_key[0].code(), KEYEVENTF_KEYUP));
    for &m in modifiers.iter().rev() {
        inputs.push(make_key_input(m.code(), KEYEVENTF_KEYUP));
    }

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Press keys down (hold). Call send_key_up to release.
#[cfg(windows)]
pub fn send_key_down(keys: &[VKey]) {
    if keys.is_empty() {
        return;
    }
    let inputs: Vec<INPUT> = keys.iter().map(|k| make_key_input(k.code(), 0)).collect();
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Release held keys (reverse order for proper modifier release).
#[cfg(windows)]
pub fn send_key_up(keys: &[VKey]) {
    if keys.is_empty() {
        return;
    }
    let inputs: Vec<INPUT> = keys
        .iter()
        .rev()
        .map(|k| make_key_input(k.code(), KEYEVENTF_KEYUP))
        .collect();
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Send a sequence of key combos with a delay between each (e.g., tmux prefix + action).
#[cfg(windows)]
pub fn send_key_sequence(combos: &[Vec<VKey>], delay_ms: u64) {
    for (i, combo) in combos.iter().enumerate() {
        send_key_combo(combo);
        if i < combos.len() - 1 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
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
        // Give the target app time to process the text batch before Enter arrives
        // so the two events don't race (shared two-phase guard, all platforms).
        std::thread::sleep(std::time::Duration::from_millis(ENTER_DELAY_MS));
        let enter = [
            make_key_input(VK_RETURN, 0),
            make_key_input(VK_RETURN, KEYEVENTF_KEYUP),
        ];
        unsafe {
            SendInput(
                enter.len() as u32,
                enter.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }
}

/// Move the mouse cursor by a relative offset via Windows SendInput.
#[cfg(windows)]
pub fn send_mouse_move(dx: i32, dy: i32) {
    let input = make_mouse_move_input(dx, dy);
    unsafe {
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Send a left mouse button click (down + up) via Windows SendInput.
#[cfg(windows)]
pub fn send_mouse_click() {
    let inputs = [
        make_mouse_flag_input(MOUSEEVENTF_LEFTDOWN),
        make_mouse_flag_input(MOUSEEVENTF_LEFTUP),
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Send a mouse scroll event via Windows SendInput.
#[cfg(windows)]
pub fn send_scroll(horizontal: i32, vertical: i32) {
    let mut inputs: Vec<INPUT> = Vec::new();

    if vertical != 0 {
        inputs.push(make_mouse_input(MOUSEEVENTF_WHEEL, vertical));
    }
    if horizontal != 0 {
        inputs.push(make_mouse_input(MOUSEEVENTF_HWHEEL, horizontal));
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

/// Execute an action (send keystrokes, scroll, or mouse movement/click).
#[cfg(windows)]
pub fn execute_action(action: &Action) {
    match action {
        Action::KeyCombo(keys) => send_key_combo(keys),
        Action::KeyDown(keys) => send_key_down(keys),
        Action::KeyUp(keys) => send_key_up(keys),
        Action::KeySequence(combos) => send_key_sequence(combos, 10),
        Action::Scroll {
            horizontal,
            vertical,
        } => send_scroll(*horizontal, *vertical),
        Action::MouseMove { dx, dy } => send_mouse_move(*dx, *dy),
        Action::MouseClick => send_mouse_click(),
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

/// Ordered injection phases for a launcher action: always type the whole text
/// first (one invocation, zero per-character delay), then—only when submitting—
/// press Return. The `ENTER_DELAY_MS` guard is applied between phase 0 and phase 1
/// by `send_launcher_text`; this pure builder captures the invocation order so it
/// can be asserted without spawning processes.
#[cfg(target_os = "linux")]
pub fn linux_injection_plan(
    backend: LinuxBackend,
    text: &str,
    submit_enter: bool,
) -> Vec<Vec<String>> {
    let mut plan = vec![linux_text_args(backend, text)];
    if submit_enter {
        plan.push(linux_enter_args(backend));
    }
    plan
}

/// Spawn `prog` with `args` as discrete process arguments (never a shell string).
///
/// Returns `true` if the process was spawned **and exited successfully** (exit code 0),
/// `false` if it could not be started (e.g. the backend executable is not installed)
/// or exited with a non-zero status (e.g. compositor rejected injection).
///
/// The caller's early-exit guard (`if !run_injector(..) && i == 0 { return; }`)
/// depends on this returning `false` on non-zero exit so that a failed text batch
/// does not lead to a spurious `Return` keystroke being sent to the focused window.
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

/// Inject Unicode text on Linux, then optionally press Enter.
///
/// Backend is chosen from `XDG_SESSION_TYPE`: Wayland → `wtype`, otherwise
/// `xdotool`. Text is passed as a single discrete argument (no shell), so shell
/// metacharacters and leading dashes have no special meaning. Best-effort: if the
/// selected backend is not installed the call logs a warning and returns.
#[cfg(target_os = "linux")]
pub fn send_launcher_text(text: &str, submit_enter: bool) {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let backend = select_backend(session_type.as_deref());
    let prog = backend.program();

    // Phase 0 = whole text (one invocation, zero per-character delay);
    // phase 1 (only when submitting) = Return, after the shared 16 ms guard.
    for (i, phase_args) in linux_injection_plan(backend, text, submit_enter)
        .iter()
        .enumerate()
    {
        if i > 0 {
            // Give the compositor/app time to process the typed text before Enter,
            // so the two events don't race (shared two-phase guard, all platforms).
            std::thread::sleep(std::time::Duration::from_millis(ENTER_DELAY_MS));
        }
        if !run_injector(prog, phase_args) && i == 0 {
            // Text batch failed to spawn (backend missing) — pressing Enter alone
            // would be meaningless, so bail before the guard/Return phase.
            return;
        }
    }
}

/// Execute an action — no-op stub on non-Windows/non-Linux platforms.
/// Destructures all variants so Action fields are considered "read" on every
/// platform, silencing the dead_code lint without any suppression attribute.
#[cfg(not(windows))]
pub fn execute_action(action: &Action) {
    match action {
        Action::KeyCombo(_keys) => {}
        Action::KeyDown(_keys) => {}
        Action::KeyUp(_keys) => {}
        Action::KeySequence(_combos) => {}
        Action::Scroll {
            horizontal: _h,
            vertical: _v,
        } => {}
        Action::MouseMove { dx: _dx, dy: _dy } => {}
        Action::MouseClick => {}
        #[cfg(target_os = "linux")]
        Action::LauncherText { text, enter } => send_launcher_text(text, *enter),
        #[cfg(not(target_os = "linux"))]
        Action::LauncherText {
            text: _text,
            enter: _enter,
        } => {}
    }
}

// ── Linux injection backend tests ─────────────────────────────────────
//
// These cover the Wayland-first (wtype) / X11-fallback (xdotool) backend:
// selection from XDG_SESSION_TYPE, exact argv construction, Unicode payload,
// optional Enter, graceful missing-executable behavior, and FIFO queue
// preservation across a batch of queued launcher actions.
#[cfg(all(test, target_os = "linux"))]
mod linux_inject_tests {
    use super::{
        ENTER_DELAY_MS, LinuxBackend, linux_enter_args, linux_injection_plan, linux_text_args,
        run_injector, select_backend,
    };

    // ── Two-phase submit timing / ordering ────────────────────────────

    #[test]
    fn enter_delay_is_exactly_16ms() {
        // Post-text submit guard: exactly 16 ms after the text batch, before Return.
        assert_eq!(ENTER_DELAY_MS, 16);
    }

    #[test]
    fn injection_plan_types_text_then_enter_when_submitting() {
        // Phase order is fixed: whole text first (one invocation, zero per-char
        // delay), then Return. Nothing precedes the text; Return is strictly last.
        let plan = linux_injection_plan(LinuxBackend::Wayland, "| godspeed", true);
        assert_eq!(plan.len(), 2, "submit = two phases: text, then Return");
        assert_eq!(plan[0], vec!["--".to_string(), "| godspeed".to_string()]);
        assert_eq!(plan[1], linux_enter_args(LinuxBackend::Wayland));
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
    fn return_not_emitted_when_text_phase_exits_nonzero() {
        // Full guard-loop regression: replays the send_launcher_text loop
        // using `false` as the injector (always exits 1). With the fix in place,
        // run_injector returns false → early-exit guard fires at i==0 → the
        // Return phase is never reached.
        //
        // `phases_started` counts how many phases the loop body enters.
        // Expected: 1 (only the text phase starts; Return phase is suppressed).
        let mut phases_started = 0usize;
        let plan = linux_injection_plan(LinuxBackend::Wayland, "test payload", true);
        for (i, args) in plan.iter().enumerate() {
            phases_started += 1;
            if !run_injector("false", args) && i == 0 {
                // Guard fires: text batch failed, bail before Return.
                break;
            }
        }
        assert_eq!(
            phases_started, 1,
            "only the text phase (i=0) must start; Return phase must be suppressed on non-zero exit"
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
    fn ps_does_nothing() {
        let mut mapper = MapperState::default();
        let ps_press = input_with(|i| i.buttons.ps = true);
        let actions = mapper.update(&ps_press);
        assert!(actions.is_empty(), "PS button is unmapped");
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
        // These buttons are unmapped in the default tmux config
        let unmapped: Vec<fn(&mut UnifiedInput)> = vec![
            |i| i.buttons.share = true,
            |i| i.buttons.options = true,
            |i| i.buttons.touchpad = true,
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
    fn l3_sends_ctrl_t() {
        let mut mapper = MapperState::default();
        let input = input_with(|i| i.buttons.l3 = true);
        let actions = mapper.update(&input);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::KeyCombo(k) if *k == vec![VKey::Control, VKey::T])),
            "L3 should send Ctrl+T"
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
    fn touchpad_click_rising_edge() {
        let mut mapper = MapperState::default();
        let input = input_with_touch(500, 300, true);
        let actions = mapper.update(&input);
        assert!(
            actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "MouseClick on first press frame"
        );
        // Hold: no second click
        let actions = mapper.update(&input);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "No click on hold"
        );
    }

    #[test]
    fn touchpad_disabled_no_actions() {
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
        assert!(!actions.iter().any(|a| matches!(a, Action::MouseClick)));
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
    fn touchpad_click_always_fires_in_stick_mode() {
        let mut mapper = MapperState::default();
        enable_stick_mode(&mapper);
        // Touchpad press → click must fire even in stick mode
        let actions = mapper.update(&input_with_touch(500, 300, true));
        assert!(
            actions.iter().any(|a| matches!(a, Action::MouseClick)),
            "Touchpad click must fire regardless of mouse mode"
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
}
