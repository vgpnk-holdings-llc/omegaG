# RECON.md — omegaG (ds4cc 3.1.0) Code Reconnaissance

Source: github.com/vgpnk-holdings-llc/omegaG @ commit `bbaa2f7542ab01299e198d48831ef223009f4294`
Mirrored to: `/mnt/agents/output/omegag-port/repo/` (all files SHA-1-verified against git blob shas)

## 1. Session-status → lightbar RGB projection (symbol list)

### src/codex_micro.rs
- `enum ChatStatus { Unassigned, Idle, Thinking, CompleteUnread, RequiresInput, Error }`
- `ChatStatus::color(self) -> [u8; 3]` — status→RGB: Idle `[255,255,255]`, Thinking `[59,130,246]`, CompleteUnread `[34,197,94]`, RequiresInput `[245,158,11]`, Error `[239,68,68]`, Unassigned `[0,0,0]`
- `struct ThreadRecord { context, status: ChatStatus, updated_ms, pinned, priority }`
- `struct CodexMicro { slots: [ChatSlot; SLOT_COUNT=6], selected, last_activity_ms, transport_degraded, ... }`
- `CodexMicro::rgb(&self, now_ms: u64, cfg: &CodexMicroConfig) -> [u8; 3]` — selected-slot status (or Error if `transport_degraded`), 500 ms pulse (100%/55%), `cfg.brightness` scale, `[0,0,0]` after `cfg.inactivity_seconds`
- `compose_rgb(state: &CodexMicro, cfg: &CodexMicroConfig, legacy: [u8;3], now_ms: u64) -> [u8; 3]` — runtime_active ? state.rgb() : legacy lightbar
- `CodexMicro::reduce(event: CodexEvent, now_ms) -> Result<(), TransportError>` — applies status updates
- `CodexMicro::mark_dispatch(&DispatchResult, now_ms)` — sets `transport_degraded` → red
- `CodexMicro::begin_generation(generation)` — epoch gate
- `enum CodexEventKind { Snapshot{..}, Upsert, SelectUpsert, StatusById{thread_id,status,updated_ms}, DisarmApproval, Remove }`
- `struct CodexEvent { connection_generation, sequence, kind }`

### src/codex_runtime.rs
- `struct RuntimeView { connected, ready, server_epoch, last_error, models_loaded, threads_loaded, skills_loaded, fast, efforts, effort_index, composer, voice: VoiceState }`
- `enum VoiceState { Idle, Capturing, Finalizing }`
- `status_from_thread(status: Option<&Value>) -> ChatStatus` — wire `notLoaded/idle/systemError/active(+activeFlags waitingOnApproval|waitingOnUserInput)` → ChatStatus
- `turn_completion_status(params: &Value) -> ChatStatus` — `completed→CompleteUnread`, `failed→Error`, `interrupted→Idle`
- `emit(conn, events, kind: CodexEventKind)` — generation+sequence-tagged event send
- `notification(...)` — maps `turn/started`, `turn/completed`, `error`, `thread/status/changed`, `thread/started|deleted|archived`, `serverRequest/resolved` → CodexEventKind
- `RuntimeHandle { view: Arc<Mutex<RuntimeView>>, epoch: Arc<AtomicU64>, ... }`, `RuntimeHandle::spawn(cfg, events)`
- `set_disconnected(view, error)` — connected=false (drives main.rs red override)

### src/main.rs
- `run_output_loop(handle, ct, conn, lightbar_color, codex_state, codex_cfg, runtime_view, started)` — the ONLY HID lightbar writer (100 ms tick):
  - `rgb = codex_micro::compose_rgb(&state, &codex_cfg, [lightbar r,g,b], now_ms)`
  - override: `if codex_cfg.enabled && !connected { rgb = ChatStatus::Error.color() }`
  - override: `voice == Capturing → [180,0,255]`; `voice == Finalizing → [0,220,220]`
  - `pending` = selected slot `RequiresInput` → 250 ms pulse `rumble_left = 42`; Finalizing+pulse → `rumble_right = 28`
  - `player_leds = if fast { 0x1f } else { PLAYER_LEDS (0x04) }`; `mute_led = mic::MIC_MUTED`
  - builds `OutputState` → `output::build_report(...)` → `handle.write(report).await`
- `"codex-events"` thread in `main()`: `state.begin_generation(...)` / `state.reduce(event, now)`
- `PLAYER_LEDS: u8 = 0x04`

### src/output.rs
- `struct OutputState { lightbar_r, lightbar_g, lightbar_b, rumble_left, rumble_right, player_leds, mute_led }`
- `build_report(ct, conn, state, bt_seq) -> Vec<u8>`
- `build_dualsense_usb` (RGB at bytes 45-47), `build_dualsense_bt` (RGB at 46-48 + CRC), `build_ds4_usb` (RGB at 6-8), `build_ds4_bt` (RGB at 8-10 + CRC)

### Data flow
app-server frames → `codex_runtime::notification/handle_frame` → `emit(CodexEventKind)` → mpsc → main `"codex-events"` thread → `CodexMicro::reduce` → slots[].status → `run_output_loop` → `compose_rgb` (+runtime overrides) → `OutputState` → `build_report` → HID write

## 2. Test inventory (#[test] functions)

| File | Tests | Notes |
|---|---|---|
| src/mapper.rs | 43 + 17 | 17 in `linux_inject_tests` gated `cfg(all(test, target_os="linux"))` |
| src/codex_runtime.rs | 19 + 1 | +1 `live_codex_read_only_smoke` is `#[ignore]`; 2 tests unix-gated |
| src/launcher.rs | 18 | |
| src/codex_micro.rs | 15 | |
| src/tmux_detect.rs | 23 | |
| src/input.rs | 9 | |
| src/codex_protocol.rs | 8 | |
| src/config.rs | 8 | |
| src/output.rs | 6 | |
| src/detect.rs | 5 | |
| src/codex_voice.rs | 4 | 3 unix-gated |
| src/hid.rs | 4 | |
| src/controller.rs | 4 | |
| src/tray.rs | 4 | 3 gated `cfg(not(windows))` |
| src/update.rs | 4 | 2 gated `cfg(not(windows))` |
| src/crc32.rs | 3 | |
| src/main.rs | 3 | 1 is `#[tokio::test(flavor="multi_thread")]`, 1 `#[tokio::test]` |
| **Total** | **198** | 197 active + 1 ignored; ~26 platform-gated subset |

Counts verified by summing per-file inventories above (mapper 60 = 43 core + 17 linux-gated; runtime 20 = 19 + 1 ignored).

## 3. Dependency versions (from Cargo.lock)

Direct deps of `ds4cc` package:
- hidapi **2.6.4** (features: windows-native)
- tokio **1.49.0** (rt-multi-thread, time, macros, sync)
- tray-icon **0.21.3** (pulls muda 0.17.1, gtk 0.18.2 on Linux)
- windows-sys **0.59.0** + windows **0.58.0**
- serde **1.0.228**, serde_json **1.0.149**, toml **0.8.2**
- log **0.4.29**, env_logger **0.11.9**, libc **0.2.182**
- image **0.25.9** (png only), ureq **3.2.0** (rustls 0.23.37)

Package: `ds4cc` **3.1.0**, Rust edition 2024.

## 4. Mirror status

Complete tree mirrored & verified: 19 src/*.rs, root files (Cargo.toml/lock, README, HIGHLIGHTS, LICENSE, build.rs, .gitignore, .gitattributes), assets/ (ds4cc.rc, ds4cc.res, icon.ico), docs/, installer/ds4cc.iss, imgs/ (9 files incl. tray icon PNG required by `tray.rs` `include_bytes!`).
Note: `imgs/ChatGPT Image Feb 23, 2026, 06_05_18 AM.svg` is a byte-identical copy of the 05_59_26 PNG (same git blob sha) — upstream misnaming, mirrored as-is.
