<p align="center">
  <img src="imgs/logo_nobg.png" alt="omegaG" width="320">
</p>

<h1 align="center">omegaG</h1>

<p align="center">
  Turn a PlayStation controller into a shortcut mapper for terminal-first development.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024_edition-f74c00?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/Linux-Ubuntu%20%2F%20Arch-FCC624?logo=linux&logoColor=black" alt="Linux">
  <img src="https://img.shields.io/badge/DualSense%20%2F%20DS4-USB%20%2B%20BT-003791?logo=playstation&logoColor=white" alt="Controllers">
</p>

<p align="center">
  <a href="https://veigapunk.github.io/omegag-site/"><b>Product site (primary)</b></a>
  · <a href="https://vgpnk-holdings-llc.github.io/omegaG/">org Pages (docs mirror)</a>
  · <a href="https://ds4cc-proto.kimi.page/">kimi.page (may lag)</a>
  · <a href="https://github.com/vgpnk-holdings-llc/omegaG/releases/tag/website-static">static zip</a>
  · source <a href="website/"><code>website/</code></a>
  · <a href="website/DEPLOY.md">deploy</a>
  · <code>bash website/publish-all.sh</code>
  · <a href="WEBSITE-AUDIT.md">audit</a>
</p>

> **Primary site:** always link users to
> [https://veigapunk.github.io/omegag-site/](https://veigapunk.github.io/omegag-site/)
> first. Holdings org Pages and release assets are secondary docs/mirrors only —
> do not treat the holdings remote as the product homepage.

---

## Mission

OmegaG explores safe controller semantics for coding workflows while retaining
the proven DS4CC shortcut mapper and its simple defaults.

---

## What This Is

OmegaG is a fork of [VeigaPunk/DS4CC](https://github.com/VeigaPunk/DS4CC).
It preserves the `ds4cc` package, binary, configuration path, and MIT attribution
for compatibility.

**Controller foundation:** DualSense / DualShock HID knowledge — report layouts,
Bluetooth extended mode, lightbar / player LEDs / mute LED, CRC — is grounded in
the work of **[@Ryochan7](https://github.com/Ryochan7)** and
[**DS4Windows**](https://github.com/Ryochan7/DS4Windows). That research is the
base this daemon is built on. See [CREDITS.md](CREDITS.md).

The daemon runs in the system tray on **Windows and Linux**
(Ubuntu / Arch, USB **and** Bluetooth) and lets your controller:

- **Send keystrokes** — every button maps to a key combo or chord sequence
- **Drive tmux** — window navigation on the shoulder buttons, keybinds auto-detected from your running tmux server (natively on Linux, via WSL on Windows)
- **Drive Claude Code CLI** — map buttons to Claude Code actions (e.g. `chat:cycleMode`), auto-resolved from `~/.claude/keybindings.json`
- **Move the mouse** — touchpad swipe or left stick moves the cursor, touchpad press clicks
- **Scroll** — right stick, vertical + horizontal
- **Toggle the mic** — DualSense mute button toggles the system microphone

No hooks, no polling, no agent-state tracking, no profiles. It is a shortcut mapper.

An optional **OmegaG Codex controller runtime** (Windows-only) provides a testable semantic
engine. Exact ChatGPT desktop parity is impossible through public APIs today.
The runtime is disabled by default and cannot consume legacy controls unless
`enabled = true`. It talks only to a supervised local `codex app-server --stdio`.

---

## Quick Start

**Windows**

1. Download **[DS4CC-Setup.exe](https://github.com/VeigaPunk/DS4CC/releases/latest)** and run it
2. When asked, leave **LordOfMice hidusbf** checked unless you opt out — optional USB HID
   buffering / overclock ([LordOfMice/hidusbf](https://github.com/LordOfMice/hidusbf));
   helpful for DualSense/DS4 latency (community ballpark: DS4 without tooling can sit in a
   ~200 ms class lag regime; DualSense with high-rate polling can reach sub‑ms order)
3. Plug in your controller — done

**Linux (Ubuntu / Arch)**

```bash
cargo build --release
packaging/linux/install.sh   # then log out and back in
```

Keybinds are detected automatically on launch (one probe — native on Linux, WSL on Windows). Everything is configurable via `config.toml` (`%APPDATA%\ds4cc` on Windows, `~/.config/ds4cc` on Linux), but defaults work out of the box.

---

## How It Works

1. You launch `ds4cc` — it backgrounds itself and a tray icon appears
2. One probe detects your tmux prefix + bindings and your Claude Code keybindings
3. Every configurable button resolves to a key sequence at startup
4. It connects to your controller via HID and maps input → keystrokes
   (`SendInput` on Windows, a `uinput` virtual keyboard/mouse on Linux)

---

## Button Map

### Fixed

| Button | Action |
|---|---|
| D-pad | Arrow keys (hold to repeat) |
| Right stick | Scroll (vertical + horizontal) |
| Left stick / Touchpad | Mouse cursor (mode toggled in tray) |
| Touchpad press | Mouse left-click |
| L2 (hold) | Ctrl+Win — Wispr push-to-talk |
| Mute | Toggle system microphone (DualSense only) |

### Configurable (`[buttons]` in config.toml)

| Button | Default |
|---|---|
| Cross (×) | enter |
| Circle (○) | escape |
| Triangle (△) | tab |
| L3 | ctrl+t |
| R3 | ctrl+u (clear line) |
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

Built-in launcher names available without defining `[launchers.*]`:
- `launcher:godspeed` → types `| godspeed` and presses Enter
- `launcher:colulossus` → types `COLULOSSUS` (no auto-submit)

---

## Configuration

Config file: `%APPDATA%\ds4cc\config.toml` (Windows) or `~/.config/ds4cc/config.toml` (Linux) — all settings optional.

```toml
[buttons]
l1 = "previous-window"      # tmux action
r1 = "next-window"
r2 = "kill-window"
square = "new-window"
share = "chat:cycleMode"    # Claude Code action
options = "ctrl+shift+b"    # direct combo

[tmux]
auto_detect = true          # query the running tmux server (native on Linux, WSL on Windows)
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

[voice]                     # Linux: optional voice app for the tray menu
app_command = ""            # e.g. "wispr-flow" — empty hides the tray item

[codex_micro]               # Windows-only; parsed but ignored on Linux
enabled = false             # opt in; legacy defaults remain untouched
codex_executable = "codex"   # PATH name, or an absolute Windows/WSL path
cwd = ""                     # empty = process working directory
request_timeout_ms = 15000
reconnect_min_ms = 250
reconnect_max_ms = 8000
composer_limit = 16384
voice_argv = []              # e.g. ["C:\\Tools\\stt.exe", "--capture"]
voice_timeout_ms = 30000
voice_output_limit = 16384
brightness = 70             # 0..100
inactivity_seconds = 180    # lightbar sleeps after three minutes
analog_dead_zone = 48
analog_hysteresis = 12
source_policy = "recent"    # recent | pinned | priority | custom
custom_order = []           # exact thread IDs for custom policy

[codex_micro.commands]      # exact prompt text submitted as a turn
review = "Review the current changes."
[codex_micro.skills]
test = "test"               # exact current advertised skill name or path
[codex_micro.cardinal_actions]
up = "Summarize progress."
right = "Run the configured checks."
```

## OmegaG Codex controller runtime (Windows-only)

> **Platform note:** the codex runtime and its session-status lightbar
> projection are Windows features. On Linux the `[codex_micro]` section is
> parsed for compatibility but the runtime never starts (a warning is logged
> if `enabled = true`).

### Controller layer

Hold **PS** to enter the discoverable, exclusive modifier layer. While held:

| Control | Semantic intent |
|---|---|
| L1 / R1 | Previous / next of six chat slots |
| Share | Create a blank thread (`thread/start`) |
| Cross / Circle | One-shot accept / decline for selected armed approval |
| Square | Toggle advertised `priority` tier for subsequent turns |
| Triangle | Send bounded composer (`turn/start`) |
| Options | Fork selected thread (`thread/fork`) |
| L2 | Hold starts/release stops; second press within 350 ms toggles hands-free latch; next press stops |
| Right stick cardinal | Four analog actions with dead zone + hysteresis |
| D-pad up/down | Previous / next model-advertised reasoning effort |
| L3 / R3 | First configured command / skill (sorted by configured name) |
| Touchpad press | Select; second press reads and resumes |

The semantic model has exactly six slots, `recent`, `pinned`, `priority`, and
`custom` source policies, and `idle`, `thinking`, `complete-unread`,
`requires-input`, `error`, and `unassigned` states. A press selects; a second
press within 350 ms (inclusive) activates. Reasoning indexes the model-advertised
efforts. Commands/cardinals submit configured prompt text; skill favorites are
resolved against the current exact advertised name/path before submission.

### Feedback

Status RGB uses white (idle), blue (thinking), green (complete unread), amber
(input required), red (error), and off (unassigned). The lightbar projects the
selected chat only; `priority` controls slot ordering, not status selection.
The selected projection pulses at 500 ms, honors configured brightness, sleeps
after 180 seconds by default, and wakes on modifier input. RGB is composed into
the **existing single HID output loop**; there is no second writer. The same
report builders cover DualSense and DS4 over USB and Bluetooth. A controller
lightbar is lossy: it can show one selected status, not six colors at once; DS4
has no player or mute LEDs. A rejected mutation turns feedback red and logs a
sanitized error without command or transcript bodies.

### Runtime, setup, and safety

Install exactly `codex-cli 0.145.0-alpha.24`, verify `codex --version`, authenticate
Codex normally, and set `enabled = true`. omegaG starts local
`codex app-server --stdio`, performs initialize → response → initialized, then
requests model, thread, and skill catalogs. Version mismatch, malformed/oversized
frames, queue pressure, timeout, and EOF fail closed and reconnect with bounded
backoff. Server epochs remain independent of controller HID reconnects. This is
controller-native behavior comparable to a Codex frontend, not literal keyboard
hardware emulation, and it performs no ChatGPT desktop automation.

Approvals retain the original inbound JSON-RPC ID plus server epoch, method,
thread, turn, item, and optional approval ID. Cross/Circle are enabled only for
the selected armed request and can return only one-shot `accept` or `decline`.
Command bodies and voice transcripts are never logged. Queue admission is not
reported as completion: mutating controls turn green only after local validation
and the correlated app-server response. Voice adapters require an absolute
executable, run argv directly without a shell, begin on L2 press, treat stdin EOF
on release as the stop signal, and write only the final transcript to stdout.
Hard process, timeout, output, and composer limits apply. On Windows use a native
absolute path; under WSL, run omegaG and the adapter in the same environment so
paths remain valid. Controller, app-server, and application shutdown cancel and
reap an active capture instead of finalizing a partial transcript. Adapters must
remain in the launched process and must not daemonize detached helper processes.

Existing generic mappings outside the modifier remain unchanged. Reconnect
neutralizes held/latching state, emits a PTT stop intent where necessary, and
requires a neutral controller frame before accepting presses. Semantic releases
bypass generic action admission limits, and legacy `KeyUp` releases cannot be
dropped by saturated motion traffic. In active runtime mode the exclusive modifier
suppresses buttons, both sticks, and touch coordinates. No undocumented desktop
API or deep link is used.

The reducer accepts authoritative generation/sequence-guarded snapshots,
lifecycle/status/turn/error upserts, and removals into six slots.

### Capability matrix

- [x] Six-slot typed reducer, statuses, source arrangement, ordering/generation guards
- [x] Inclusive 350 ms slot activation and PTT double-press state machine
- [x] Cardinal analog neutral hysteresis and bounded physical reasoning control
- [x] Selected-status RGB/pulse/brightness/sleep/wake through the sole output loop
- [x] Full mutation identity validation, reconnect neutral gating, release bypass
- [x] Backward-compatible opt-in configuration and deterministic pure tests
- [x] Fast/approve/decline/start/fork/PTT/send/command/skill runtime effects
- [x] Supervised app-server handshake, catalogs, events, bounds, and reconnect
- [x] Supported thread read/resume activation without desktop automation

---

## Keybind Detection

On launch DS4CC runs a **single probe** that fetches:

- `tmux show-options -g prefix` — your prefix key
- `tmux list-keys -T prefix` — the full binding table (falls back to parsing `~/.tmux.conf` when no server is running)
- `~/.claude/keybindings.json` — Claude Code CLI keybindings

On Linux the probe runs natively; on Windows it runs through one WSL command.
Missing pieces degrade gracefully: no WSL/tmux → hardcoded defaults, no tmux server → config prefix, no Claude Code → action names fall through to direct-combo parsing.

---

## 🎙️ Controller + Voice = No Keyboard

DS4CC pairs with [Wispr Flow](https://ref.wisprflow.ai/vgpnk) on Windows
(configure any voice app on Linux via `[voice] app_command`):

- **Voice** handles all text input — you talk, it types (L2 is the push-to-talk hold)
- **DS4CC** handles everything else — navigation, tmux, scrolling, Enter/Escape/Tab

Voice dictates. Controller navigates. No keyboard required.

---

## Tray Icon

| Menu item | What it does |
|---|---|
| Open Wispr Flow | Launch Wispr Flow — on Linux: "Open voice app" when `[voice] app_command` is set |
| Restart | Restart DS4CC |
| Check for Update | Self-update from GitHub releases (per-OS asset) |
| Enable auto start-up | Toggle startup entry (registry on Windows, systemd user / XDG autostart on Linux) |
| Mouse: Left Stick | Switch cursor control between touchpad and left stick |
| Show Log Window | Show/hide the console log window — on Linux: "Open log file" |
| Exit | Quit |

On Linux the tray is a StatusNotifierItem (see the Linux section for desktop notes).

---

## Supported Controllers

| Controller | USB | Bluetooth |
|---|:---:|:---:|
| DualSense | ✓ | ✓ |
| DualShock 4 | ✓ | ✓ |

USB takes priority; Bluetooth is the automatic fallback. DS4 defaults to stick-mouse mode (no touchpad coordinate parsing).

## Requirements

- **Windows** 10 / 11, or **Linux**: Ubuntu 22.04+ / Arch
- DualSense or DualShock 4 controller (USB or Bluetooth)
- **Optional:** WSL2 (Windows) or native `tmux` (Linux) — for tmux / Claude Code keybind detection
- **Linux only:** group membership in `uinput` + `input` (the installer sets this up); optional `pactl`/`wpctl` for mic toggle

---

## Install

### Windows — Installer (recommended)

Download **DS4CC-Setup.exe** from [Releases](https://github.com/VeigaPunk/DS4CC/releases) and run it.

- Installs to `%LOCALAPPDATA%\DS4CC` — no admin rights needed
- Auto-start is **off by default** (opt-in checkbox)

### Linux — install script

```bash
cargo build --release
packaging/linux/install.sh
```

Installs to `~/.local/bin/ds4cc`, sets up udev rules, group membership and a
`systemd --user` unit (see the Linux section for details). Log out and back in
afterwards.

### Build from source

Requires [Rust](https://rustup.rs). Native Linux build:

```bash
cargo build --release          # binary: target/release/ds4cc
```

From WSL, cross-compile for Windows with the GNU toolchain:

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
main.rs            Startup, connection loop, input/output orchestration, CLI flags
keys.rs            Portable key model (combo parsing, per-OS lowering)
platform/          OS backends behind a shared interface
  linux/inject.rs    uinput virtual keyboard/mouse (evdev)
  linux/mic.rs       pactl / wpctl microphone toggle
  linux/autostart.rs systemd user unit + XDG autostart
  linux/paths.rs     XDG config/state directories
config.rs          TOML config with serde defaults (same schema on both OSes)
detect.rs          Single-probe keybind detection (native on Linux, WSL on Windows)
native_run.rs      Linux process runner (no shell, kill-on-timeout)
tmux_detect.rs     Pure tmux notation/binding parsers
controller.rs      VID/PID detection, controller type enums
hid.rs             HID device discovery, open, read/write (hidraw on Linux)
input.rs           Raw HID report parsing → UnifiedInput
mapper.rs          Button map resolution + dispatch, d-pad repeat, scroll, mouse
output.rs          HID output reports (lightbar + player LED + mic LED)
mic.rs             Microphone toggle dispatch (Core Audio COM on Windows)
tray.rs            Tray dispatch (tray-icon on Windows)
tray_linux.rs      StatusNotifierItem tray (ksni, pure-Rust D-Bus)
update.rs          Self-update from GitHub releases (per-OS assets)
wsl.rs             Shared WSL command execution utility (Windows only)
codex_*.rs         Optional codex controller runtime (Windows only)
```

## Technical Notes

- Rust 2024 edition, `tokio` async runtime; one codebase, `cfg`-gated OS backends
- HID via `hidapi` (BT CRC validation, USB-priority reconnect loop; hidraw backend on Linux)
- Input read timeout 5ms; output refresh 100ms (static lightbar + mute LED)
- Keystroke/mouse injection: `SendInput` on Windows, `evdev`/`uinput` virtual device on Linux
- Mic mute: Windows Core Audio COM (`IAudioEndpointVolume`); `pactl`/`wpctl` on Linux
- Tray: `tray-icon` on Windows, `ksni` (StatusNotifierItem) on Linux
- 215 unit/integration tests, all hardware-free (mocked tmux, parsers, injectors)

---

## Linux details (Ubuntu/Arch)

The same daemon runs natively on Linux — Ubuntu 22.04+ and Arch — with the
full shortcut-mapper feature set over USB **and** Bluetooth.

### Requirements

- Ubuntu 22.04+ or Arch Linux
- DualSense or DualShock 4 controller (USB or Bluetooth)
- Group membership in `uinput` + `input` (the installer sets this up)
- **Optional:** `tmux` — enables tmux keybind auto-detection (native, no WSL)
- **Optional:** `pactl` (PipeWire/PulseAudio) or `wpctl` — system mic toggle

### Quick install

```bash
cargo build --release
packaging/linux/install.sh
```

The installer (`packaging/linux/install.sh`, POSIX sh) detects `apt` vs
`pacman`, installs runtime deps (`libudev1`/`systemd-libs`, `tar`, `tmux`),
creates the `uinput` group, adds you to `uinput`+`input`, installs the udev
rule, copies the binary to `~/.local/bin/ds4cc`, and enables the
`systemd --user` unit (falls back to XDG autostart when `systemctl --user`
is unavailable). **Log out and back in afterwards** — group membership only
applies to new sessions.

### Permissions / udev

`packaging/linux/99-ds4cc.rules` grants:

- **hidraw access** to Sony controllers (VID `054c`: DS4 v1/v2, DualSense,
  DualSense Edge) for the `input` group + active session (`uaccess`)
- **/dev/uinput** access for the `uinput` group (virtual keyboard/mouse the
  daemon injects keystrokes through)

If injection fails, the daemon logs the exact remediation
(`modprobe uinput`, udev rule, group membership, re-login) and keeps running
in a feature-degraded mode — it never panics.

### Bluetooth pairing

1. Hold **PS + Share** until the light bar flashes rapidly (pairing mode)
2. Pair "Wireless Controller" from your desktop's Bluetooth settings

USB works out of the box and takes priority; Bluetooth is the automatic
fallback, exactly like on Windows.

### Tray icon

The tray is a StatusNotifierItem (ksni):

- **KDE Plasma** — works natively
- **GNOME** — install the *AppIndicator and KStatusNotifierItem Support*
  extension, otherwise no icon is shown (the daemon still runs headless)

Menu: *Open voice app* (only when `[voice] app_command` is set in
`~/.config/ds4cc/config.toml`), *Restart*, *Check for Update*,
*Enable auto start-up*, *Mouse: Left Stick*, *Open log file*, *Exit*.
Self-update replaces the running binary atomically from the
`ds4cc-linux-x86_64.tar.gz` release asset; releases without a Linux asset
are reported as "no update available".

CLI flags: `--help`, `--version`, `--verbose`, `--no-tray` (the tray is also
auto-skipped when no D-Bus session bus is reachable).

### Codex runtime is Windows-only

The optional OmegaG Codex controller runtime (`[codex_micro]`) remains a
Windows feature. The config section still parses on Linux (defaults apply),
but if `enabled = true` the daemon logs a warning and ignores it.

### Feature parity

| Feature | Windows | Linux |
|---|:---:|:---:|
| Button → key combo mapping | ✓ | ✓ (evdev/uinput) |
| D-pad hold-repeat, chords, admission limits | ✓ | ✓ |
| Mouse via touchpad / left stick, scroll | ✓ | ✓ |
| tmux keybind auto-detect | ✓ (via WSL) | ✓ (native) |
| Claude Code keybindings detect | ✓ | ✓ (native) |
| Launcher actions (`launcher:<name>`) | ✓ | ✓ |
| Mic toggle (DualSense mute button) | ✓ (Core Audio) | ✓ (`pactl`/`wpctl`) |
| Tray menu parity | ✓ | ✓ (ksni; "Open voice app" replaces Wispr) |
| Auto-start toggle | ✓ (registry) | ✓ (systemd --user / XDG autostart) |
| Self-update | ✓ (installer) | ✓ (tarball, atomic replace) |
| USB priority + BT fallback reconnect | ✓ | ✓ |
| Codex controller runtime | ✓ | ✗ (Windows-only) |
| Wispr Flow integration | ✓ | via `[voice] app_command` |

Config on Linux: `~/.config/ds4cc/config.toml` — same schema as Windows,
plus the optional:

```toml
[voice]
app_command = ""   # e.g. "wispr-flow" — adds "Open voice app" to the tray
```

## Acknowledgments

- **[@Ryochan7](https://github.com/Ryochan7)** / [DS4Windows](https://github.com/Ryochan7/DS4Windows) — DualShock 4 & DualSense HID research and tooling that made this project possible. Full note in [CREDITS.md](CREDITS.md).

## License

MIT
