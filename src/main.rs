mod config;
mod controller;
mod crc32;
mod detect;
mod hid;
mod input;
mod mapper;
mod mic;
mod output;
mod tmux_detect;
mod tray;
mod update;
mod wsl;

use crate::controller::ConnectionType;
use crate::output::OutputState;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let ts = buf.timestamp_millis();
            // Compact: "10:30:45.123 INFO  message"
            // Strip date prefix — only keep HH:MM:SS.mmm
            let ts_str = ts.to_string();
            let time_part = ts_str.split('T').nth(1).unwrap_or(&ts_str);
            let time_part = time_part.trim_end_matches('Z');
            write!(buf, "{time_part} {:<5} {}\r\n", record.level(), record.args())
        })
        .init();

    // Hide console window immediately — app runs as a tray icon.
    // Logs still accumulate; user can show the console via tray menu.
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    log::info!("DS4CC v3 starting...");

    let cfg = config::Config::load();

    // Detect keybinds (tmux prefix + bindings, Claude Code keybindings.json)
    // in a single WSL round-trip. Best-effort — defaults cover any gap.
    let detected = detect::detect();

    // Shared mouse mode toggle: false = touchpad, true = left stick.
    // Owned here; cloned into tray thread and each input loop iteration.
    let mouse_stick_active = Arc::new(AtomicBool::new(false));

    // Tray icon
    let tray_tx = tray::spawn(Arc::clone(&mouse_stick_active));

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
            let has_bt = all.iter().any(|c| c.connection_type == ConnectionType::Bluetooth);
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
        if info.connection_type == ConnectionType::Bluetooth {
            if let Err(e) = hid::activate_bt_extended_mode(&device, info.controller_type) {
                log::error!("Failed to activate BT extended mode: {e}");
                log::error!("Controller may not work correctly over Bluetooth.");
            }
        }

        // DS4 has no touchpad parsing — auto-enable stick mouse mode.
        // DualSense switches back to touchpad mode (its native input).
        let stick = info.controller_type.is_ds4();
        let _ = tray_tx.send(tray::TrayCmd::SetStickMode(stick));

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
                let _ = std::thread::Builder::new()
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
                                log::info!("USB scanner: USB controller detected, signaling switch");
                                flag_clone.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                    });
                (Some(flag), Some(stop))
            } else {
                (None, None)
            };

        // Spawn output loop for this connection
        let output_handle = handle.clone_handle();
        let lightbar_color = cfg.lightbar.clone();
        let output_task = tokio::spawn(async move {
            run_output_loop(output_handle, ct, conn, lightbar_color).await;
        });

        // Run input loop — returns when device disconnects or USB scanner signals
        run_input_loop(handle, ct, conn, &cfg, &detected, Arc::clone(&mouse_stick_active), usb_available.clone()).await;

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
async fn run_input_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    cfg: &config::Config,
    detected: &detect::Detected,
    mouse_stick_active: Arc<AtomicBool>,
    usb_switch_flag: Option<Arc<AtomicBool>>,
) {
    let mut mapper_state = mapper::MapperState::new(cfg, detected, mouse_stick_active);
    let mut buf = [0u8; 128];
    let mut consecutive_errors = 0u32;
    let mut first_report = true;
    let mut last_mute = false;

    loop {
        match handle.read(&mut buf) {
            Err(()) => {
                // Device disconnected
                return;
            }
            Ok(0) => {
                // No data available — yield and retry
                sleep(Duration::from_millis(4)).await;
                consecutive_errors = 0;

                // Check if USB scanner detected a USB controller (BT→USB switch)
                if let Some(ref flag) = usb_switch_flag {
                    if flag.load(Ordering::Relaxed) {
                        log::info!("USB controller available — switching from Bluetooth");
                        return;
                    }
                }

                continue;
            }
            Ok(n) => {
                let data = &buf[..n];

                if first_report {
                    let hex: Vec<String> = data.iter().take(16).map(|b| format!("{b:02X}")).collect();
                    log::info!("First report ({n} bytes): {}", hex.join(" "));
                    first_report = false;
                }

                // Validate CRC on Bluetooth
                if conn == ConnectionType::Bluetooth && !input::validate_bt_crc(ct, data) {
                    consecutive_errors += 1;
                    if consecutive_errors % 100 == 1 {
                        log::warn!("BT CRC validation failed ({consecutive_errors} times)");
                    }
                    continue;
                }

                match input::parse(ct, conn, data) {
                    Ok(unified) => {
                        consecutive_errors = 0;
                        let actions = mapper_state.update(&unified);
                        for action in &actions {
                            #[cfg(windows)]
                            mapper::execute_action(action);
                            log::debug!("Action: {action:?}");
                        }

                        // Mute button — toggle system mic on press (DualSense only; DS4 has no mic)
                        let mute_now = unified.buttons.mute;
                        if ct.is_dualsense() && mute_now && !last_mute {
                            tokio::task::spawn_blocking(mic::toggle_mute);
                        }
                        last_mute = mute_now;
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors % 100 == 1 {
                            log::warn!("Input parse error ({consecutive_errors}): {e}");
                        }
                    }
                }
            }
        }
    }
}

/// Player indicator LED preset — mimics PS5 native Player 1 assignment (center dot).
const PLAYER_LEDS: u8 = 0x04;

/// Output loop: keep the static lightbar color, player LED, and mic-mute LED updated.
async fn run_output_loop(
    handle: hid::HidHandle,
    ct: controller::ControllerType,
    conn: controller::ConnectionType,
    lightbar_color: config::ColorConfig,
) {
    let mut bt_seq = 0u8;

    // Prime mic mute state from system before first frame
    tokio::task::spawn_blocking(mic::init).await.ok();

    // The output report is static except for the mute LED, but resending
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
            player_leds: PLAYER_LEDS,
            mute_led: mic::MIC_MUTED.load(Ordering::Relaxed) as u8,
        };
        let report = output::build_report(ct, conn, &out, &mut bt_seq);
        handle.write(&report);
    }
}
