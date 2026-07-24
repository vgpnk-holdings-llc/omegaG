# DS4CC Launcher Integration — Technical Highlights

## What was added

DS4CC now supports a **named action catalog** called the *launcher*. You assign a button to `launcher:<name>` in config and that button types a Unicode string into whatever window is focused — optionally pressing Enter at the end.

This mirrors the [claude-launcher](https://github.com/anthropics/claude-code) approach: pressing a gamepad button sends a slash command into your Claude Code terminal without leaving the controller.

---

## How it works — the big picture

```
Controller hardware
    │  (HID, ~250 Hz USB / ~125 Hz BT)
    ▼
HID poll loop (async task, main.rs)
    │  parse bytes → UnifiedInput
    ▼
MapperState::update()          ← pure, no I/O, runs inline
    │  returns Vec<Action>
    │  LauncherText { text, enter } is one action variant
    ▼
tokio channel (bounded, 32 slots)   ← non-blocking try_send
    │
    ▼
action_worker task (separate tokio task)
    │  execute_action()
    ▼
Windows: SendInput (KEYEVENTF_UNICODE per UTF-16 code unit)
Linux:   Wayland → wtype -- <text>   (virtual-keyboard protocol)
         X11     → xdotool type --clearmodifiers --delay 0 -- <text>   (fallback)
```

The HID loop **never blocks** waiting for text injection to finish. Rapid presses enqueue in FIFO order; the worker processes them one by one. If the queue fills up (32+ pending actions), the newest action is dropped with a warning rather than stalling the poll loop.

### Submit timing (two-phase, all platforms)

Injection is a **direct clone of claude-launcher's two-phase submit**:

1. **Type the whole text as one instantaneous batch** — zero per-character delay. Windows sends every UTF-16 code unit in a single atomic `SendInput`; Wayland/X11 type the payload in a single `wtype`/`xdotool` invocation (`xdotool` with an explicit `--delay 0`).
2. **Wait exactly `ENTER_DELAY_MS = 16 ms`, then press Return.** This guard lets the focused app finish processing the text batch before Enter arrives, so the two events never race. The same `16 ms` constant is shared verbatim by the Windows and Linux paths.

`ENTER_DELAY_MS` lives in `mapper.rs` and is asserted by test (`enter_delay_is_exactly_16ms`); the phase order (text → 16 ms → Return) is asserted by `injection_plan_types_text_then_enter_when_submitting`.

---

## The action catalog

### Built-ins (no config needed)

| Name | Text | Enter | Notes |
|------|------|-------|-------|
| `godspeed` | `\| godspeed` | yes | Matches claude-launcher exactly |

Built-ins are **unassigned** — no default button fires them. Wire one up:

```toml
[buttons]
share = "launcher:godspeed"
```

### User-defined actions

```toml
[launchers.xbreed]
text  = "/xbreed "
enter = false   # leave cursor at end for you to type the prompt

[launchers.wwkd]
text  = "/wwkd "
enter = false

[launchers.commit]
text  = "git commit -m '"
enter = false
```

Resolution order per `launcher:<name>`:
1. User entry in `[launchers]` (can override built-ins)
2. Built-in catalog (`launcher.rs`)
3. Unknown name → button silently unmapped

---

## Why the queue is bounded (32 slots)

The action worker can block for up to ~16 ms per launcher action (text injection + the 16 ms Enter guard). At 250 Hz that is ~4 frames of potential queue growth per Enter press.

With 32 slots, you can queue about 8–10 sequential Enter-submitting presses before overflow — far more than any real use case. The bound prevents the queue from growing forever if the worker falls behind, keeping memory and latency predictable.

`try_send` returns immediately on both success and failure, so the HID poll loop latency is unaffected regardless of action worker load.

---

## Unicode handling

On **Windows**, text is injected via `KEYEVENTF_UNICODE` — one `INPUT` struct per UTF-16 code unit, all sent in a single `SendInput` call (atomic to the OS). Surrogate pairs (emoji, characters above U+FFFF) become two code units, which Windows reassembles into the correct character before delivering to the focused application. Shell metacharacters have no special meaning in this path.

On **Linux**, the backend is chosen from `XDG_SESSION_TYPE` at inject time:

- **Wayland** (`XDG_SESSION_TYPE=wayland`) → `wtype -- <text>`, then `wtype -k Return` for optional Enter. `wtype` speaks the Wayland virtual-keyboard protocol, so it works natively on Hyprland, Sway, GNOME, and KDE. It maps each UTF-32 code point to an xkb keysym, so emoji and accented characters inject correctly.
- **X11 / anything else** → `xdotool type --clearmodifiers --delay 0 -- <text>`, then `xdotool key Return`. The `--delay 0` types the payload instantaneously (xdotool's default is 12 ms/key). This is the explicit fallback for real X11 or XWayland-only sessions.

In both paths the text is passed as a **single discrete process argument after a `--` end-of-options separator** — never a shell string. That prevents both shell interpolation and option injection, even when the payload begins with `-`. If the selected backend executable is not installed, the call logs a warning and returns without crashing the action worker.

---

## Default shortcuts are byte-for-byte unchanged

All 12 default button mappings are preserved exactly:

| Button | Default action |
|--------|---------------|
| L1 | tmux prev-window (Ctrl+B, P) |
| R1 | tmux next-window (Ctrl+B, N) |
| R2 | tmux kill-window (Ctrl+B, &) |
| Square | tmux new-window (Ctrl+B, C) |
| Share | *(unmapped)* |
| Options | *(unmapped)* |
| Touchpad | *(unmapped, or MouseClick if touchpad enabled)* |
| Cross | Enter |
| Circle | Escape |
| Triangle | Tab |
| L3 | Ctrl+T |
| R3 | Ctrl+U |

No default button is pre-wired to any launcher action. The `no_default_button_maps_to_launcher` test enforces this at the type level.

---

## Config schema reference

```toml
# Zero or more named launcher actions
[launchers.myaction]
text  = "unicode text here 🎮"   # required; empty = unmapped
enter = true                      # optional, default false

# Wire a button to a launcher action
[buttons]
share = "launcher:myaction"

# Other button values still work alongside launcher actions:
# share = "previous-window"       # tmux action
# share = "chat:externalEditor"   # Claude Code keybinding
# share = "ctrl+shift+b"          # direct key combo
# share = ""                      # unmapped (silent)
```

---

## Verification summary

- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test`: **142/142 passed, 0 ignored**
  - Regression: all default shortcuts verified (L1→prev-window, Cross→Enter, R3→Ctrl+U, etc.)
  - Launcher: built-in catalog, `launcher:` prefix parsing, user-config override, built-in fallback
  - Unicode: emoji, multibyte surrogate pairs, optional Enter
  - Queue ordering: rapid presses each produce exactly one action, held button does not repeat
  - Malformed: unknown name → unmapped, empty text → unmapped
  - Cross-action ordering: launcher before regular in same frame
  - Mapper/channel: bounded `try_send` path exercised
  - **Linux backend:** Wayland/X11 selection from `XDG_SESSION_TYPE`, exact `wtype`/`xdotool` argv construction, Unicode payload preservation, Enter/no-Enter, missing-executable graceful degradation, FIFO queue preservation
  - **Submit timing:** `ENTER_DELAY_MS == 16` constant, two-phase order (text → 16 ms → Return), Return omitted when `enter=false`, zero per-character delay on both backends

**Blockers:** none. The Linux build injects natively on Wayland via `wtype` (validated on Hyprland) and falls back to `xdotool` on X11. Binary links only on Windows for the Win32 `SendInput` path (expected); the full test suite runs on Linux.

---

# Linux Port (Ubuntu/Arch) — Technical Highlights

## What was added

DS4CC now runs natively on Linux (Ubuntu 22.04+, Arch) with **all** legacy
shortcut-mapper features preserved over USB and Bluetooth. The optional Codex
controller runtime stays Windows-only; its config still parses on Linux and is
ignored with a warning.

## Tray: StatusNotifierItem via ksni

The Windows `tray-icon` implementation is unchanged behind `cfg(windows)`;
Linux gets a `ksni` (blocking API) tray on its own std thread with full menu
parity:

| Windows | Linux |
|---|---|
| Open Wispr Flow | "Open voice app" — only when `[voice] app_command` is set |
| Restart | Re-exec of `current_exe` via exec(2) — same PID, fresh image |
| Check for Update | Linux self-update flow (below) |
| Enable auto start-up | `systemd --user` unit, XDG autostart fallback |
| Mouse: Left Stick | Same shared `Arc<AtomicBool>` with the mapper |
| Show Log Window | "Open log file" (`xdg-open ~/.local/state/ds4cc/ds4cc.log`) |
| Exit | Exit |

The icon is `assets/icon.png` (64×64 RGBA, extracted once from `icon.ico`),
embedded with `include_bytes!` and converted to premultiplied ARGB32 for
`ksni::Icon`. If the D-Bus session bus or a StatusNotifierWatcher is
unreachable (headless, GNOME without the AppIndicator extension), the tray
thread logs and exits gracefully — the daemon keeps running; `--no-tray` and
an automatic session-bus check let `main` skip it entirely.

## Self-update without an installer

The Linux update flow queries the same GitHub releases API, selects the
asset whose name contains `linux` **and** `x86_64`
(`ds4cc-linux-x86_64.tar.gz`), downloads it, extracts with the system
`tar xzf` into a temp dir, `chmod +x`s the binary, stages it next to the
running executable, and atomically `rename(2)`s it over `current_exe`
(Linux permits replacing a running binary). The result is surfaced as a
desktop notification (`notify-send`, log fallback) prompting a tray Restart.
Releases without a Linux asset degrade to a graceful "no update available".
The asset-name matcher is unit-tested against fake release JSON (x86_64 vs
aarch64 vs Windows-only releases, case-insensitivity, empty asset lists).

## Packaging

- `packaging/linux/99-ds4cc.rules` — hidraw access for Sony VID `054c`
  controllers (DS4 v1/v2, DualSense, DualSense Edge) via `input` group +
  `uaccess`; `/dev/uinput` for the `uinput` group.
- `packaging/linux/ds4cc.service` — `systemd --user` unit
  (`ExecStart=%h/.local/bin/ds4cc`, `Restart=on-failure`).
- `packaging/linux/ds4cc.desktop` — XDG autostart fallback.
- `packaging/linux/install.sh` — POSIX sh, `set -eu`; detects apt vs pacman,
  installs runtime deps, creates the `uinput` group, installs the udev rule,
  installs the binary to `~/.local/bin`, enables the user unit, and prints
  re-login + Bluetooth pairing (PS + Share) instructions.
