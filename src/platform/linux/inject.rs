//! Linux input injection via evdev/uinput.
//!
//! One virtual device, `ds4cc-virtual-input`, carrying the full key set used
//! by any default mapping plus every [`Key`] variant (US-layout superset),
//! relative pointer axes, wheel/hwheel, and BTN_LEFT.
//!
//! Combo semantics mirror the legacy Windows SendInput ordering: press keys
//! in order, release in reverse, SYN after each phase. Holds (d-pad repeat)
//! use `key_down`/`key_up` without auto-release.
//!
//! Scroll: the mapper speaks Windows wheel-delta units (120/notch, positive
//! = up/right). evdev REL_WHEEL counts *clicks* (positive = up as well), so
//! this module converts delta → clicks with a remainder accumulator (the
//! sign convention already matches; only the unit differs). This keeps the
//! mapper's scroll math fully shared.
//!
//! Never panics: a missing/unwritable /dev/uinput yields
//! [`InjectError::UinputUnavailable`] and every emit failure is logged.

use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};

use crate::keys::Key;
use crate::platform::{InjectError, Injector};

/// Windows wheel delta for one notch; the mapper emits multiples of this.
const WHEEL_DELTA: i32 = 120;

const UINPUT_REMEDIATION: &str = "run: sudo modprobe uinput; \
    install packaging/linux/99-ds4cc.rules (udev); \
    add your user to the 'uinput' group; re-login";

/// Every key the virtual device can emit: all letters, digits, US-layout
/// punctuation, F1–F12, modifiers, navigation cluster, system keys, and the
/// left mouse button. Superset of every `Key::to_evdev()` result.
const FULL_KEY_SET: &[KeyCode] = &[
    // Modifiers (left + right)
    KeyCode::KEY_LEFTCTRL,
    KeyCode::KEY_RIGHTCTRL,
    KeyCode::KEY_LEFTALT,
    KeyCode::KEY_RIGHTALT,
    KeyCode::KEY_LEFTSHIFT,
    KeyCode::KEY_RIGHTSHIFT,
    KeyCode::KEY_LEFTMETA,
    KeyCode::KEY_RIGHTMETA,
    // Letters
    KeyCode::KEY_A,
    KeyCode::KEY_B,
    KeyCode::KEY_C,
    KeyCode::KEY_D,
    KeyCode::KEY_E,
    KeyCode::KEY_F,
    KeyCode::KEY_G,
    KeyCode::KEY_H,
    KeyCode::KEY_I,
    KeyCode::KEY_J,
    KeyCode::KEY_K,
    KeyCode::KEY_L,
    KeyCode::KEY_M,
    KeyCode::KEY_N,
    KeyCode::KEY_O,
    KeyCode::KEY_P,
    KeyCode::KEY_Q,
    KeyCode::KEY_R,
    KeyCode::KEY_S,
    KeyCode::KEY_T,
    KeyCode::KEY_U,
    KeyCode::KEY_V,
    KeyCode::KEY_W,
    KeyCode::KEY_X,
    KeyCode::KEY_Y,
    KeyCode::KEY_Z,
    // Digits
    KeyCode::KEY_1,
    KeyCode::KEY_2,
    KeyCode::KEY_3,
    KeyCode::KEY_4,
    KeyCode::KEY_5,
    KeyCode::KEY_6,
    KeyCode::KEY_7,
    KeyCode::KEY_8,
    KeyCode::KEY_9,
    KeyCode::KEY_0,
    // US-layout punctuation
    KeyCode::KEY_SEMICOLON,
    KeyCode::KEY_LEFTBRACE,
    KeyCode::KEY_RIGHTBRACE,
    KeyCode::KEY_BACKSLASH,
    KeyCode::KEY_APOSTROPHE,
    KeyCode::KEY_SLASH,
    KeyCode::KEY_MINUS,
    KeyCode::KEY_EQUAL,
    KeyCode::KEY_COMMA,
    KeyCode::KEY_DOT,
    KeyCode::KEY_GRAVE,
    // Editing / whitespace
    KeyCode::KEY_ENTER,
    KeyCode::KEY_ESC,
    KeyCode::KEY_TAB,
    KeyCode::KEY_SPACE,
    KeyCode::KEY_BACKSPACE,
    KeyCode::KEY_DELETE,
    KeyCode::KEY_INSERT,
    // Navigation
    KeyCode::KEY_UP,
    KeyCode::KEY_DOWN,
    KeyCode::KEY_LEFT,
    KeyCode::KEY_RIGHT,
    KeyCode::KEY_HOME,
    KeyCode::KEY_END,
    KeyCode::KEY_PAGEUP,
    KeyCode::KEY_PAGEDOWN,
    // Function keys
    KeyCode::KEY_F1,
    KeyCode::KEY_F2,
    KeyCode::KEY_F3,
    KeyCode::KEY_F4,
    KeyCode::KEY_F5,
    KeyCode::KEY_F6,
    KeyCode::KEY_F7,
    KeyCode::KEY_F8,
    KeyCode::KEY_F9,
    KeyCode::KEY_F10,
    KeyCode::KEY_F11,
    KeyCode::KEY_F12,
    // System keys
    KeyCode::KEY_SYSRQ, // PrintScreen
    KeyCode::KEY_SCROLLLOCK,
    KeyCode::KEY_PAUSE,
    KeyCode::KEY_CAPSLOCK,
    KeyCode::KEY_NUMLOCK,
    KeyCode::KEY_MENU,
    // Mouse
    KeyCode::BTN_LEFT,
];

/// evdev/uinput-backed [`Injector`].
pub struct UinputInjector {
    dev: evdev::uinput::VirtualDevice,
    /// Remainder accumulators converting Windows wheel deltas to clicks.
    vwheel_acc: i32,
    hwheel_acc: i32,
}

impl UinputInjector {
    pub fn new() -> Result<Self, InjectError> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for kc in FULL_KEY_SET {
            keys.insert(*kc);
        }
        let mut rel = AttributeSet::<RelativeAxisCode>::new();
        rel.insert(RelativeAxisCode::REL_X);
        rel.insert(RelativeAxisCode::REL_Y);
        rel.insert(RelativeAxisCode::REL_WHEEL);
        rel.insert(RelativeAxisCode::REL_HWHEEL);

        // VirtualDevice::builder() == VirtualDeviceBuilder::new() (non-deprecated).
        let dev = evdev::uinput::VirtualDevice::builder()
            .map_err(|e| uinput_unavailable("open /dev/uinput", e))?
            .name("ds4cc-virtual-input")
            .with_keys(&keys)
            .map_err(|e| uinput_unavailable("configure keys", e))?
            .with_relative_axes(&rel)
            .map_err(|e| uinput_unavailable("configure relative axes", e))?
            .build()
            .map_err(|e| uinput_unavailable("create virtual device", e))?;

        Ok(Self {
            dev,
            vwheel_acc: 0,
            hwheel_acc: 0,
        })
    }

    fn key_event(code: KeyCode, value: i32) -> InputEvent {
        InputEvent::new(EventType::KEY.0, code.0, value)
    }

    fn rel_event(code: RelativeAxisCode, value: i32) -> InputEvent {
        InputEvent::new(EventType::RELATIVE.0, code.0, value)
    }

    fn syn_event() -> InputEvent {
        InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0)
    }

    /// Append key-down events for `k` (implicit Shift first if needed).
    fn push_down(out: &mut Vec<InputEvent>, k: Key) {
        let code = k.to_evdev();
        if code == KeyCode::KEY_RESERVED {
            log::warn!("inject: no US-layout key for {k:?}; skipped");
            return;
        }
        if k.needs_shift() {
            out.push(Self::key_event(KeyCode::KEY_LEFTSHIFT, 1));
        }
        out.push(Self::key_event(code, 1));
    }

    /// Append key-up events for `k` (mirror of `push_down`).
    fn push_up(out: &mut Vec<InputEvent>, k: Key) {
        let code = k.to_evdev();
        if code == KeyCode::KEY_RESERVED {
            return;
        }
        out.push(Self::key_event(code, 0));
        if k.needs_shift() {
            out.push(Self::key_event(KeyCode::KEY_LEFTSHIFT, 0));
        }
    }

    fn emit(&mut self, events: &[InputEvent]) {
        if events.is_empty() {
            return;
        }
        if let Err(e) = self.dev.emit(events) {
            log::warn!("inject: uinput emit failed: {e}");
        }
    }

    /// Convert one axis of Windows wheel delta to whole clicks, keeping the
    /// remainder so slow scrolling doesn't starve.
    fn delta_to_clicks(acc: &mut i32, delta: i32) -> i32 {
        *acc += delta;
        let clicks = *acc / WHEEL_DELTA;
        *acc -= clicks * WHEEL_DELTA;
        clicks
    }
}

impl Injector for UinputInjector {
    /// Press in order, release in reverse, SYN after the batch (legacy
    /// SendInput combo ordering: modifiers held, main key tapped).
    fn combo(&mut self, keys: &[Key]) {
        if keys.is_empty() {
            return;
        }
        let mut events = Vec::with_capacity(keys.len() * 2 + 2);
        for &k in keys {
            Self::push_down(&mut events, k);
        }
        for &k in keys.iter().rev() {
            Self::push_up(&mut events, k);
        }
        events.push(Self::syn_event());
        self.emit(&events);
    }

    fn key_down(&mut self, k: Key) {
        let mut events = Vec::with_capacity(3);
        Self::push_down(&mut events, k);
        events.push(Self::syn_event());
        self.emit(&events);
    }

    fn key_up(&mut self, k: Key) {
        let mut events = Vec::with_capacity(3);
        Self::push_up(&mut events, k);
        events.push(Self::syn_event());
        self.emit(&events);
    }

    fn mouse_rel(&mut self, dx: i32, dy: i32) {
        let events = [
            Self::rel_event(RelativeAxisCode::REL_X, dx),
            Self::rel_event(RelativeAxisCode::REL_Y, dy),
            Self::syn_event(),
        ];
        self.emit(&events);
    }

    /// Windows deltas (positive = up/right) → evdev clicks. REL_WHEEL is
    /// positive-up like SendInput, so the sign convention carries over; only
    /// the unit conversion happens here.
    fn wheel(&mut self, vertical: i32, horizontal: i32) {
        let vclicks = Self::delta_to_clicks(&mut self.vwheel_acc, vertical);
        let hclicks = Self::delta_to_clicks(&mut self.hwheel_acc, horizontal);
        if vclicks == 0 && hclicks == 0 {
            return;
        }
        let mut events = Vec::with_capacity(3);
        if vclicks != 0 {
            events.push(Self::rel_event(RelativeAxisCode::REL_WHEEL, vclicks));
        }
        if hclicks != 0 {
            events.push(Self::rel_event(RelativeAxisCode::REL_HWHEEL, hclicks));
        }
        events.push(Self::syn_event());
        self.emit(&events);
    }

    fn click(&mut self) {
        let events = [
            Self::key_event(KeyCode::BTN_LEFT, 1),
            Self::key_event(KeyCode::BTN_LEFT, 0),
            Self::syn_event(),
        ];
        self.emit(&events);
    }
}

fn uinput_unavailable(phase: &str, err: std::io::Error) -> InjectError {
    InjectError::UinputUnavailable(format!("{phase} failed: {err}. {UINPUT_REMEDIATION}"))
}

/// Create the platform injector.
///
/// On failure this logs the precise remediation and returns
/// [`InjectError::UinputUnavailable`]; the caller (daemon startup) must treat
/// that as feature-degraded and keep running with injection disabled.
pub fn new_injector() -> Result<Box<dyn Injector>, InjectError> {
    match UinputInjector::new() {
        Ok(injector) => {
            log::info!("inject: virtual input device 'ds4cc-virtual-input' ready");
            Ok(Box::new(injector))
        }
        Err(e) => {
            log::warn!("inject: {e}");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_accumulates_sub_notch_amounts() {
        // Three 40-delta scrolls must produce exactly one click, not zero.
        let mut acc = 0;
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, 40), 0);
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, 40), 0);
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, 40), 1);
        assert_eq!(acc, 0);
    }

    #[test]
    fn wheel_delta_negative_accumulates() {
        let mut acc = 0;
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, -60), 0);
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, -60), -1);
        assert_eq!(acc, 0);
    }

    #[test]
    fn wheel_delta_full_notches_passthrough() {
        let mut acc = 0;
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, 120), 1);
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, 240), 2);
        assert_eq!(UinputInjector::delta_to_clicks(&mut acc, -120), -1);
        assert_eq!(acc, 0);
    }

    #[test]
    fn full_key_set_covers_every_evdev_lowering() {
        use crate::keys::Key;
        let set: std::collections::HashSet<KeyCode> = FULL_KEY_SET.iter().copied().collect();
        let sample = [
            Key::Ctrl,
            Key::Alt,
            Key::Shift,
            Key::Super,
            Key::Enter,
            Key::Escape,
            Key::Tab,
            Key::Space,
            Key::Backspace,
            Key::Delete,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::Insert,
            Key::PrintScreen,
            Key::ScrollLock,
            Key::Pause,
            Key::CapsLock,
            Key::NumLock,
            Key::Menu,
        ];
        for k in sample {
            assert!(set.contains(&k.to_evdev()), "missing {k:?}");
        }
        for n in 1..=12 {
            assert!(set.contains(&Key::F(n).to_evdev()), "missing F{n}");
        }
        // Every US-layout char (letters, digits, punct, shifted symbols).
        for c in "abcdefghijklmnopqrstuvwxyz0123456789;[]\\'/-=,.`!@#$%^&*(){}|:\"<>?_+~".chars() {
            let code = Key::Char(c).to_evdev();
            assert_ne!(code, KeyCode::KEY_RESERVED, "unmapped char {c:?}");
            assert!(set.contains(&code), "missing key for char {c:?}");
        }
    }
}
