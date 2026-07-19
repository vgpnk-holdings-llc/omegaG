<p align="center">
  <img src="imgs/logo_nobg.png" alt="omegaG" width="320">
</p>

<h1 align="center">omegaG</h1>

<p align="center">
  Turn a PlayStation controller into a shortcut mapper for terminal-first development.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024_edition-f74c00?logo=rust&logoColor=white" alt="Rust">
</p>

---

## Mission

OmegaG explores safe controller semantics for coding workflows while retaining
the proven DS4CC shortcut mapper and its simple defaults.

---

## What This Is

OmegaG is a fork of [VeigaPunk/DS4CC](https://github.com/VeigaPunk/DS4CC).
It preserves the `ds4cc` package, binary, configuration path, and MIT attribution
for compatibility. The daemon runs in the Windows tray and lets your controller:

- **Send keystrokes** — every button maps to a key combo or chord sequence
- **Drive tmux** — window navigation on the shoulder buttons, keybinds auto-detected from your running tmux server via WSL
- **Drive Claude Code CLI** — map buttons to Claude Code actions (e.g. `chat:cycleMode`), auto-resolved from `~/.claude/keybindings.json`
- **Move the mouse** — touchpad swipe or left stick moves the cursor, touchpad press clicks
- **Scroll** — right stick, vertical + horizontal
- **Toggle the mic** — DualSense mute button toggles the system microphone
- **Pair with [Wispr](https://ref.wisprflow.ai/vgpnk)** — voice handles text, controller handles everything else

No hooks, no polling, no agent-state tracking, no profiles. It is a shortcut mapper.

An optional **OmegaG Codex controller runtime** provides a testable semantic
engine. Exact ChatGPT desktop parity is impossible through public APIs today.
The runtime is disabled by default and cannot consume legacy controls unless
`enabled = true`. It talks only to a supervised local `codex app-server --stdio`.

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

[codex_micro]
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

## OmegaG Codex controller runtime

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
