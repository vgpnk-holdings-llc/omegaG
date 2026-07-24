//! Linux platform implementations: uinput injection, pactl/wpctl mic,
//! systemd-user/XDG autostart, XDG paths.

pub mod autostart;
pub mod inject;
pub mod mic;
pub mod paths;

pub use autostart::*;
pub use inject::*;
pub use mic::*;
pub use paths::*;
