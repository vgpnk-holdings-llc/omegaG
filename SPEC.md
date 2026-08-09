# SPEC.md — omegaG Linux Port (single source of truth)

Target: Linux (Ubuntu 22.04+, Arch) — DualSense/DS4 over USB **and** Bluetooth.
Preserve ALL legacy shortcut-mapper features. DITCH the codex session-polling + status→LED
layer on Linux (stays Windows-only, untouched). Windows build must keep compiling byte-for-byte
in behavior (`cargo check --target x86_64-pc-windows-gnu` is the gate).

Golden rule: **minimal diff for Windows paths.** All existing Windows code stays as-is behind
`#[cfg(windows)]`. Linux code is NEW code behind `#[cfg(target_os = "linux")]`. Pure/platform-neutral
modules (tmux_detect.rs parsers, input.rs report parsing, output.rs HID report builders, crc32.rs,
controller.rs, config.rs schema, mapper.rs mapping logic) are shared, edited only to abstract
OS calls — never to change behavior.

## 1. Cargo.toml (final, apply verbatim structure)

```toml
[package]
name = "ds4cc"
version = "3.2.0"
edition = "2024"
description = "Daemon bridging DualSense/DS4 controllers with AI coding agents"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "time", "macros", "sync"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
serde_json = "1"
log = "0.4"
libc = "0.2"
env_logger = "0.11"
image = { version = "0.25", default-features = false, features = ["png"] }
ureq = "3"

[target.'cfg(windows)'.dependencies]
hidapi = { version = "2.6", features = ["windows-native"] }
windows-sys = { version = "0.59", features = ["Win32_UI_Input_KeyboardAndMouse", "Win32_UI_WindowsAndMessaging", "Win32_System_Console"] }
windows = { version = "0.58", features = ["Win32_System_Com", "Win32_Media_Audio", "Win32_Media_Audio_Endpoints", "Win32_Foundation"] }
tray-icon = "0.21"

[target.'cfg(target_os = "linux")'.dependencies]
hidapi = { version = "2.6", features = ["linux-static-hidraw"] }
evdev = "0.13"
ksni = { version = "0.3", features = ["blocking"] }
dirs = "6"
```
Delete Cargo.lock, regenerate on first build (vendored offline registry is wired in
`.cargo/config.toml` → `../vendor-linux`; keep that file, gitignore `vendor-linux/`).

## 2. Validated API snippets (DO NOT GUESS — these compile)

### evdev 0.13 uinput
```rust
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};
let mut keys = AttributeSet::<KeyCode>::new();
keys.insert(KeyCode::KEY_A); keys.insert(KeyCode::BTN_LEFT);
let mut rel = AttributeSet::<RelativeAxisCode>::new();
rel.insert(RelativeAxisCode::REL_X); rel.insert(RelativeAxisCode::REL_Y);
rel.insert(RelativeAxisCode::REL_WHEEL); rel.insert(RelativeAxisCode::REL_HWHEEL);
let mut dev = evdev::uinput::VirtualDeviceBuilder::new()?
    .name("ds4cc-virtual-input")
    .with_keys(&keys)?            // takes &AttributeSetRef; &AttributeSet derefs
    .with_relative_axes(&rel)?
    .build()?;
dev.emit(&[InputEvent::new(EventType::KEY.0, KeyCode::KEY_A.0, 1)])?;   // press
dev.emit(&[InputEvent::new(EventType::KEY.0, KeyCode::KEY_A.0, 0)])?;   // release
dev.emit(&[InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0)])?;      // SYN_REPORT
dev.emit(&[InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, dx)])?;
```

### ksni 0.3.6 blocking tray (own std thread)
```rust
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
struct MyTray { /* state */ }
impl ksni::Tray for MyTray {
    fn id(&self) -> String { "ds4cc".into() }
    fn title(&self) -> String { "DS4CC".into() }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> { /* ARGB32 Vec<u8>, premultiplied */ }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![ StandardItem { label: "Exit".into(),
              activate: Box::new(|t: &mut Self| { /* ... */ }),
              ..Default::default() }.into() ]
    }
}
let handle = tray.spawn()?;        // ksni::blocking::Handle<MyTray>
handle.update(|t| { /* mutate state, menu re-read */ });
handle.shutdown();
```

## 3. New shared key abstraction — src/keys.rs (C1 owns)

Mapper today resolves combo strings ("ctrl+shift+b", tmux "C-b"/"M-S-7", Claude Code names)
to Windows VK codes. Introduce ONE portable enum used by both OS paths:

```rust
pub enum Key { Ctrl, Alt, Shift, Super, Enter, Escape, Tab, Space, Backspace, Delete,
    Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert,
    F1..=F12 (as F(u8)), Char(char),       // letters/digits/punct, layout-neutral
    PrintScreen, ScrollLock, Pause, CapsLock, NumLock, Menu }
pub fn parse_combo(s: &str) -> Option<Vec<Key>>;   // "ctrl+shift+b", "Shift+7", tmux "C-b", "M-x"
```
- Windows: `Key::to_win_vk() -> (u16 vk, Option<char-shift>)` — C2 adapts existing SendInput
  call sites to lower from `Key` (keep exact current semantics incl. shifted chars like "Shift+7" → '&').
- Linux: `Key::to_evdev() -> KeyCode` (US-layout mapping, same as every linux hotkey tool).
- Unit tests: combo parsing matrix (must pass on both OSes).

## 4. Platform layer — src/platform/ (C1 owns)

```
src/platform/mod.rs        cfg re-export: pub use win_impl::* / linux_impl::*
src/platform/win_impl.rs   thin re-exports of existing windows code (paths, autostart, mic)
src/platform/linux/
    mod.rs                 pub use inject::*, mic::*, autostart::*, paths::*
    inject.rs              UinputInjector (evdev) — see §2
    mic.rs                 pactl/wpctl
    autostart.rs           systemd user unit + XDG fallback
    paths.rs               XDG config dir
```

### Exact trait surface (used by C2/C3/C4 — DO NOT DEVIATE)
```rust
// inject
pub trait Injector: Send {
    fn combo(&mut self, keys: &[Key]);          // press in order, release reverse
    fn key_down(&mut self, k: Key); fn key_up(&mut self, k: Key);   // hold/repeat
    fn mouse_rel(&mut self, dx: i32, dy: i32);
    fn wheel(&mut self, vertical: i32, horizontal: i32);
    fn click(&mut self);                        // left click press+release
}
pub fn new_injector() -> anyhow_result::Result<Box<dyn Injector>, InjectError>;
// InjectError::UinputUnavailable(msg) — C2 catches: logs remediation, daemon keeps running,
// injection calls become logged no-ops (feature-degraded, not fatal).

// mic
pub fn mic_toggle() -> Result<(), String>;
pub fn mic_is_muted() -> Option<bool>;          // for controller mic LED

// autostart
pub fn autostart_is_enabled() -> bool;
pub fn autostart_set(enabled: bool) -> Result<(), String>;

// paths
pub fn config_dir() -> PathBuf;                 // $XDG_CONFIG_HOME/ds4cc | ~/.config/ds4cc
pub fn log_dir() -> PathBuf;                    // $XDG_STATE_HOME/ds4cc | ~/.local/state/ds4cc
```

### Linux inject.rs requirements
- One virtual device "ds4cc-virtual-input" with the FULL key set used by any default mapping
  + every Key variant (superset table) + REL_X/REL_Y/REL_WHEEL/REL_HWHEEL + BTN_LEFT.
- Combo semantics: modifiers down → keys → reverse up → SYN. Holds (d-pad repeat) use
  key_down/key_up without auto-release.
- Scroll: REL_WHEEL positive = up (match Windows SendInput sign convention already in mapper;
  adjust sign HERE so mapper stays shared), REL_HWHEEL for horizontal.
- Errors: /dev/uinput missing → precise log: "modprobe uinput; install packaging/linux/99-ds4cc.rules;
  add user to uinput group; re-login". NEVER panic.

### Linux mic.rs requirements
- `pactl set-source-mute @DEFAULT_SOURCE@ toggle`; if pactl missing/fails →
  `wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle`. Fixed argv arrays, no shell.
- `mic_is_muted`: `pactl get-source-mute @DEFAULT_SOURCE@` parse yes/no; wpctl fallback
  (`wpctl get-volume @DEFAULT_AUDIO_SOURCE@` contains "[MUTED]"). None if both unavailable.
- Cache last toggle result so the mic LED works even when query fails (mirror Windows behavior
  of LED tracking actual state; document divergence if any).

### Linux autostart.rs
- Unit file written to ~/.config/systemd/user/ds4cc.service (content from packaging/linux/ds4cc.service,
  ExecStart = current exe abs path); enable/disable via `systemctl --user enable|disable --now ds4cc.service`.
- If systemctl --user unavailable → XDG autostart ~/.config/autostart/ds4cc.desktop (Terminal=false).
- is_enabled: systemctl --user is-enabled OR desktop file exists.

## 5. C2 — mapper.rs, main.rs, output.rs, mic.rs wiring

- mapper.rs: every SendInput/SendMessage/windows-sys input call site → route through
  `platform::Injector` (shared trait object passed in or global OnceLock — follow existing
  structure, least invasive). Windows impl of Injector wraps the EXISTING SendInput code
  (move it into platform/win_impl or keep in mapper behind cfg — your call, behavior equal).
  Keep: d-pad hold-repeat timing, chord sequences, admission limits, stick→mouse math,
  scroll scaling, touchpad click, USB/BT input differences. PURE logic stays shared.
- main.rs: `mod codex_*` declarations → `#[cfg(windows)]`. ALL codex runtime startup/LED
  projection → cfg(windows). Linux startup: init logger → parse CLI flags (§9) → load config
  (config_dir()) → keybind detect (C3's detect) → create Injector (degraded-ok) → HID connect
  loop (unchanged) → tray thread (C4) → existing event loop.
- output.rs: HID lightbar/mic-LED report builders are pure protocol — SHARED, no status→LED
  codex projection on linux. Static [lightbar] color + mic LED state only. If output.rs has
  windows-specific calls, cfg them.
- mic.rs: file becomes platform dispatch — windows COM impl cfg(windows); linux → platform::mic.
- Keep `--` flag parsing minimal (no clap): `--help`, `--version`, `--verbose`, `--no-tray`.

## 6. C3 — detect.rs, wsl.rs, hid.rs, controller.rs, launcher.rs

- wsl.rs → `#[cfg(windows)]` entirely.
- NEW src/native_run.rs (linux): `run(argv: &[&str], timeout_ms) -> io::Result<String>` using
  std::process::Command, capture stdout, kill on timeout.
- detect.rs: single-probe semantics preserved. Linux: tmux via native_run
  (`tmux show-options -g prefix`, `tmux list-keys -T prefix`, fallback ~/.tmux.conf),
  claude keybindings read ~/.claude/keybindings.json natively. REUSE tmux_detect.rs parsers
  untouched. If tmux absent → same graceful degradation as "no WSL" path on Windows.
- hid.rs: verify hidapi calls are OS-neutral (they should be). Linux specifics: hidraw paths
  /dev/hidrawN; permission errors → log udev remediation (packaging rule). Preserve USB-priority
  + BT-fallback reconnect + CRC validation exactly.
- controller.rs: VID/PID table shared; no changes expected beyond cfg hygiene.
- launcher.rs: READ IT FIRST. If it launches Wispr Flow / downloads exe → cfg(windows); on linux
  replace with: if config `[voice] app_command` non-empty → spawn it (no shell), else no-op+log.

## 7. C4 — tray.rs, update.rs, packaging, docs

- tray.rs: keep Windows tray-icon impl cfg(windows). Linux: NEW ksni implementation (§2) in
  src/platform/linux/tray.rs or tray_linux.rs, SAME menu semantics:
  | Windows item | Linux item |
  |---|---|
  | Open Wispr Flow | "Open voice app" — only if `[voice] app_command` set |
  | Restart | Restart (re-exec std::env::current_exe, exec(2) replace) |
  | Check for Update | same (update.rs) |
  | Enable auto start-up (checkmark) | same via platform::autostart |
  | Mouse: Left Stick (checkmark) | same state toggle (shared with mapper) |
  | Show Log Window | "Open log file" (xdg-open log_dir/ds4cc.log) — file logging on linux |
  | Exit | Exit |
  Icon: decode assets/icon.ico (image crate can't do ico — embed a 64x64 ARGB PNG: convert
  icon.ico→png AT BUILD TIME via build.rs on linux? NO — simpler: C4 extracts a PNG once,
  decodes assets/icon.ico at runtime (image crate `ico` feature), resizes to 64x64, converts RGBA→premultiplied ARGB for ksni).
- update.rs: keep Windows flow cfg(windows). Linux: query same GitHub releases API (ureq),
  pick asset name containing "linux" AND "x86_64" (expect ds4cc-linux-x86_64.tar.gz);
  download → extract via system `tar xzf` to tempdir → chmod +x → atomic replace current_exe
  (rename new over old; linux allows replacing running binary via rename) → prompt restart in tray.
  If no linux asset in latest release → "no update available" (graceful).
- packaging/linux/99-ds4cc.rules (EXACT):
  ```
  # DualSense / DS4 hidraw access
  KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="05c4|09cc|0ba0|0ce6|0df2", MODE="0660", GROUP="input", TAG+="uaccess"
  # uinput for ds4cc virtual keyboard/mouse
  KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"
  ```
- packaging/linux/ds4cc.service (systemd user): ExecStart=%h/.local/bin/ds4cc,
  Restart=on-failure, WantedBy=default.target.
- packaging/linux/ds4cc.desktop (XDG autostart fallback).
- packaging/linux/install.sh: detects apt vs pacman; installs runtime deps
  (Ubuntu: libudev1 tar; Arch: systemd-libs tar; optional tmux); groupadd -f uinput;
  usermod -aG uinput,input $USER; installs udev rule (sudo), binary → ~/.local/bin,
  systemd unit (no sudo), prints re-login + BT pairing instructions. POSIX sh, set -eu.
- README.md: add "Linux (Ubuntu/Arch)" section — requirements, quick install, permissions,
  BT pairing (PS+Share), tray notes (KDE native; GNOME needs AppIndicator extension),
  codex runtime = Windows-only note, feature parity table. Keep Windows sections intact.
- HIGHLIGHTS.md: append Linux port entry.

## 8. Config (C1 owns the code change, all must respect)
- config.rs: config path resolution → platform::config_dir()/config.toml. Schema UNCHANGED.
  `[codex_micro]` stays parseable on linux (serde defaults) but its runtime is cfg(windows);
  on linux if codex_micro.enabled == true → log warning "codex runtime is Windows-only; ignored".
- NEW optional `[voice] app_command = ""` (default empty; linux tray uses it; windows ignores).
- NEW optional `[linux]` section NOT allowed — keep schema OS-neutral.

## 9. CLI flags (C2)
`--help` (print usage+flags, exit 0), `--version` (print "ds4cc 3.2.0", exit 0),
`--verbose` (debug logging), `--no-tray` (skip tray; also auto-skip if D-Bus session bus
unreachable — log info). These make headless smoke (verifier G8) possible.

## 10. Tests (each coder adds, all must pass centrally)
- C1: keys.rs parse matrix; config XDG resolution (env override); mic parse of pactl output strings.
- C2: existing mapper tests keep passing + combo→Key lowering unit tests (no device needed).
- C3: tmux parse tests unchanged (they're pure); native detect with fake tmux script in PATH.
- C4: update asset-name matcher unit test (fake release JSON).
- NO test may require /dev/uinput, D-Bus, HID, tmux server, or network (mock via PATH fixtures).

## 11. Definition of done (per coder)
1. Your files written into /mnt/agents/output/omegag-port/repo/ per ownership (NO other files touched).
2. `git add -A && git commit` on YOUR branch inside repo (branches: linux/c1-platform, linux/c2-mapper,
   linux/c3-detect, linux/c4-tray) — repo has no remote; local commit only.
3. If you can build (rustup in /tmp, env vars from §12): `cargo check --offline` passes for your scope.
   If env blocks you, say so explicitly in the final report.
4. Final report: files changed, design decisions, risks, anything SPEC didn't cover.

## 12. Build environment recipe (for anyone building in this sandbox)
```bash
export RUSTUP_HOME=/tmp/rh CARGO_HOME=/tmp/ch PATH="/tmp/ch/bin:$PATH"
export PKG_CONFIG_PATH=/tmp/local/usr/lib/x86_64-linux-gnu/pkgconfig PKG_CONFIG_SYSROOT_DIR=/tmp/local
export CFLAGS="-I/tmp/local/usr/include" RUSTFLAGS="-L /tmp/local/usr/lib/x86_64-linux-gnu"
# rustup+libudev install steps if /tmp was wiped: see omegag-port/buildbox.sh
cargo build --offline --release
```
