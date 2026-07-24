//! Platform abstraction layer.
//!
//! The OS-neutral pieces (the [`Injector`] trait and [`InjectError`]) live
//! here; per-OS implementations are re-exported behind `cfg`:
//!
//! - Windows: [`win_impl`] — thin adapters over the existing registry /
//!   Core Audio / `%APPDATA%` code (behavior unchanged). The Windows
//!   `Injector` implementation (`WinInjector`) is owned by `mapper.rs`.
//! - Linux: [`linux`] — uinput/evdev injection, pactl/wpctl mic, systemd
//!   user + XDG autostart, XDG paths.
//!
//! Unicode *text* injection (launcher actions) does NOT go through
//! `Injector`; it stays in `mapper.rs` (SendInput on Windows, wtype/xdotool
//! on Linux).

use crate::keys::Key;
use std::fmt;

#[cfg(windows)]
mod win_impl;
#[cfg(windows)]
pub use win_impl::*;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

/// Synthetic input device (keyboard + mouse) used by the mapper to emit
/// resolved shortcut actions.
///
/// Combo semantics (both OSes, matching the legacy SendInput ordering):
/// `combo(&[Ctrl, Shift, B])` presses keys in order, then releases them in
/// reverse order. `key_down`/`key_up` hold keys without auto-release (d-pad
/// hold-repeat). `wheel(vertical, horizontal)` takes wheel-delta units
/// (positive = up/right), the same convention the mapper uses for Windows
/// SendInput; per-OS implementations convert units/sign internally.
pub trait Injector: Send {
    /// Press keys in order, release in reverse (modifiers held around the
    /// main key).
    fn combo(&mut self, keys: &[Key]);
    /// Hold a key down (no auto-release).
    fn key_down(&mut self, k: Key);
    /// Release a held key.
    fn key_up(&mut self, k: Key);
    /// Relative mouse movement in pixels.
    fn mouse_rel(&mut self, dx: i32, dy: i32);
    /// Scroll, in Windows wheel-delta units (120 = one notch;
    /// positive = up/right).
    fn wheel(&mut self, vertical: i32, horizontal: i32);
    /// Left mouse button press + release.
    fn click(&mut self);
}

/// Failure to create an [`Injector`]. Recoverable: the daemon keeps running
/// with injection disabled (feature-degraded, not fatal).
#[derive(Debug)]
pub enum InjectError {
    /// /dev/uinput is missing or not writable. The message carries the exact
    /// remediation steps for the user.
    UinputUnavailable(String),
    /// Any other platform-specific failure.
    Platform(String),
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InjectError::UinputUnavailable(msg) => write!(f, "uinput unavailable: {msg}"),
            InjectError::Platform(msg) => write!(f, "injector error: {msg}"),
        }
    }
}

impl std::error::Error for InjectError {}
