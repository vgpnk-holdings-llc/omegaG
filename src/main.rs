// The codex session-polling + status→LED layer is Windows-only (SPEC §1).
#[cfg(windows)]
mod codex_micro;
#[cfg(windows)]
mod codex_protocol;
#[cfg(windows)]
mod codex_runtime;
#[cfg(windows)]
mod codex_voice;
mod config;
mod controller;
mod crc32;
mod detect;
mod hid;
mod input;
mod keys;
mod launcher;
mod mapper;
mod mic;
mod output;
mod platform;
mod state;
mod tmux_detect;
mod tray;
mod update;
// wsl.rs carries an inner #![cfg(windows)] — compiled out on Linux.
mod wsl;

use crate::controller::ConnectionType;
use crate::output::OutputState;

use std::sync::Arc;
#[cfg(windows)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tokio::time::{Duration, sleep};

const ORDINARY_ACTION_LIMIT: usize = 28;

#[derive(Debug)]
enum EnqueueError {
    Full,
    Closed,
}

fn enqueue_action(
    tx: &tokio::sync::mpsc::UnboundedSender<mapper::Action>,
    ordinary_pending: &std::sync::atomic::AtomicUsize,
    action: mapper::Action,
) -> Result<(), EnqueueError> {
    let ordinary = !action.is_safety_release();
    if ordinary
        && ordinary_pending
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |pending| {
                (pending < ORDINARY_ACTION_LIMIT).then_some(pending + 1)
            })
            .is_err()
    {
        return Err(EnqueueError::Full);
    }
    tx.send(action).map_err(|error| {
        if ordinary {
            ordinary_pending.fetch_sub(1, Ordering::AcqRel);
        }
        let _ = error;
        EnqueueError::Closed
    })
}

/// Codex semantic input consumes controller buttons/sticks/touch (Windows-only).
#[cfg(windows)]
fn suppress_semantic_input(input: &mut input::UnifiedInput) {
    input.buttons = input::ButtonState::default();
    input.left_stick = (128, 128);
    input.right_stick = (128, 128);
    input.touchpad = [input::TouchPoint::default(); 2];
}

/// Parsed command-line flags (SPEC §9 — minimal, hand-rolled, no clap).
struct CliFlags {
    verbose: bool,
    no_tray: bool,
}

fn print_usage() {
    println!(
        "ds4cc {version} — DualSense/DS4 shortcut-mapper daemon\n\
         \n\
         USAGE:\n\
         \x20   ds4cc [OPTIONS]\n\
         \n\
         OPTIONS:\n\
         \x20   -h, --help       Print this help and exit\n\
         \x20   -V, --version    Print version and exit\n\
         \x20   -v, --verbose    Debug-level logging\n\
         \x20       --no-tray    Run without the system tray icon",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn parse_cli() -> CliFlags {
    let mut flags = CliFlags {
        verbose: false,
        no_tray: false,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("ds4cc {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-v" | "--verbose" => flags.verbose = true,
            "--no-tray" => flags.no_tray = true,
            other => {
                eprintln!("ds4cc: unknown argument '{other}'\n");
                print_usage();
                std::process::exit(2);
            }
        }
    }
    flags
}

#[tokio::main]
async fn main() {
    let cli = parse_cli();
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if cli.verbose { "debug" } else { "info" }),
    )
    .format(|buf, record| {
        use std::io::Write;
        let ts = buf.timestamp_millis();
        // Compact: "10:30:45.123 INFO  message"
        // Strip date prefix — only keep HH:MM:SS.mmm
        let ts_str = ts.to_string();
        let time_part = ts_str.split('T').nth(1).unwrap_or(&ts_str);
        let time_part = time_part.trim_end_matches('Z');
        write!(
            buf,
            "{time_part} {:<5} {}\r\n",
            record.level(),
            record.args()
        )
    })
    .init();

    // Hide console window immediately — app runs as a tray icon.
    // Logs still accumulate; user can show the console via tray menu.
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    log::info!("DS4CC v3 starting...");

    let mut cfg = config::Config::load();
    cfg.codex_micro.normalize();

    // ── Codex runtime (Windows-only; SPEC §1) ─────────────────────────────
    #[cfg(windows)]
    let codex_state = {
        let mut initial_codex_state = codex_micro::CodexMicro::default();
        initial_codex_state.configure_sources(
            codex_micro::SourcePolicy::parse(&cfg.codex_micro.source_policy),
            cfg.codex_micro.custom_order.clone(),
        );
        Arc::new(Mutex::new(initial_codex_state))
    };
    #[cfg(windows)]
    let (codex_runtime, codex_events) = if cfg.codex_micro.enabled {
        let (event_tx, event_rx) = std::sync::mpsc::sync_channel(128);
        (
            Some(codex_runtime::RuntimeHandle::spawn(
                cfg.codex_micro.clone(),
                event_tx,
            )),
            Some(event_rx),
        )
    } else {
        (None, None)
    };
    #[cfg(windows)]
    if let Some(events) = codex_events {
        let state = Arc::clone(&codex_state);
        let _ = std::thread::Builder::new()
            .name("codex-events".into())
            .spawn(move || {
                let mut epoch = 0;
                while let Ok(event) = events.recv() {
                    let mut state = state.lock().expect("codex state poisoned");
                    if event.connection_generation != epoch {
                        if state.begin_generation(event.connection_generation).is_err() {
                            continue;
                        }
                        epoch = event.connection_generation;
                    }
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let _ = state.reduce(event, now);
                }
            });
    }
    #[cfg(target_os = "linux")]
    if cfg.codex_micro.enabled {
        log::warn!("codex runtime is Windows-only; ignored");
    }

    #[cfg(windows)]
    let started = std::time::Instant::now();

    // Detect keybinds (tmux prefix + bindings, Claude Code keybindings.json)
    // in a single WSL round-trip. Best-effort — defaults cover any gap.
    let detected = detect::detect();

    // Shared mouse mode toggle: false = touchpad, true = left stick.
    // Owned here; cloned into tray thread and each input loop iteration.
    let mouse_stick_active = Arc::new(AtomicBool::new(false));

    // Create the input injector up front. Degraded-ok: if the OS injection
    // device is unavailable (Linux: /dev/uinput), the daemon keeps running
    // and injection calls become logged no-ops (SPEC §4).
    mapper::init_injector();

    // Tray icon — skipped with --no-tray, and auto-skipped on Linux when no
    // D-Bus session bus is reachable (headless; SPEC §9).
    #[cfg(target_os = "linux")]
    let no_tray = {
        let dbus = tray::session_bus_available();
        if !cli.no_tray && !dbus {
            log::info!("No D-Bus session bus reachable — running without tray icon");
        }
        cli.no_tray || !dbus
    };
    #[cfg(windows)]
    let no_tray = cli.no_tray;
    let tray_tx = if no_tray {
        None
    } else {
        Some(tray::spawn(Arc::clone(&mouse_stick_active)))
    };

    // Initialize HID
    let mut api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            log::error!("Failed to initialize HID API: {e}");
            std::process::exit(1);
        }
    };

    // Main connection loop — reconnects on disconnect
    loop {
        // Find controller (USB priority: find_all_controllers returns USB first)
        let (info, device, bt_paired) = loop {
            if let Err(e) = api.refresh_devices() {
                log::debug!("HID refresh failed: {e}");
            }
            let all = hid::find_all_controllers(&api);
            let has_bt = all
                .iter()
                .any(|c| c.connection_type == ConnectionType::Bluetooth);
            match all.into_iter().next() {
                Some(info) => match hid::open_device(&api, &info) {
                    Ok(dev) => break (info, dev, has_bt),
                    Err(e) => {
                        log::warn!("Found controller but failed to open: {e}");
                    }
                },
                None => {
                    log::info!("No controller found. Retrying in 2s...");
                }
            }
            sleep(Duration::from_secs(2)).await;
        };

        log::info!(
            "Connected: {} ({})",
            info.controller_type,
            info.connection_type
        );
        if bt_paired && info.connection_type == ConnectionType::Usb {
            log::info!("Bluetooth also paired — will serve as fallback if USB is disconnected");
        }

        // Activate BT extended mode if needed
        if info.connection_type == ConnectionType::Bluetooth
            && let Err(e) = hid::activate_bt_extended_mode(&device, info.controller_type)
        {
            log::error!("Failed to activate BT extended mode: {e}");
            log::error!("Controller may not work correctly over Bluetooth.");
        }

        // Stick mouse on for both DualSense and DS4 (precise cursor). Touchpad
        // swipe is independent (fast cursor) when the pad reports contacts.
        // Tray can still mute the stick via SetStickMode(false).
        if let Some(tx) = &tray_tx {
            let _ = tx.send(tray::TrayCmd::SetStickMode(true));
        }

        let handle = hid::HidHandle::new(device);
        let ct = info.controller_type;
        let conn = info.connection_type;

        // If connected over Bluetooth, spawn a background USB scanner thread.
        // It sets `usb_available` when a USB controller appears so the input loop
        // can exit and the main loop re-scans (picking USB with higher priority).
        let (usb_available, scanner_stop): (Option<Arc<AtomicBool>>, Option<Arc<AtomicBool>>) =
            if conn == ConnectionType::Bluetooth {
                let flag = Arc::new(AtomicBool::new(false));
                let stop = Arc::new(AtomicBool::new(false));
                let flag_clone = Arc::clone(&flag);
                let stop_clone = Arc::clone(&stop);
                let spawn_result = std::thread::Builder::new()
                    .name("usb-scanner".into())
                    .spawn(move || {
                        let Ok(mut scanner_api) = hidapi::HidApi::new() else {
                            log::warn!("USB scanner: failed to create HidApi instance");
                            return;
                        };
                        loop {
                            std::thread::sleep(std::time::Duration::from_secs(5));
                            if stop_clone.load(Ordering::Relaxed) {
                                log::debug!("USB scanner: stop signal received");
                                return;
                            }
                            if let Err(e) = scanner_api.refresh_devices() {
                                log::debug!("USB scanner refresh failed: {e}");
                                continue;
                            }
                            if hid::has_usb_controller(&scanner_api) {
                                log::info!(
                                    "USB scanner: USB controller detected, signaling switch"
                                );
                                flag_clone.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                    });
                if let Err(e) = spawn_result {
                    // Scanner failed to start: the flag simply never fires, so the
                    // app stays on Bluetooth. Surface it rather than dropping silently.
                    log::warn!("USB scanner: failed to spawn scanner thread: {e}");
                }
                (Some(flag), Some(stop))
            } else {
                (None, None)
            };

        // Shared DualSense player-indicator LEDs (profile P1–P4). Input loop
        // writes on PS cycle; output loop reads every refresh — original design.
        let player_leds = Arc::new(AtomicU8::new(mapper::PROFILE_PLAYER_LEDS[0]));

        // Spawn output loop for this connection
        let output_handle = handle.clone_handle();
        let lightbar_color = cfg.lightbar.clone();
        let player_leds_out = Arc::clone(&player_leds);
        #[cfg(windows)]
        let output_task = {
            let codex_output = Arc::clone(&codex_state);
            let codex_cfg = cfg.codex_micro.clone();
            let runtime_view = codex_runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.view));
            tokio::spawn(async move {
                run_output_loop(
                    output_handle,
                    ct,
                    conn,
                    lightbar_color,
                    codex_output,
                    codex_cfg,
                    runtime_view,
                    started,
                    player_leds_out,
                )
                .await;
            })
        };
        #[cfg(not(windows))]
        let output_task = tokio::spawn(async move {
            run_output_loop(output_handle, ct, conn, lightbar_color, player_leds_out).await;
        });

        // Run input loop — returns when device disconnects or USB scanner signals
        #[cfg(windows)]
        run_input_loop(
            handle,
            ct,
            conn,
            &cfg,
            &detected,
            Arc::clone(&mouse_stick_active),
            usb_available.clone(),
            Arc::clone(&codex_state),
            started,
            codex_runtime
                .as_ref()
                .map(codex_runtime::RuntimeHandle::transport),
            codex_runtime
                .as_ref()
                .map_or(0, |runtime| runtime.epoch.load(Ordering::Acquire)),
            Arc::clone(&player_leds),
        )
        .await;
        #[cfg(not(windows))]
        run_input_loop(
            handle,
            ct,
            conn,
            &cfg,
            &detected,
            Arc::clone(&mouse_stick_active),
            usb_available.clone(),
            Arc::clone(&player_leds),
        )
        .await;

        // Input loop exited — cancel output task and stop USB scanner
        output_task.abort();
        if let Some(ref stop) = scanner_stop {
            stop.store(true, Ordering::Relaxed);
        }

        // Determine why the input loop exited and reconnect accordingly
        let switching_to_usb = usb_available
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Relaxed));

        if switching_to_usb {
            log::info!("Switching to USB controller...");
            // No sleep — USB is already present, re-scan will find it immediately
        } else if conn == ConnectionType::Usb {
            log::info!("USB disconnected. Scanning for Bluetooth fallback...");
            sleep(Duration::from_millis(200)).await;
        } else {
            log::info!("Controller disconnected. Scanning for new connection...");
            sleep(Duration::from_secs(1)).await;
        }
    }
}

/// Input loop: read HID reports, parse, map to keystrokes.
/// Returns when the device disconnects or `usb_switch_flag` is set (BT→USB switch).
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
async fn run_input_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    cfg: &config::Config,
    detected: &detect::Detected,
    mouse_stick_active: Arc<AtomicBool>,
    usb_switch_flag: Option<Arc<AtomicBool>>,
    codex_state: Arc<Mutex<codex_micro::CodexMicro>>,
    started: std::time::Instant,
    codex_transport: Option<codex_runtime::RuntimeTransport>,
    codex_generation: u64,
    player_leds: Arc<AtomicU8>,
) {
    let mut mapper_state = mapper::MapperState::new(cfg, detected, mouse_stick_active);
    player_leds.store(mapper_state.profile_led_mask(), Ordering::Relaxed);
    let mut last_profile = mapper_state.profile();
    let transport: Box<dyn codex_micro::CodexTransport> = match codex_transport {
        Some(transport) => Box::new(transport),
        None => Box::new(codex_micro::UnavailableTransport),
    };
    let mut codex_dispatcher = codex_micro::Dispatcher::new(transport, codex_generation);
    // Semantic releases never enter the bounded/droppable generic action queue.
    // Dispatch them synchronously at the transport boundary before reading input.
    let reconnect_actions = codex_state
        .lock()
        .expect("codex state poisoned")
        .reconnect();
    for action in reconnect_actions {
        let result = codex_dispatcher.dispatch(action);
        log::warn!("Codex Micro reconnect release: {result:?}");
    }
    let mut consecutive_errors = 0u32;
    let mut first_report = true;
    let mut last_mute = false;
    let mut running = true;
    // FIFO queue: ordinary traffic is admission-limited without blocking HID.
    // Safety KeyUp releases bypass that limit and therefore cannot be dropped
    // by motion/repeat saturation.
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<mapper::Action>();
    let ordinary_pending = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker_pending = Arc::clone(&ordinary_pending);
    let action_worker = tokio::spawn(async move {
        while let Some(action) = action_rx.recv().await {
            if !action.is_safety_release() {
                worker_pending.fetch_sub(1, Ordering::AcqRel);
            }
            // execute_action blocks (SendInput / process spawn + the 16 ms Enter
            // guard sleep). Run it on the blocking pool and await so the async
            // runtime is never starved; awaiting one action at a time preserves
            // exact FIFO execution order.
            if let Err(e) =
                tokio::task::spawn_blocking(move || mapper::execute_action(&action)).await
            {
                log::warn!("action worker: execution task failed to join: {e}");
            }
        }
    });

    while running {
        let read_result = handle.read().await;
        match read_result {
            Err(()) => {
                // Device disconnected
                break;
            }
            Ok(data) if data.is_empty() => {
                // No data available — yield and retry
                sleep(Duration::from_millis(4)).await;
                consecutive_errors = 0;

                // Check if USB scanner detected a USB controller (BT→USB switch)
                if let Some(ref flag) = usb_switch_flag
                    && flag.load(Ordering::Relaxed)
                {
                    log::info!("USB controller available — switching from Bluetooth");
                    break;
                }

                continue;
            }
            Ok(data) => {
                let n = data.len();

                if first_report {
                    let hex: Vec<String> =
                        data.iter().take(16).map(|b| format!("{b:02X}")).collect();
                    log::info!("First report ({n} bytes): {}", hex.join(" "));
                    first_report = false;
                }

                // State reading only: BT CRC + report parse → UnifiedInput.
                // Mapper never sees raw HID / connection type.
                let link = state::ControllerLink::new(ct, conn);
                match state::decode_report(link, &data) {
                    Ok(mut unified) => {
                        consecutive_errors = 0;
                        let now_ms = started.elapsed().as_millis() as u64;
                        let (semantic_actions, consumed) = {
                            let mut state = codex_state.lock().expect("codex state poisoned");
                            let (actions, consumed) =
                                state.update_input(&unified, now_ms, &cfg.codex_micro);
                            (actions, consumed)
                        };
                        for action in semantic_actions {
                            let result = codex_dispatcher.dispatch(action);
                            match &result {
                                codex_micro::DispatchResult::Applied(id) => {
                                    log::info!("Codex Micro applied request {}", id.request_id)
                                }
                                codex_micro::DispatchResult::Rejected { error, .. } => {
                                    log::warn!(
                                        "Codex runtime rejected a controller request: {error:?}"
                                    )
                                }
                            }
                            codex_state
                                .lock()
                                .expect("codex state poisoned")
                                .mark_dispatch(&result, now_ms);
                        }
                        if consumed {
                            suppress_semantic_input(&mut unified);
                        }
                        let actions = mapper_state.update(&unified);
                        let current_profile = mapper_state.profile();
                        if current_profile != last_profile {
                            player_leds.store(mapper_state.profile_led_mask(), Ordering::Relaxed);
                            last_profile = current_profile;
                        }
                        for action in actions {
                            log::debug!("Action: {action:?}");
                            match enqueue_action(&action_tx, &ordinary_pending, action) {
                                Ok(_) => {}
                                Err(EnqueueError::Full) => {
                                    log::warn!("Action queue full — dropping action");
                                }
                                Err(EnqueueError::Closed) => {
                                    log::warn!("Action worker dropped; stopping input loop");
                                    running = false;
                                    break;
                                }
                            }
                        }
                        if !running {
                            break;
                        }

                        // Mute is a state side-effect, not a shortcut map.
                        let mute_now = unified.buttons.mute;
                        if ct.is_dualsense() && mute_now && !last_mute {
                            tokio::task::spawn_blocking(mic::toggle_mute);
                        }
                        last_mute = mute_now;
                    }
                    Err(state::DecodeError::BtCrcInvalid) => {
                        consecutive_errors += 1;
                        if consecutive_errors % 100 == 1 {
                            log::warn!("BT CRC validation failed ({consecutive_errors} times)");
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors % 100 == 1 {
                            log::warn!("Input decode error ({consecutive_errors}): {e}");
                        }
                    }
                }
            }
        }
    }
    // A PTT stop is safety-critical and bypasses the generic action queue.
    let stop_actions = codex_state
        .lock()
        .expect("codex state poisoned")
        .reconnect();
    for action in stop_actions {
        let result = codex_dispatcher.dispatch(action);
        log::warn!("Codex Micro disconnect release: {result:?}");
    }
    // Safety: release L2-held modifiers if the link drops mid-hold.
    for action in mapper_state.force_release_holds() {
        let _ = enqueue_action(&action_tx, &ordinary_pending, action);
    }
    drop(action_tx);
    action_worker.await.ok();
}

/// Output loop: keep the static lightbar color, player LED, and mic-mute LED updated.
/// Windows: overlays codex runtime/session status onto lightbar + rumble.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
async fn run_output_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    lightbar_color: config::ColorConfig,
    codex_state: Arc<Mutex<codex_micro::CodexMicro>>,
    codex_cfg: config::CodexMicroConfig,
    runtime_view: Option<Arc<Mutex<codex_runtime::RuntimeView>>>,
    started: std::time::Instant,
    player_leds: Arc<AtomicU8>,
) {
    let mut bt_seq = 0u8;

    // Prime mic mute state from system before first frame
    tokio::task::spawn_blocking(mic::init).await.ok();

    // The output report is static except for the mute LED, but resending
    // periodically keeps the controller state correct after wake/reconnect blips.
    let mut ticker = tokio::time::interval(Duration::from_millis(100));

    loop {
        ticker.tick().await;
        let now_ms = started.elapsed().as_millis() as u64;
        let (mut rgb, pending) = {
            let state = codex_state.lock().expect("codex state poisoned");
            (
                codex_micro::compose_rgb(
                    &state,
                    &codex_cfg,
                    [lightbar_color.r, lightbar_color.g, lightbar_color.b],
                    now_ms,
                ),
                state.slots[state.selected]
                    .thread
                    .as_ref()
                    .is_some_and(|t| t.status == codex_micro::ChatStatus::RequiresInput),
            )
        };
        let (connected, voice, fast) = runtime_view
            .as_ref()
            .map(|view| {
                let view = view.lock().expect("runtime view poisoned");
                (view.connected, view.voice, view.fast)
            })
            .unwrap_or((true, codex_runtime::VoiceState::Idle, false));
        if codex_cfg.enabled && !connected {
            rgb = codex_micro::ChatStatus::Error.color();
        }
        if voice == codex_runtime::VoiceState::Capturing {
            rgb = [180, 0, 255];
        }
        if voice == codex_runtime::VoiceState::Finalizing {
            rgb = [0, 220, 220];
        }
        let pulse = (now_ms / 250).is_multiple_of(2);
        let out = OutputState {
            lightbar_r: rgb[0],
            lightbar_g: rgb[1],
            lightbar_b: rgb[2],
            rumble_left: if pending && pulse { 42 } else { 0 },
            rumble_right: if voice == codex_runtime::VoiceState::Finalizing && pulse {
                28
            } else {
                0
            },
            // Profile LEDs win over "fast" all-on unless we want all five as P4.
            player_leds: player_leds.load(Ordering::Relaxed),
            mute_led: mic::MIC_MUTED.load(Ordering::Relaxed) as u8,
        };
        let _ = fast; // codex fast path no longer overrides profile LEDs
        let report = output::build_report(ct, conn, &out, &mut bt_seq);
        handle.write(report).await;
    }
}

/// Input loop (Linux): read HID reports, parse, map to keystrokes.
/// No codex semantic layer — identical to the Windows loop minus codex dispatch.
/// Returns when the device disconnects or `usb_switch_flag` is set (BT→USB switch).
#[cfg(not(windows))]
async fn run_input_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    cfg: &config::Config,
    detected: &detect::Detected,
    mouse_stick_active: Arc<AtomicBool>,
    usb_switch_flag: Option<Arc<AtomicBool>>,
    player_leds: Arc<AtomicU8>,
) {
    let mut mapper_state = mapper::MapperState::new(cfg, detected, mouse_stick_active);
    player_leds.store(mapper_state.profile_led_mask(), Ordering::Relaxed);
    let mut last_profile = mapper_state.profile();
    let mut consecutive_errors = 0u32;
    let mut first_report = true;
    let mut last_mute = false;
    let mut running = true;
    // FIFO queue: ordinary traffic is admission-limited without blocking HID.
    // Safety KeyUp releases bypass that limit and therefore cannot be dropped
    // by motion/repeat saturation.
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<mapper::Action>();
    let ordinary_pending = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker_pending = Arc::clone(&ordinary_pending);
    let action_worker = tokio::spawn(async move {
        while let Some(action) = action_rx.recv().await {
            if !action.is_safety_release() {
                worker_pending.fetch_sub(1, Ordering::AcqRel);
            }
            // execute_action blocks (uinput emit / process spawn + the 16 ms Enter
            // guard sleep). Run it on the blocking pool and await so the async
            // runtime is never starved; awaiting one action at a time preserves
            // exact FIFO execution order.
            if let Err(e) =
                tokio::task::spawn_blocking(move || mapper::execute_action(&action)).await
            {
                log::warn!("action worker: execution task failed to join: {e}");
            }
        }
    });

    while running {
        let read_result = handle.read().await;
        match read_result {
            Err(()) => {
                // Device disconnected
                break;
            }
            Ok(data) if data.is_empty() => {
                // No data available — yield and retry
                sleep(Duration::from_millis(4)).await;
                consecutive_errors = 0;

                // Check if USB scanner detected a USB controller (BT→USB switch)
                if let Some(ref flag) = usb_switch_flag
                    && flag.load(Ordering::Relaxed)
                {
                    log::info!("USB controller available — switching from Bluetooth");
                    break;
                }

                continue;
            }
            Ok(data) => {
                let n = data.len();

                if first_report {
                    let hex: Vec<String> =
                        data.iter().take(16).map(|b| format!("{b:02X}")).collect();
                    log::info!("First report ({n} bytes): {}", hex.join(" "));
                    first_report = false;
                }

                // State reading only: BT CRC + report parse → UnifiedInput.
                // Mapper never sees raw HID / connection type.
                let link = state::ControllerLink::new(ct, conn);
                match state::decode_report(link, &data) {
                    Ok(unified) => {
                        consecutive_errors = 0;
                        let actions = mapper_state.update(&unified);
                        // Instant profile LED update (original e62224e design).
                        let current_profile = mapper_state.profile();
                        if current_profile != last_profile {
                            player_leds.store(mapper_state.profile_led_mask(), Ordering::Relaxed);
                            last_profile = current_profile;
                        }
                        for action in actions {
                            log::debug!("Action: {action:?}");
                            match enqueue_action(&action_tx, &ordinary_pending, action) {
                                Ok(_) => {}
                                Err(EnqueueError::Full) => {
                                    log::warn!("Action queue full — dropping action");
                                }
                                Err(EnqueueError::Closed) => {
                                    log::warn!("Action worker dropped; stopping input loop");
                                    running = false;
                                    break;
                                }
                            }
                        }
                        if !running {
                            break;
                        }

                        // Mute is a state side-effect, not a shortcut map.
                        let mute_now = unified.buttons.mute;
                        if ct.is_dualsense() && mute_now && !last_mute {
                            tokio::task::spawn_blocking(mic::toggle_mute);
                        }
                        last_mute = mute_now;
                    }
                    Err(state::DecodeError::BtCrcInvalid) => {
                        consecutive_errors += 1;
                        if consecutive_errors % 100 == 1 {
                            log::warn!("BT CRC validation failed ({consecutive_errors} times)");
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors % 100 == 1 {
                            log::warn!("Input decode error ({consecutive_errors}): {e}");
                        }
                    }
                }
            }
        }
    }
    // Safety: release L2-held modifiers if the link drops mid-hold.
    for action in mapper_state.force_release_holds() {
        let _ = enqueue_action(&action_tx, &ordinary_pending, action);
    }
    drop(action_tx);
    action_worker.await.ok();
}

/// Output loop (Linux): static [lightbar] color, player LED, mic-mute LED.
/// No codex status→LED projection (Windows-only, SPEC §1/§5).
#[cfg(not(windows))]
async fn run_output_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    lightbar_color: config::ColorConfig,
    player_leds: Arc<AtomicU8>,
) {
    let mut bt_seq = 0u8;

    // Prime mic mute state from system before first frame
    tokio::task::spawn_blocking(mic::init).await.ok();

    // The output report is static except for mute + profile LEDs, but resending
    // periodically keeps the controller state correct after wake/reconnect blips.
    let mut ticker = tokio::time::interval(Duration::from_millis(100));

    loop {
        ticker.tick().await;
        let out = OutputState {
            lightbar_r: lightbar_color.r,
            lightbar_g: lightbar_color.g,
            lightbar_b: lightbar_color.b,
            rumble_left: 0,
            rumble_right: 0,
            player_leds: player_leds.load(Ordering::Relaxed),
            mute_led: mic::MIC_MUTED.load(Ordering::Relaxed) as u8,
        };
        let report = output::build_report(ct, conn, &out, &mut bt_seq);
        handle.write(report).await;
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::suppress_semantic_input;
    use super::{ORDINARY_ACTION_LIMIT, enqueue_action};
    use crate::mapper::{Action, VKey};
    /// Regression for the action worker: draining the bounded FIFO and awaiting
    /// each `spawn_blocking` execution one at a time must preserve exact input
    /// order. This mirrors the `action_worker` loop structure (recv → await
    /// spawn_blocking → recv) that keeps blocking execution off the async runtime
    /// without reordering actions.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_blocking_worker_preserves_fifo_order() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<usize>(32);
        for i in 0..16 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let mut executed = Vec::new();
        while let Some(item) = rx.recv().await {
            // Await each blocking unit before receiving the next — exact ordering.
            let out = tokio::task::spawn_blocking(move || item * 10)
                .await
                .unwrap();
            executed.push(out);
        }

        let expected: Vec<usize> = (0..16).map(|i| i * 10).collect();
        assert_eq!(executed, expected, "FIFO order must be preserved exactly");
    }

    #[tokio::test]
    async fn saturated_motion_queue_retains_release_reserve() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let pending = std::sync::atomic::AtomicUsize::new(0);
        let mut dropped = false;
        for _ in 0..=ORDINARY_ACTION_LIMIT {
            if enqueue_action(&tx, &pending, Action::MouseMove { dx: 1, dy: 1 }).is_err() {
                dropped = true;
                break;
            }
        }
        assert!(dropped, "ordinary motion must stop before the reserve");
        enqueue_action(&tx, &pending, Action::KeyUp(vec![VKey::Control])).unwrap();
        drop(tx);
        let mut saw_release = false;
        while let Some(action) = rx.recv().await {
            saw_release |= action.is_safety_release();
        }
        assert!(saw_release);
    }

    #[cfg(windows)]
    #[test]
    fn exclusive_semantic_input_suppresses_buttons_sticks_and_touch() {
        let mut input = crate::input::UnifiedInput::default();
        input.buttons.cross = true;
        input.buttons.mute = true;
        input.left_stick = (0, 255);
        input.right_stick = (255, 0);
        input.touchpad[0] = crate::input::TouchPoint {
            active: true,
            x: 900,
            y: 500,
        };
        suppress_semantic_input(&mut input);
        assert_eq!(input.buttons, crate::input::ButtonState::default());
        assert_eq!(input.left_stick, (128, 128));
        assert_eq!(input.right_stick, (128, 128));
        assert_eq!(input.touchpad, [crate::input::TouchPoint::default(); 2]);
    }
}
