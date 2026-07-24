//! Portable key abstraction shared by the Windows and Linux input paths.
//!
//! One enum, [`Key`], replaces the Windows-only `mapper::VKey` at OS
//! boundaries. All combo-string parsing (`"ctrl+shift+b"`, `"Shift+7"`,
//! tmux notation `"C-b"` / `"M-x"` / `"S-Left"`, escaped `"\;"`, single
//! symbols like `"&"`) funnels through [`parse_combo`], preserving the exact
//! semantics of the legacy `mapper::parse_key_combo` and
//! `tmux_detect::parse_tmux_key` parsers:
//!
//! - `'+'`-separated parts are looked up case-insensitively (`"Ctrl+B"`).
//! - tmux modifier prefixes `C-`, `M-C-`, `M-`, `S-` (plus the `M-S-`
//!   extension tmux actually emits for Alt+Shift bindings).
//! - A lone uppercase letter or shifted symbol expands to `[Shift, base]`
//!   on the US layout (`"&"` → `[Shift, Char('7')]`), matching the legacy
//!   `[Shift, D7]` combos.
//!
//! Lowering: [`Key::to_win_vk`] (pure numeric table, no Windows headers, so
//! it is unit-testable on every OS) and [`Key::to_evdev`] (Linux only).

#[cfg(target_os = "linux")]
use evdev::KeyCode;

/// A layout-neutral key. `Char` holds the *typed* character; lowering maps it
/// to the US-layout base key plus an implicit Shift when [`Key::needs_shift`]
/// is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Ctrl,
    Alt,
    Shift,
    Super,
    Enter,
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// Function key F1..=F12.
    F(u8),
    /// Letter / digit / punctuation, layout-neutral (US layout assumed when
    /// lowering, same as every Linux hotkey tool and the legacy VK table).
    Char(char),
    PrintScreen,
    ScrollLock,
    Pause,
    CapsLock,
    NumLock,
    Menu,
}

impl Key {
    /// True if producing this key on a US layout requires holding Shift
    /// (uppercase letters and shifted symbols). Callers that emit raw key
    /// events should hold Shift around this key when it appears without an
    /// explicit `Key::Shift` in the combo.
    pub fn needs_shift(self) -> bool {
        match self {
            Key::Char(c) => {
                unshift_base(c).is_some_and(|base| base != c.to_ascii_lowercase())
                    || c.is_ascii_uppercase()
            }
            _ => false,
        }
    }

    /// Lower to a Windows virtual-key code.
    ///
    /// Returns `(vk, shift_base)`:
    /// - `vk` is the VK code (0 when the key cannot be represented).
    /// - `shift_base` is `Some(base_char)` when Shift must be held to produce
    ///   the character (e.g. `Char('&')` → `(0x37, Some('7'))`), `None`
    ///   otherwise.
    ///
    /// The numeric table matches the legacy `mapper::VKey::code()` exactly.
    pub fn to_win_vk(self) -> (u16, Option<char>) {
        match self {
            Key::Ctrl => (0x11, None),      // VK_CONTROL
            Key::Alt => (0x12, None),       // VK_MENU
            Key::Shift => (0x10, None),     // VK_SHIFT
            Key::Super => (0x5B, None),     // VK_LWIN
            Key::Enter => (0x0D, None),     // VK_RETURN
            Key::Escape => (0x1B, None),    // VK_ESCAPE
            Key::Tab => (0x09, None),       // VK_TAB
            Key::Space => (0x20, None),     // VK_SPACE
            Key::Backspace => (0x08, None), // VK_BACK
            Key::Delete => (0x2E, None),    // VK_DELETE
            Key::Up => (0x26, None),        // VK_UP
            Key::Down => (0x28, None),      // VK_DOWN
            Key::Left => (0x25, None),      // VK_LEFT
            Key::Right => (0x27, None),     // VK_RIGHT
            Key::Home => (0x24, None),      // VK_HOME
            Key::End => (0x23, None),       // VK_END
            Key::PageUp => (0x21, None),    // VK_PRIOR
            Key::PageDown => (0x22, None),  // VK_NEXT
            Key::Insert => (0x2D, None),    // VK_INSERT
            Key::F(n) if (1..=12).contains(&n) => (0x70 + n as u16 - 1, None), // VK_F1..F12
            Key::F(_) => (0, None),
            Key::PrintScreen => (0x2C, None), // VK_SNAPSHOT
            Key::ScrollLock => (0x91, None),  // VK_SCROLL
            Key::Pause => (0x13, None),       // VK_PAUSE
            Key::CapsLock => (0x14, None),    // VK_CAPITAL
            Key::NumLock => (0x90, None),     // VK_NUMLOCK
            Key::Menu => (0x5D, None),        // VK_APPS
            Key::Char(c) => char_win_vk(c),
        }
    }

    /// Lower to an evdev key code (US layout). Returns `KeyCode::KEY_RESERVED`
    /// for characters that have no US-layout key; emitters must skip those.
    #[cfg(target_os = "linux")]
    pub fn to_evdev(self) -> KeyCode {
        match self {
            Key::Ctrl => KeyCode::KEY_LEFTCTRL,
            Key::Alt => KeyCode::KEY_LEFTALT,
            Key::Shift => KeyCode::KEY_LEFTSHIFT,
            Key::Super => KeyCode::KEY_LEFTMETA,
            Key::Enter => KeyCode::KEY_ENTER,
            Key::Escape => KeyCode::KEY_ESC,
            Key::Tab => KeyCode::KEY_TAB,
            Key::Space => KeyCode::KEY_SPACE,
            Key::Backspace => KeyCode::KEY_BACKSPACE,
            Key::Delete => KeyCode::KEY_DELETE,
            Key::Up => KeyCode::KEY_UP,
            Key::Down => KeyCode::KEY_DOWN,
            Key::Left => KeyCode::KEY_LEFT,
            Key::Right => KeyCode::KEY_RIGHT,
            Key::Home => KeyCode::KEY_HOME,
            Key::End => KeyCode::KEY_END,
            Key::PageUp => KeyCode::KEY_PAGEUP,
            Key::PageDown => KeyCode::KEY_PAGEDOWN,
            Key::Insert => KeyCode::KEY_INSERT,
            Key::F(n) => match n {
                1 => KeyCode::KEY_F1,
                2 => KeyCode::KEY_F2,
                3 => KeyCode::KEY_F3,
                4 => KeyCode::KEY_F4,
                5 => KeyCode::KEY_F5,
                6 => KeyCode::KEY_F6,
                7 => KeyCode::KEY_F7,
                8 => KeyCode::KEY_F8,
                9 => KeyCode::KEY_F9,
                10 => KeyCode::KEY_F10,
                11 => KeyCode::KEY_F11,
                12 => KeyCode::KEY_F12,
                _ => KeyCode::KEY_RESERVED,
            },
            Key::PrintScreen => KeyCode::KEY_SYSRQ,
            Key::ScrollLock => KeyCode::KEY_SCROLLLOCK,
            Key::Pause => KeyCode::KEY_PAUSE,
            Key::CapsLock => KeyCode::KEY_CAPSLOCK,
            Key::NumLock => KeyCode::KEY_NUMLOCK,
            Key::Menu => KeyCode::KEY_MENU,
            Key::Char(c) => char_evdev(c),
        }
    }
}

/// US-layout base character for `c` (the key that produces `c`, ignoring
/// Shift). Returns `None` for characters with no US-layout key.
fn unshift_base(c: char) -> Option<char> {
    Some(match c {
        'a'..='z' | '0'..='9' => c,
        'A'..='Z' => c.to_ascii_lowercase(),
        // Unshifted punctuation
        ';' | '[' | ']' | '\\' | '\'' | '/' | '-' | '=' | ',' | '.' | '`' => c,
        // Shifted digit symbols (US layout)
        '!' => '1',
        '@' => '2',
        '#' => '3',
        '$' => '4',
        '%' => '5',
        '^' => '6',
        '&' => '7',
        '*' => '8',
        '(' => '9',
        ')' => '0',
        // Shifted punctuation
        '{' => '[',
        '}' => ']',
        '|' => '\\',
        ':' => ';',
        '"' => '\'',
        '<' => ',',
        '>' => '.',
        '?' => '/',
        '_' => '-',
        '+' => '=',
        '~' => '`',
        _ => return None,
    })
}

/// VK code for a character key, mirroring the legacy `VKey` OEM table.
fn char_win_vk(c: char) -> (u16, Option<char>) {
    if c == ' ' {
        return (0x20, None); // VK_SPACE
    }
    let Some(base) = unshift_base(c) else {
        return (0, None);
    };
    let vk = match base {
        'a'..='z' => (base as u16) - 0x20, // 'A'..='Z' VK codes
        '0'..='9' => base as u16,
        ';' => 0xBA,  // VK_OEM_1
        '[' => 0xDB,  // VK_OEM_4
        ']' => 0xDD,  // VK_OEM_6
        '\\' => 0xDC, // VK_OEM_5
        '\'' => 0xDE, // VK_OEM_7
        '/' => 0xBF,  // VK_OEM_2
        '-' => 0xBD,  // VK_OEM_MINUS
        '=' => 0xBB,  // VK_OEM_PLUS (unshifted =)
        ',' => 0xBC,  // VK_OEM_COMMA
        '.' => 0xBE,  // VK_OEM_PERIOD
        '`' => 0xC0,  // VK_OEM_3
        _ => return (0, None),
    };
    let shift = if Key::Char(c).needs_shift() {
        Some(base)
    } else {
        None
    };
    (vk, shift)
}

/// evdev code for a character key (US layout).
#[cfg(target_os = "linux")]
fn char_evdev(c: char) -> KeyCode {
    let base = match c {
        ' ' => return KeyCode::KEY_SPACE,
        _ => match unshift_base(c) {
            Some(b) => b,
            None => return KeyCode::KEY_RESERVED,
        },
    };
    match base {
        'a' => KeyCode::KEY_A,
        'b' => KeyCode::KEY_B,
        'c' => KeyCode::KEY_C,
        'd' => KeyCode::KEY_D,
        'e' => KeyCode::KEY_E,
        'f' => KeyCode::KEY_F,
        'g' => KeyCode::KEY_G,
        'h' => KeyCode::KEY_H,
        'i' => KeyCode::KEY_I,
        'j' => KeyCode::KEY_J,
        'k' => KeyCode::KEY_K,
        'l' => KeyCode::KEY_L,
        'm' => KeyCode::KEY_M,
        'n' => KeyCode::KEY_N,
        'o' => KeyCode::KEY_O,
        'p' => KeyCode::KEY_P,
        'q' => KeyCode::KEY_Q,
        'r' => KeyCode::KEY_R,
        's' => KeyCode::KEY_S,
        't' => KeyCode::KEY_T,
        'u' => KeyCode::KEY_U,
        'v' => KeyCode::KEY_V,
        'w' => KeyCode::KEY_W,
        'x' => KeyCode::KEY_X,
        'y' => KeyCode::KEY_Y,
        'z' => KeyCode::KEY_Z,
        '1' => KeyCode::KEY_1,
        '2' => KeyCode::KEY_2,
        '3' => KeyCode::KEY_3,
        '4' => KeyCode::KEY_4,
        '5' => KeyCode::KEY_5,
        '6' => KeyCode::KEY_6,
        '7' => KeyCode::KEY_7,
        '8' => KeyCode::KEY_8,
        '9' => KeyCode::KEY_9,
        '0' => KeyCode::KEY_0,
        ';' => KeyCode::KEY_SEMICOLON,
        '[' => KeyCode::KEY_LEFTBRACE,
        ']' => KeyCode::KEY_RIGHTBRACE,
        '\\' => KeyCode::KEY_BACKSLASH,
        '\'' => KeyCode::KEY_APOSTROPHE,
        '/' => KeyCode::KEY_SLASH,
        '-' => KeyCode::KEY_MINUS,
        '=' => KeyCode::KEY_EQUAL,
        ',' => KeyCode::KEY_COMMA,
        '.' => KeyCode::KEY_DOT,
        '`' => KeyCode::KEY_GRAVE,
        _ => KeyCode::KEY_RESERVED,
    }
}

/// Case-insensitive key-name lookup (superset of legacy `VKey::from_name`:
/// adds navigation/system keys). Single characters resolve to their
/// unshifted base (e.g. `";"` → `Char(';')`), exactly like the legacy table.
fn key_from_name(s: &str) -> Option<Key> {
    let lower = s.to_ascii_lowercase();
    Some(match lower.as_str() {
        "return" | "enter" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "insert" | "ins" => Key::Insert,
        "alt" => Key::Alt,
        "shift" => Key::Shift,
        "ctrl" | "control" => Key::Ctrl,
        "win" | "windows" | "super" | "meta" => Key::Super,
        "printscreen" | "prtsc" => Key::PrintScreen,
        "scrolllock" => Key::ScrollLock,
        "pause" => Key::Pause,
        "capslock" => Key::CapsLock,
        "numlock" => Key::NumLock,
        "menu" => Key::Menu,
        "semicolon" => Key::Char(';'),
        "leftbracket" => Key::Char('['),
        "rightbracket" => Key::Char(']'),
        "backslash" => Key::Char('\\'),
        "quote" => Key::Char('\''),
        "slash" => Key::Char('/'),
        "minus" => Key::Char('-'),
        "equals" => Key::Char('='),
        "comma" => Key::Char(','),
        "period" => Key::Char('.'),
        "backtick" => Key::Char('`'),
        fkey if fkey.len() >= 2
            && fkey.starts_with('f')
            && fkey[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            let n: u8 = fkey[1..].parse().ok()?;
            if (1..=12).contains(&n) {
                Key::F(n)
            } else {
                return None;
            }
        }
        single if single.chars().count() == 1 => {
            let c = single.chars().next().unwrap();
            match c {
                'a'..='z' | '0'..='9' => Key::Char(c),
                ';' | '[' | ']' | '\\' | '\'' | '/' | '-' | '=' | ',' | '.' | '`' => Key::Char(c),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// Single character → combo, expanding Shift-required symbols
/// (exact legacy `symbol_to_vkeys` table).
fn symbol_to_keys(c: char) -> Option<Vec<Key>> {
    let shifted = |base: char| vec![Key::Shift, Key::Char(base)];
    Some(match c {
        'a'..='z' | '0'..='9' => vec![Key::Char(c)],
        'A'..='Z' => vec![Key::Shift, Key::Char(c.to_ascii_lowercase())],
        '!' => shifted('1'),
        '@' => shifted('2'),
        '#' => shifted('3'),
        '$' => shifted('4'),
        '%' => shifted('5'),
        '^' => shifted('6'),
        '&' => shifted('7'),
        '*' => shifted('8'),
        '(' => shifted('9'),
        ')' => shifted('0'),
        '[' | ']' | '\\' | ';' | '\'' | ',' | '.' | '/' | '-' | '=' | '`' => vec![Key::Char(c)],
        ' ' => vec![Key::Space],
        '{' => shifted('['),
        '}' => shifted(']'),
        '|' => shifted('\\'),
        ':' => shifted(';'),
        '"' => shifted('\''),
        '<' => shifted(','),
        '>' => shifted('.'),
        '?' => shifted('/'),
        '_' => shifted('-'),
        '+' => shifted('='),
        '~' => shifted('`'),
        _ => return None,
    })
}

/// tmux `C-`/`M-C-` rest: legacy accepts only a single letter or digit
/// (letters lowercased, no Shift).
fn tmux_ctrl_char(s: &str) -> Option<Key> {
    if s.chars().count() != 1 {
        return None;
    }
    let c = s.chars().next().unwrap();
    match c {
        'a'..='z' | 'A'..='Z' => Some(Key::Char(c.to_ascii_lowercase())),
        '0'..='9' => Some(Key::Char(c)),
        _ => None,
    }
}

/// tmux `M-`/`S-` rest: named key (legacy case-sensitive tmux names,
/// extended via the shared name table) or a single character.
fn tmux_modified_key(s: &str) -> Option<Vec<Key>> {
    if s.chars().count() == 1 {
        return symbol_to_keys(s.chars().next().unwrap());
    }
    key_from_name(s).map(|k| vec![k])
}

/// Parse a key-combo string into an ordered list of keys (modifiers first,
/// main key last), or `None` if any part is unrecognized.
///
/// Accepted forms (legacy-compatible):
/// - `'+'`-separated names, case-insensitive: `"ctrl+shift+b"`, `"Shift+7"`,
///   `"Ctrl+B"`, `"alt+f4"`.
/// - tmux notation: `"C-b"`, `"M-x"`, `"M-C-a"`, `"S-Left"`, `"M-Up"`,
///   `"M-S-7"`, escaped `"\\;"`.
/// - Named keys: `"enter"`, `"Space"`, `"Tab"`, `"f12"`, `"pageup"`, ...
/// - A single character, with Shift expansion for uppercase letters and
///   shifted symbols: `"p"`, `"A"`, `"&"` → `[Shift, Char('7')]`.
pub fn parse_combo(s: &str) -> Option<Vec<Key>> {
    // No whole-string trim (legacy parsers don't trim either; '+'-separated
    // parts are trimmed individually). A bare " " is the Space key.
    let s = s;
    if s.is_empty() {
        return None;
    }
    // tmux escape prefix: "\;" → ";". A lone "\" stays a backslash key.
    let s = if s.len() > 1 {
        s.strip_prefix('\\').unwrap_or(s)
    } else {
        s
    };
    if s.is_empty() {
        return None;
    }

    // '+'-separated notation ("ctrl+shift+b"). A lone "+" is the plus symbol.
    if s.contains('+') && s != "+" {
        return s
            .split('+')
            .map(|part| key_from_name(part.trim()))
            .collect();
    }

    // tmux modifier prefixes.
    if let Some(rest) = s.strip_prefix("C-") {
        let key = tmux_ctrl_char(rest)?;
        return Some(vec![Key::Ctrl, key]);
    }
    if let Some(rest) = s.strip_prefix("M-C-") {
        let key = tmux_ctrl_char(rest)?;
        return Some(vec![Key::Alt, Key::Ctrl, key]);
    }
    if let Some(rest) = s.strip_prefix("M-S-") {
        // Extension beyond the legacy parser: tmux emits M-S- for Alt+Shift.
        let mut keys = vec![Key::Alt, Key::Shift];
        keys.extend(tmux_modified_key(rest)?);
        return Some(keys);
    }
    if let Some(rest) = s.strip_prefix("M-") {
        let mut keys = vec![Key::Alt];
        keys.extend(tmux_modified_key(rest)?);
        return Some(keys);
    }
    if let Some(rest) = s.strip_prefix("S-") {
        let mut keys = vec![Key::Shift];
        keys.extend(tmux_modified_key(rest)?);
        return Some(keys);
    }

    // Single character → combo (may include Shift for symbols/uppercase).
    if s.chars().count() == 1 {
        return symbol_to_keys(s.chars().next().unwrap());
    }

    // Multi-character key name ("enter", "F12", "pageup", ...).
    key_from_name(s).map(|k| vec![k])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<Key> {
        parse_combo(s).unwrap_or_else(|| panic!("expected {s:?} to parse"))
    }

    // ── '+' notation (legacy mapper::parse_key_combo semantics) ────────

    #[test]
    fn plus_ctrl_b_case_insensitive() {
        assert_eq!(parse("Ctrl+B"), vec![Key::Ctrl, Key::Char('b')]);
        assert_eq!(parse("ctrl+b"), vec![Key::Ctrl, Key::Char('b')]);
        assert_eq!(
            parse("ctrl+shift+b"),
            vec![Key::Ctrl, Key::Shift, Key::Char('b')]
        );
    }

    #[test]
    fn plus_shift_7() {
        // "Shift+7" stays an explicit Shift + unshifted digit (legacy [Shift, D7]).
        assert_eq!(parse("Shift+7"), vec![Key::Shift, Key::Char('7')]);
    }

    #[test]
    fn plus_single_letters_and_names() {
        assert_eq!(parse("p"), vec![Key::Char('p')]);
        assert_eq!(parse("enter"), vec![Key::Enter]);
        assert_eq!(parse("escape"), vec![Key::Escape]);
        assert_eq!(parse("esc"), vec![Key::Escape]);
        assert_eq!(parse("tab"), vec![Key::Tab]);
        assert_eq!(parse("space"), vec![Key::Space]);
        assert_eq!(parse("up"), vec![Key::Up]);
        assert_eq!(parse("f1"), vec![Key::F(1)]);
        assert_eq!(parse("F12"), vec![Key::F(12)]);
        assert_eq!(parse("pageup"), vec![Key::PageUp]);
        assert_eq!(parse("pagedown"), vec![Key::PageDown]);
        assert_eq!(parse("ctrl+g"), vec![Key::Ctrl, Key::Char('g')]);
        assert_eq!(parse("ctrl+u"), vec![Key::Ctrl, Key::Char('u')]);
    }

    #[test]
    fn plus_punctuation_parts() {
        assert_eq!(parse("ctrl+;"), vec![Key::Ctrl, Key::Char(';')]);
        assert_eq!(parse("ctrl+\\"), vec![Key::Ctrl, Key::Char('\\')]);
        assert_eq!(parse("semicolon"), vec![Key::Char(';')]);
        assert_eq!(parse("backtick"), vec![Key::Char('`')]);
    }

    #[test]
    fn plus_invalid_rejected() {
        assert_eq!(parse_combo(""), None);
        assert_eq!(parse_combo("ctrl+"), None);
        assert_eq!(parse_combo("ctrl+nosuchkey"), None);
        assert_eq!(parse_combo("f13"), None);
        assert_eq!(parse_combo("f0"), None);
    }

    // ── tmux notation (legacy tmux_detect::parse_tmux_key semantics) ───

    #[test]
    fn tmux_ctrl_prefix() {
        assert_eq!(parse("C-b"), vec![Key::Ctrl, Key::Char('b')]);
        assert_eq!(parse("C-a"), vec![Key::Ctrl, Key::Char('a')]);
    }

    #[test]
    fn tmux_alt_prefixes() {
        assert_eq!(parse("M-n"), vec![Key::Alt, Key::Char('n')]);
        assert_eq!(parse("M-C-a"), vec![Key::Alt, Key::Ctrl, Key::Char('a')]);
        assert_eq!(parse("M-Up"), vec![Key::Alt, Key::Up]);
        assert_eq!(parse("M-S-7"), vec![Key::Alt, Key::Shift, Key::Char('7')]);
    }

    #[test]
    fn tmux_shift_prefix_named() {
        assert_eq!(parse("S-Left"), vec![Key::Shift, Key::Left]);
    }

    #[test]
    fn tmux_named_keys() {
        assert_eq!(parse("Space"), vec![Key::Space]);
        assert_eq!(parse("Enter"), vec![Key::Enter]);
        assert_eq!(parse("Escape"), vec![Key::Escape]);
        assert_eq!(parse("Tab"), vec![Key::Tab]);
        assert_eq!(parse("Down"), vec![Key::Down]);
        assert_eq!(parse("Right"), vec![Key::Right]);
    }

    #[test]
    fn tmux_escape_prefix() {
        assert_eq!(parse("\\;"), vec![Key::Char(';')]);
        assert_eq!(parse("\\#"), vec![Key::Shift, Key::Char('3')]);
    }

    #[test]
    fn tmux_single_symbols_expand_shift() {
        // Exact legacy symbol table (kill-window = "&" → [Shift, D7]).
        assert_eq!(parse("&"), vec![Key::Shift, Key::Char('7')]);
        assert_eq!(parse("%"), vec![Key::Shift, Key::Char('5')]);
        assert_eq!(parse("\""), vec![Key::Shift, Key::Char('\'')]);
        assert_eq!(parse("~"), vec![Key::Shift, Key::Char('`')]);
        assert_eq!(parse("{"), vec![Key::Shift, Key::Char('[')]);
        assert_eq!(parse("|"), vec![Key::Shift, Key::Char('\\')]);
        assert_eq!(parse("_"), vec![Key::Shift, Key::Char('-')]);
        assert_eq!(parse("+"), vec![Key::Shift, Key::Char('=')]);
        assert_eq!(parse("!"), vec![Key::Shift, Key::Char('1')]);
        assert_eq!(parse(")"), vec![Key::Shift, Key::Char('0')]);
    }

    #[test]
    fn tmux_single_chars() {
        assert_eq!(parse("c"), vec![Key::Char('c')]);
        assert_eq!(parse("7"), vec![Key::Char('7')]);
        assert_eq!(parse("A"), vec![Key::Shift, Key::Char('a')]);
        assert_eq!(parse("["), vec![Key::Char('[')]);
        assert_eq!(parse("\\"), vec![Key::Char('\\')]);
        assert_eq!(parse(" "), vec![Key::Space]);
    }

    // ── lowering ───────────────────────────────────────────────────────

    #[test]
    fn win_vk_matches_legacy_codes() {
        assert_eq!(Key::Enter.to_win_vk(), (0x0D, None)); // VK_RETURN
        assert_eq!(Key::Escape.to_win_vk(), (0x1B, None));
        assert_eq!(Key::Tab.to_win_vk(), (0x09, None));
        assert_eq!(Key::Up.to_win_vk(), (0x26, None));
        assert_eq!(Key::Down.to_win_vk(), (0x28, None));
        assert_eq!(Key::Left.to_win_vk(), (0x25, None));
        assert_eq!(Key::Right.to_win_vk(), (0x27, None));
        assert_eq!(Key::Alt.to_win_vk(), (0x12, None)); // VK_MENU
        assert_eq!(Key::Shift.to_win_vk(), (0x10, None));
        assert_eq!(Key::Ctrl.to_win_vk(), (0x11, None)); // VK_CONTROL
        assert_eq!(Key::Super.to_win_vk(), (0x5B, None)); // VK_LWIN
        assert_eq!(Key::Space.to_win_vk(), (0x20, None));
        assert_eq!(Key::Char('b').to_win_vk(), (0x42, None));
        assert_eq!(Key::Char('7').to_win_vk(), (0x37, None));
        assert_eq!(Key::Char(';').to_win_vk(), (0xBA, None)); // VK_OEM_1
        assert_eq!(Key::Char('[').to_win_vk(), (0xDB, None)); // VK_OEM_4
        assert_eq!(Key::Char(']').to_win_vk(), (0xDD, None)); // VK_OEM_6
        assert_eq!(Key::Char('\\').to_win_vk(), (0xDC, None)); // VK_OEM_5
        assert_eq!(Key::Char('\'').to_win_vk(), (0xDE, None)); // VK_OEM_7
        assert_eq!(Key::Char('/').to_win_vk(), (0xBF, None)); // VK_OEM_2
        assert_eq!(Key::Char('-').to_win_vk(), (0xBD, None)); // VK_OEM_MINUS
        assert_eq!(Key::Char('=').to_win_vk(), (0xBB, None)); // VK_OEM_PLUS
        assert_eq!(Key::Char(',').to_win_vk(), (0xBC, None));
        assert_eq!(Key::Char('.').to_win_vk(), (0xBE, None));
        assert_eq!(Key::Char('`').to_win_vk(), (0xC0, None));
        assert_eq!(Key::F(1).to_win_vk(), (0x70, None));
        assert_eq!(Key::F(12).to_win_vk(), (0x7B, None));
        assert_eq!(Key::F(13).to_win_vk(), (0, None));
    }

    #[test]
    fn win_vk_shifted_chars_report_shift_base() {
        assert_eq!(Key::Char('&').to_win_vk(), (0x37, Some('7')));
        assert_eq!(Key::Char('!').to_win_vk(), (0x31, Some('1')));
        assert_eq!(Key::Char('B').to_win_vk(), (0x42, Some('b')));
        assert_eq!(Key::Char('"').to_win_vk(), (0xDE, Some('\'')));
        assert_eq!(Key::Char('+').to_win_vk(), (0xBB, Some('=')));
    }

    #[test]
    fn needs_shift_table() {
        assert!(Key::Char('&').needs_shift());
        assert!(Key::Char('A').needs_shift());
        assert!(Key::Char('~').needs_shift());
        assert!(!Key::Char('a').needs_shift());
        assert!(!Key::Char('7').needs_shift());
        assert!(!Key::Char(';').needs_shift());
        assert!(!Key::Shift.needs_shift());
        assert!(!Key::Enter.needs_shift());
    }

    #[test]
    fn unmappable_chars_lower_to_zero() {
        assert_eq!(Key::Char('🚀').to_win_vk(), (0, None));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn evdev_lowering_us_layout() {
        use evdev::KeyCode;
        assert_eq!(Key::Ctrl.to_evdev(), KeyCode::KEY_LEFTCTRL);
        assert_eq!(Key::Enter.to_evdev(), KeyCode::KEY_ENTER);
        assert_eq!(Key::Escape.to_evdev(), KeyCode::KEY_ESC);
        assert_eq!(Key::Char('a').to_evdev(), KeyCode::KEY_A);
        assert_eq!(Key::Char('A').to_evdev(), KeyCode::KEY_A);
        assert_eq!(Key::Char('7').to_evdev(), KeyCode::KEY_7);
        assert_eq!(Key::Char('&').to_evdev(), KeyCode::KEY_7);
        assert_eq!(Key::Char(';').to_evdev(), KeyCode::KEY_SEMICOLON);
        assert_eq!(Key::Char('[').to_evdev(), KeyCode::KEY_LEFTBRACE);
        assert_eq!(Key::Char('`').to_evdev(), KeyCode::KEY_GRAVE);
        assert_eq!(Key::F(5).to_evdev(), KeyCode::KEY_F5);
        assert_eq!(Key::PrintScreen.to_evdev(), KeyCode::KEY_SYSRQ);
        assert_eq!(Key::Char('🚀').to_evdev(), KeyCode::KEY_RESERVED);
    }
}
