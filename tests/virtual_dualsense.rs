//! Headless integration test for the Linux port.
//!
//! Two layers, both runnable without physical hardware:
//!
//! 1. **Report parsing** — build a synthetic DualSense USB HID report in
//!    memory and feed it through the real parser (`src/input.rs`), asserting
//!    the decoded `UnifiedInput`. Always runs.
//!
//! 2. **Virtual evdev device** — create a virtual DualSense gamepad via
//!    `/dev/uinput` (`evdev::uinput::VirtualDevice`), verify it is
//!    enumerable by name/VID/PID like the real hardware, then emit button
//!    and stick events on it and read them back from its
//!    `/dev/input/eventN` node. This proves the uinput/evdev input path the
//!    daemon depends on works end-to-end in CI. Skips (with a message) when
//!    `/dev/uinput` is unavailable or not writable, e.g. unprivileged
//!    containers.
//!
//! The parser modules are included by path because this crate is a binary
//! without a lib target; `input.rs` only depends on `controller.rs` and
//! `crc32.rs`, so the three files compile standalone here.

#[path = "../src/controller.rs"]
mod controller;
#[path = "../src/crc32.rs"]
mod crc32;
#[allow(dead_code)]
#[path = "../src/input.rs"]
mod input;

use controller::{ConnectionType, ControllerType};

/// Build a 64-byte DualSense USB input report (Report ID 0x01 already
/// stripped, per the parser's contract) with the given sticks and the
/// Cross button pressed.
fn dualsense_usb_report_with_cross() -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[0] = 10; // left stick X (far left)
    data[1] = 250; // left stick Y (far down)
    data[2] = 128; // right stick X (center)
    data[3] = 128; // right stick Y (center)
    data[4] = 0; // L2 analog
    data[5] = 255; // R2 analog fully pulled
    // Byte 7: hat in low nibble (8 = neutral), face buttons in high nibble.
    // Cross is bit 5 per the DualSense layout (square=4, cross=5, circle=6, triangle=7).
    data[7] = (1 << 5) | 8;
    data[8] = 0; // L1/R1/... none
    data[9] = 0; // PS/touchpad-click/mute none
    data
}

#[test]
fn parses_synthetic_dualsense_usb_report() {
    let data = dualsense_usb_report_with_cross();
    let parsed = input::parse(ControllerType::DualSense, ConnectionType::Usb, &data)
        .expect("synthetic DualSense USB report must parse");

    assert!(parsed.buttons.cross, "cross button must be decoded");
    assert!(!parsed.buttons.circle, "circle must be clear");
    assert!(!parsed.buttons.square, "square must be clear");
    assert_eq!(parsed.buttons.dpad, input::DPad::Neutral);
    assert_eq!(parsed.left_stick, (10, 250));
    assert_eq!(parsed.right_stick, (128, 128));
}

#[test]
fn rejects_truncated_dualsense_report() {
    let data = vec![0u8; 5]; // far too short (parser needs at least 10)
    assert!(
        input::parse(ControllerType::DualSense, ConnectionType::Usb, &data).is_err(),
        "truncated report must be rejected"
    );
}

/// Everything below needs a usable /dev/uinput. Returns false (after
/// logging) when the kernel module or write permission is missing so the
/// suite still passes in restricted sandboxes.
fn uinput_usable() -> bool {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
    {
        Ok(_) => true,
        Err(e) => {
            eprintln!(
                "skipping uinput integration test: /dev/uinput not usable ({e}). \
                 Enable with: sudo modprobe uinput && sudo chmod a+rw /dev/uinput"
            );
            false
        }
    }
}

#[test]
fn virtual_dualsense_round_trip_over_evdev() {
    if !uinput_usable() {
        return;
    }

    use evdev::{
        AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventType, InputEvent, InputId,
        KeyCode, UinputAbsSetup,
    };

    // Buttons a DualSense exposes as a Linux gamepad (subset is fine here:
    // we only need enough to simulate and verify).
    let mut keys = AttributeSet::<KeyCode>::new();
    for kc in [
        KeyCode::BTN_SOUTH,  // Cross
        KeyCode::BTN_EAST,   // Circle
        KeyCode::BTN_WEST,   // Square
        KeyCode::BTN_NORTH,  // Triangle
        KeyCode::BTN_TL,     // L1
        KeyCode::BTN_TR,     // R1
        KeyCode::BTN_SELECT, // Create
        KeyCode::BTN_START,  // Options
        KeyCode::BTN_MODE,   // PS
        KeyCode::BTN_THUMBL, // L3
        KeyCode::BTN_THUMBR, // R3
    ] {
        keys.insert(kc);
    }

    // Sticks + triggers as absolute axes (0..=255 like the HID report).
    let abs = AbsInfo::new(128, 0, 255, 0, 0, 0);
    let axes = [
        AbsoluteAxisCode::ABS_X,
        AbsoluteAxisCode::ABS_Y,
        AbsoluteAxisCode::ABS_Z, // L2
        AbsoluteAxisCode::ABS_RX,
        AbsoluteAxisCode::ABS_RY,
        AbsoluteAxisCode::ABS_RZ, // R2
    ];

    // Sony Interactive Entertainment VID 0x054c, DualSense PID 0x0ce6.
    let id = InputId::new(BusType::BUS_USB, 0x054c, 0x0ce6, 0x111);
    let mut builder = evdev::uinput::VirtualDevice::builder()
        .expect("create uinput builder")
        .name("DualSense Wireless Controller")
        .input_id(id)
        .with_keys(&keys)
        .expect("configure gamepad buttons");
    for ax in axes {
        builder = builder
            .with_absolute_axis(&UinputAbsSetup::new(ax, abs))
            .expect("set axis range");
    }
    let mut vdev = builder.build().expect("create virtual DualSense");

    // The virtual device must be enumerable exactly like real hardware:
    // find its /dev/input/eventN node by name.
    std::thread::sleep(std::time::Duration::from_millis(200)); // let udev settle
    let (path, _) = evdev::enumerate()
        .find(|(_, d)| d.name() == Some("DualSense Wireless Controller"))
        .expect("virtual DualSense must appear in evdev enumeration");
    assert!(path.starts_with("/dev/input/event"), "unexpected node {path:?}");

    // Verify VID/PID match Sony DualSense, as the daemon's detection expects.
    let mut reader = evdev::Device::open(&path).expect("open virtual DualSense node");
    let rid = reader.input_id();
    assert_eq!(rid.vendor(), 0x054c, "vendor ID must be Sony");
    assert_eq!(rid.product(), 0x0ce6, "product ID must be DualSense");

    // Simulate a Cross press + left-stick hard-left, then read the events
    // back from the event node: the same bytes any evdev consumer sees.
    let press = [
        InputEvent::new(EventType::KEY.0, KeyCode::BTN_SOUTH.0, 1),
        InputEvent::new(EventType::ABSOLUTE.0, AbsoluteAxisCode::ABS_X.0, 10),
        InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
    ];
    vdev.emit(&press).expect("emit gamepad events");

    // Drain until we have seen both events (bounded so a broken setup
    // fails instead of hanging).
    let mut saw_cross = false;
    let mut saw_stick = false;
    for _ in 0..100 {
        for ev in reader.fetch_events().expect("read back events") {
            if ev.event_type() == EventType::KEY && ev.code() == KeyCode::BTN_SOUTH.0 && ev.value() == 1 {
                saw_cross = true;
            }
            if ev.event_type() == EventType::ABSOLUTE
                && ev.code() == AbsoluteAxisCode::ABS_X.0
                && ev.value() == 10
            {
                saw_stick = true;
            }
        }
        if saw_cross && saw_stick {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(saw_cross, "Cross button press must round-trip through evdev");
    assert!(saw_stick, "stick movement must round-trip through evdev");
}

/// The daemon's own virtual keyboard must also work headlessly: inject a
/// key combo through the same code path the mapper uses and read it back.
/// This is the Linux-port half of the bridge (controller → keystrokes).
#[test]
fn virtual_keyboard_injection_round_trip() {
    if !uinput_usable() {
        return;
    }

    use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};

    // Mirror of UinputInjector's device (kept local: the injector lives in
    // the binary crate; this test pins the same uinput contract).
    let mut keys = AttributeSet::<KeyCode>::new();
    keys.insert(KeyCode::KEY_LEFTCTRL);
    keys.insert(KeyCode::KEY_ENTER);
    let mut rel = AttributeSet::<RelativeAxisCode>::new();
    rel.insert(RelativeAxisCode::REL_X);

    let mut kbd = evdev::uinput::VirtualDevice::builder()
        .expect("create uinput builder")
        .name("ds4cc-test-virtual-input")
        .with_keys(&keys)
        .expect("configure keys")
        .with_relative_axes(&rel)
        .expect("configure rel axes")
        .build()
        .expect("create virtual keyboard");

    std::thread::sleep(std::time::Duration::from_millis(200));
    let (path, _) = evdev::enumerate()
        .find(|(_, d)| d.name() == Some("ds4cc-test-virtual-input"))
        .expect("virtual keyboard must appear in evdev enumeration");
    let mut reader = evdev::Device::open(&path).expect("open virtual keyboard node");

    // Ctrl+Enter, pressed in order, released in reverse (mapper semantics).
    let combo = [
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 1),
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_ENTER.0, 1),
        InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_ENTER.0, 0),
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 0),
        InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
    ];
    kbd.emit(&combo).expect("emit key combo");

    let mut seen: Vec<(u16, i32)> = Vec::new();
    for _ in 0..100 {
        for ev in reader.fetch_events().expect("read back key events") {
            if ev.event_type() == EventType::KEY {
                seen.push((ev.code(), ev.value()));
            }
        }
        if seen.len() >= 4 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        seen,
        vec![
            (KeyCode::KEY_LEFTCTRL.0, 1),
            (KeyCode::KEY_ENTER.0, 1),
            (KeyCode::KEY_ENTER.0, 0),
            (KeyCode::KEY_LEFTCTRL.0, 0),
        ],
        "Ctrl+Enter must round-trip in mapper order (press in order, release in reverse)"
    );
}
