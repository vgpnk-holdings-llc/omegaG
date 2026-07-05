<p align="center">
  <img src="imgs/logo_nobg.png" alt="DS4CC" width="320">
</p>

<h1 align="center">DS4CC</h1>

<p align="center">
  Turn a PlayStation controller into a shortcut mapper for terminal-first development.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024_edition-f74c00?logo=rust&logoColor=white" alt="Rust">
</p>

---

## Mission

One controller, one flat button map, zero ceremony. Buttons send real keystrokes — tmux window control, Claude Code shortcuts, arrows, Enter/Escape/Tab — plus mouse cursor, scroll, and mic mute. Keep the system simple.

---

## What This Is

DS4CC is a small Rust daemon that runs in the Windows tray and lets your PlayStation controller:

- **Send keystrokes** — every button maps to a key combo or chord sequence
- **Drive tmux** — window navigation on the shoulder buttons, keybinds auto-detected from your running tmux server via WSL
- **Drive Claude Code CLI** — map buttons to Claude Code actions (e.g. `chat:cycleMode`), auto-resolved from `~/.claude/keybindings.json`
- **Move the mouse** — touchpad swipe or left stick moves the cursor, touchpad press clicks
- **Scroll** — right stick, vertical + horizontal
- **Toggle the mic** — DualSense mute button toggles the system microphone
- **Pair with [Wispr](https://ref.wisprflow.ai/vgpnk)** — voice handles text, controller handles everything else

No hooks, no polling, no agent-state tracking, no profiles. It is a shortcut mapper.

---

## Quick Start

1. Download **[DS4CC-Setup.exe](https://github.com/VeigaPunk/DS4CC/releases/latest)** and run it
2. Plug in your controller — done

Keybinds are detected automatically on launch (one WSL round-trip). Everything is configurable via `%APPDATA%\ds4cc\config.toml`, but defaults work out of the box.

---

## How It Works

1. You launch `ds4cc.exe` — the console window is hidden, a tray icon appears
2. One WSL probe detects your tmux prefix + bindings and your Claude Code keybindings
3. Every configurable button resolves to a key sequence at startup
4. It connects to your controller via HID and maps input → `SendInput` keystrokes

---

## Button Map

### Fixed

| Button | Action |
|---|---|
| Cross (×) | Enter |
| Circle (○) | Escape |
| Triangle (△) | Tab |
| D-pad | Arrow keys (hold to repeat) |
| Right stick | Scroll (vertical + horizontal) |
| Left stick / Touchpad | Mouse cursor (mode toggled in tray) |
| Touchpad press | Mouse left-click |
| L2 (hold) | Ctrl+Win — Wispr push-to-talk |
| L3 | Ctrl+T |
| R3 | Ctrl+U (clear line) |
| Mute | Toggle system microphone (DualSense only) |

### Configurable (`[buttons]` in config.toml)

| Button | Default |
|---|---|
| L1 | tmux: previous-window |
| R1 | tmux: next-window |
| R2 | tmux: kill-window |
| Square (□) | tmux: new-window |
| Share | unmapped |
| Options | unmapped |
| Touchpad button | unmapped (active when `[touchpad] enabled = false`) |

A button value can be:

1. **A tmux action name** — `"previous-window"` → sends prefix + the key bound to it (auto-detected from your tmux server, falling back to tmux defaults)
2. **A Claude Code action name** — `"chat:cycleMode"` → sends the key from `~/.claude/keybindings.json`
3. **A direct key combo** — `"ctrl+g"`, `"Shift+7"`
4. **Empty string** — unmapped

---

## Configuration

Config file: `%APPDATA%\ds4cc\config.toml` — all settings optional.

```toml
[buttons]
l1 = "previous-window"      # tmux action
r1 = "next-window"
r2 = "kill-window"
square = "new-window"
share = "chat:cycleMode"    # Claude Code action
options = "ctrl+shift+b"    # direct combo

[tmux]
auto_detect = true          # query the running tmux server via WSL
prefix = "Ctrl+B"           # fallback if auto-detect fails

[scroll]
dead_zone = 20
sensitivity = 1.0
horizontal = true

[touchpad]
enabled = true
sensitivity = 1.5           # cursor speed multiplier for touchpad swipe

[stick_mouse]
enabled = true
sensitivity = 8.0           # max pixels/frame at full deflection
dead_zone = 15

[lightbar]                  # static lightbar color
r = 255
g = 140
b = 0
```

---

## Keybind Detection

On launch DS4CC runs a **single WSL command** that fetches:

- `tmux show-options -g prefix` — your prefix key
- `tmux list-keys -T prefix` — the full binding table (falls back to parsing `~/.tmux.conf` when no server is running)
- `~/.claude/keybindings.json` — Claude Code CLI keybindings

Missing pieces degrade gracefully: no WSL → hardcoded defaults, no tmux → config prefix, no Claude Code → action names fall through to direct-combo parsing.

---

## 🎙️ Controller + Wispr = No Keyboard

DS4CC pairs with [Wispr Flow](https://ref.wisprflow.ai/vgpnk):

- **Wispr** handles all text input — you talk, it types (L2 is the push-to-talk hold)
- **DS4CC** handles everything else — navigation, tmux, scrolling, Enter/Escape/Tab

Voice dictates. Controller navigates. No keyboard required.

---

## Tray Icon

| Menu item | What it does |
|---|---|
| Open Wispr Flow | Launch Wispr Flow (prompts to download if not found) |
| Restart | Restart DS4CC |
| Check for Update | Self-update from GitHub releases |
| Enable auto start-up | Toggle Windows startup entry |
| Mouse: Left Stick | Switch cursor control between touchpad and left stick |
| Show Log Window | Show/hide the console log window |
| Exit | Quit |

---

## Supported Controllers

| Controller | USB | Bluetooth |
|---|:---:|:---:|
| DualSense | ✓ | ✓ |
| DualShock 4 | ✓ | ✓ |

USB takes priority; Bluetooth is the automatic fallback. DS4 defaults to stick-mouse mode (no touchpad coordinate parsing).

## Requirements

- Windows 10 / 11
- DualSense or DualShock 4 controller (USB or Bluetooth)
- **Optional:** WSL2 — needed for tmux / Claude Code keybind detection

---

## Install

### Installer (recommended)

Download **DS4CC-Setup.exe** from [Releases](https://github.com/VeigaPunk/DS4CC/releases) and run it.

- Installs to `%LOCALAPPDATA%\DS4CC` — no admin rights needed
- Auto-start is **off by default** (opt-in checkbox)

### Build from source

Requires [Rust](https://rustup.rs). From WSL, cross-compile with the GNU toolchain:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64   # provides the linker + windres
cargo build --release --target x86_64-pc-windows-gnu
```

Binary: `target/x86_64-pc-windows-gnu/release/ds4cc.exe`. (`assets/ds4cc.res` is a COFF resource generated with `x86_64-w64-mingw32-windres assets/ds4cc.rc -O coff -o assets/ds4cc.res`.)

To build the installer, compile `installer/ds4cc.iss` with [Inno Setup](https://jrsoftware.org/isinfo.php) (`ISCC.exe installer\ds4cc.iss`).

---

## Architecture

```
main.rs            Startup, connection loop, input/output orchestration
config.rs          TOML config with serde defaults
detect.rs          Single-probe keybind detection (tmux + Claude Code via WSL)
tmux_detect.rs     Pure tmux notation/binding parsers
controller.rs      VID/PID detection, controller type enums
hid.rs             HID device discovery, open, read/write
input.rs           Raw HID report parsing → UnifiedInput
mapper.rs          Button map resolution + dispatch, d-pad repeat, scroll, mouse
output.rs          HID output reports (lightbar + player LED + mic LED)
mic.rs             System microphone toggle via Core Audio COM
tray.rs            System tray icon + menu
update.rs          Self-update from GitHub releases
wsl.rs             Shared WSL command execution utility
```

## Technical Notes

- Rust 2024 edition, `tokio` async runtime
- HID via `hidapi` (BT CRC validation, USB-priority reconnect loop)
- Input read timeout 5ms; output refresh 100ms (static lightbar + mute LED)
- Mic mute: Windows Core Audio COM API (`IAudioEndpointVolume`)

## License

MIT
