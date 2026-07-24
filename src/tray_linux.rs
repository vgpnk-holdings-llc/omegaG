//! Linux system tray via `ksni` (StatusNotifierItem / AppIndicator), blocking API.
//!
//! Included from `tray.rs` as `#[path = "tray_linux.rs"] mod linux;` — there is
//! deliberately no `mod tray_linux;` in `main.rs`, so `tray.rs` stays the single
//! platform-dispatch point and `main.rs` never cfg-gates module declarations.
//!
//! Menu parity with the Windows tray:
//!
//! | Windows item                        | Linux item                                              |
//! |-------------------------------------|---------------------------------------------------------|
//! | Open Wispr Flow                     | "Open voice app" — only when `[voice] app_command` set  |
//! | Restart                             | Restart (re-exec `current_exe`, exec(2) replace)        |
//! | Check for Update                    | same (update.rs Linux flow)                             |
//! | Enable auto start-up  [checkmark]   | same, via `platform::autostart`                         |
//! | Mouse: Left Stick     [checkmark]   | same, shared `Arc<AtomicBool>` with the mapper          |
//! | Show Log Window       [checkmark]   | "Open log file" (xdg-open `log_dir()/ds4cc.log`)        |
//! | Exit                                | Exit                                                    |

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, mpsc};

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};

use super::TrayCmd;

/// Tray thread body (called from `tray::run`).
///
/// Never panics: when the D-Bus session bus or a StatusNotifierWatcher is
/// unavailable, `ksni` returns an error — we log and exit the thread, so the
/// daemon keeps running trayless (`main` holds an `Option<Sender<TrayCmd>>`
/// and simply stops receiving menu updates).
pub(super) fn run(rx: mpsc::Receiver<TrayCmd>, mouse_stick_active: Arc<AtomicBool>) {
    let tray = Ds4ccTray::new(mouse_stick_active);
    let handle = match tray.spawn() {
        Ok(handle) => handle,
        Err(e) => {
            log::error!(
                "Failed to create tray icon ({e}). Running without a tray; \
                 on GNOME install the AppIndicator extension."
            );
            return;
        }
    };
    let autostart = handle
        .update(|t: &mut Ds4ccTray| t.autostart_enabled)
        .unwrap_or_default();
    log::info!("Tray icon created (auto-start: {autostart})");

    // ksni runs its own service thread; this loop only ferries runtime
    // commands (stick-mode changes from controller connect) into the tray.
    loop {
        match rx.try_recv() {
            Ok(TrayCmd::SetStickMode(stick)) => {
                handle.update(move |t: &mut Ds4ccTray| t.set_stick_mode(stick));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                handle.shutdown();
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if handle.is_closed() {
            log::warn!("Tray service closed unexpectedly; menu updates disabled");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Returns true when a D-Bus session bus looks reachable.
///
/// Cheap pre-flight so `main` can skip the tray (set its sender to `None`)
/// on headless sessions instead of relying on the in-thread fallback.
// Re-exported as `tray::session_bus_available` for main.rs (C2).
#[allow(dead_code)]
pub(crate) fn dbus_session_available() -> bool {
    if let Ok(bus) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        if let Some(path) = bus.strip_prefix("unix:path=") {
            return std::path::Path::new(path).exists();
        }
        // abstract socket or other transport — assume reachable
        return true;
    }
    // Fall back to the default per-user socket location.
    std::env::var("XDG_RUNTIME_DIR")
        .map(|dir| std::path::Path::new(&dir).join("bus").exists())
        .unwrap_or(false)
}

/// Show a desktop notification (used by the self-update flow).
///
/// Uses `notify-send` when available; always logs as a fallback so the
/// message is never lost on minimal/headless systems.
pub(crate) fn notify(summary: &str, body: &str) {
    log::info!("[{summary}] {body}");
    if let Err(e) = std::process::Command::new("notify-send")
        .arg(summary)
        .arg(body)
        .spawn()
    {
        log::debug!("notify-send unavailable ({e}); notification logged only");
    }
}

struct Ds4ccTray {
    /// Shared with the mapper: true = left stick drives the cursor,
    /// false = touchpad. Toggled from the menu and from `TrayCmd`.
    mouse_stick_active: Arc<AtomicBool>,
    /// Mirrored stick-mode state for the menu checkmark.
    stick_mode: bool,
    /// Mirrored autostart state for the menu checkmark.
    autostart_enabled: bool,
    /// `[voice] app_command` from config; `None` hides "Open voice app".
    voice_app_command: Option<String>,
}

impl Ds4ccTray {
    fn new(mouse_stick_active: Arc<AtomicBool>) -> Self {
        Self {
            stick_mode: mouse_stick_active.load(Ordering::Relaxed),
            mouse_stick_active,
            autostart_enabled: crate::platform::autostart_is_enabled(),
            voice_app_command: voice_app_command(),
        }
    }

    fn set_stick_mode(&mut self, stick: bool) {
        self.stick_mode = stick;
        self.mouse_stick_active.store(stick, Ordering::Relaxed);
        let mode = if stick { "left stick" } else { "touchpad" };
        log::info!("Mouse cursor mode: {mode}");
    }

    fn toggle_autostart(&mut self) {
        let new = !self.autostart_enabled;
        match crate::platform::autostart_set(new) {
            Ok(()) => {
                self.autostart_enabled = new;
                log::info!("Auto-start {}", if new { "enabled" } else { "disabled" });
            }
            Err(e) => {
                log::error!("Failed to set auto-start: {e}");
                // Re-read actual state so the checkmark stays truthful.
                self.autostart_enabled = crate::platform::autostart_is_enabled();
            }
        }
    }
}

/// Read `[voice] app_command` from the config; empty/missing → `None`.
fn voice_app_command() -> Option<String> {
    let cfg = crate::config::Config::load();
    let cmd = cfg.voice.app_command.trim();
    (!cmd.is_empty()).then(|| cmd.to_string())
}

/// Restart in place: re-exec the current binary (exec(2) replaces the
/// process image, so all threads — HID loop, tokio runtime, tray — are
/// replaced by a fresh instance with the same PID and CLI arguments).
fn restart_app() {
    use std::os::unix::process::CommandExt;

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            log::error!("Cannot determine exe path to restart: {e}");
            return;
        }
    };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    log::info!("Restarting via exec: {}", exe.display());
    // Only returns on failure.
    let err = std::process::Command::new(&exe).args(&args).exec();
    log::error!("Failed to re-exec {}: {err}", exe.display());
}

/// Launch the configured voice app via `launcher::launch_voice_app`
/// (argv split, no shell, spawn failures logged and non-fatal).
/// Surfaces a desktop notification on failure so the click isn't silent.
fn open_voice_app(cmd: &str) {
    if !crate::launcher::launch_voice_app(cmd) {
        notify(
            "DS4CC",
            "Could not launch the configured voice app — see log.",
        );
    }
}

/// Open the daemon log file in the default desktop app.
///
/// Linux logs go to a file (`log_dir()/ds4cc.log`) instead of a hidden
/// console window, so this replaces the Windows "Show Log Window" toggle.
fn open_log_file() {
    let dir = crate::platform::log_dir();
    let path = dir.join("ds4cc.log");
    if !path.exists() {
        // Create an empty log so xdg-open never fails on a missing file.
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("Cannot create log dir {}: {e}", dir.display());
        }
        if let Err(e) = std::fs::File::create(&path) {
            log::error!("Cannot create log file {}: {e}", path.display());
        }
    }
    match std::process::Command::new("xdg-open").arg(&path).spawn() {
        Ok(_) => log::info!("Opened log file: {}", path.display()),
        Err(e) => log::error!("Failed to xdg-open {}: {e}", path.display()),
    }
}

impl ksni::Tray for Ds4ccTray {
    // Left click opens the menu (no window to raise on Linux).
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "ds4cc".into()
    }

    fn title(&self) -> String {
        "DS4CC".into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "DS4CC".into(),
            description: "DualSense/DS4 shortcut mapper".into(),
            ..Default::default()
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        tray_icon().clone()
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        // Keep the service alive: the watcher may appear later (e.g. the DE
        // finished starting after us). `assume_sni_available` is not set, so
        // this only fires after a successful registration.
        log::warn!("StatusNotifierWatcher offline ({reason:?}); keeping tray service alive");
        true
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        // Open voice app — only when `[voice] app_command` is configured.
        if let Some(cmd) = self.voice_app_command.clone() {
            items.push(
                StandardItem {
                    label: "Open voice app".into(),
                    activate: Box::new(move |_: &mut Self| open_voice_app(&cmd)),
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(
            StandardItem {
                label: "Restart".into(),
                activate: Box::new(|_: &mut Self| restart_app()),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            StandardItem {
                label: "Check for Update".into(),
                activate: Box::new(|_: &mut Self| {
                    std::thread::spawn(crate::update::check_for_update);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            CheckmarkItem {
                label: "Enable auto start-up".into(),
                checked: self.autostart_enabled,
                activate: Box::new(|this: &mut Self| this.toggle_autostart()),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            CheckmarkItem {
                label: "Mouse: Left Stick".into(),
                checked: self.stick_mode,
                activate: Box::new(|this: &mut Self| {
                    let stick = !this.stick_mode;
                    this.set_stick_mode(stick);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            StandardItem {
                label: "Open log file".into(),
                activate: Box::new(|_: &mut Self| open_log_file()),
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        items.push(
            StandardItem {
                label: "Exit".into(),
                activate: Box::new(|_: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

// ── Icon ────────────────────────────────────────────────────────────────

/// Source icon shipped in the repo (`assets/icon.ico`, 256×256 RGBA). The
/// `image` crate decodes ICO (feature `ico`); we downscale to 64×64 for the
/// StatusNotifierItem pixmap.
const ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");

/// Decode the embedded ICO once, resize to 64×64, and convert RGBA →
/// premultiplied ARGB32 (network byte order: A, R, G, B per pixel) as
/// required by `ksni::Icon`.
fn tray_icon() -> &'static Vec<ksni::Icon> {
    static ICON: LazyLock<Vec<ksni::Icon>> = LazyLock::new(|| {
        let decoded = image::load_from_memory_with_format(ICON_ICO, image::ImageFormat::Ico)
            .expect("embedded assets/icon.ico is valid");
        let img = image::imageops::resize(
            &decoded.to_rgba8(),
            64,
            64,
            image::imageops::FilterType::Lanczos3,
        );
        let (width, height) = img.dimensions();
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for pixel in img.pixels() {
            let [r, g, b, a] = pixel.0;
            data.push(a);
            data.push(premultiply(r, a));
            data.push(premultiply(g, a));
            data.push(premultiply(b, a));
        }
        vec![ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        }]
    });
    &ICON
}

/// Premultiply one color channel by alpha, with rounding.
fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes_to_premultiplied_argb() {
        let icons = tray_icon();
        assert_eq!(icons.len(), 1);
        let icon = &icons[0];
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.data.len(), (64 * 64 * 4) as usize);
        // Opaque pixels must be untouched by premultiplication (a == 255).
        for px in icon.data.chunks_exact(4) {
            let (a, r, g, b) = (px[0], px[1], px[2], px[3]);
            assert!(r <= a && g <= a && b <= a, "channels must be <= alpha");
        }
    }

    #[test]
    fn premultiply_rounds_correctly() {
        assert_eq!(premultiply(255, 255), 255);
        assert_eq!(premultiply(255, 0), 0);
        assert_eq!(premultiply(128, 128), 64);
        assert_eq!(premultiply(200, 100), 78);
    }

    #[test]
    fn dbus_check_never_panics() {
        let _ = dbus_session_available();
    }
}
