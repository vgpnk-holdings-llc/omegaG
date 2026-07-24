/// HID device management: open controller, read input reports, write output reports.
///
/// Key DS4Windows patterns replicated here:
/// - Filter by VID/PID + usage page 0x01 / usage 0x05 (gamepad collection)
/// - Activate Bluetooth extended mode via feature report
/// - Non-blocking read with timeout
/// - Write errors are non-fatal (log and continue)
use crate::controller::{self, ConnectionType, ControllerType, GAMEPAD_USAGE, GAMEPAD_USAGE_PAGE};
use hidapi::{BusType, HidApi, HidDevice};
use std::sync::{Arc, Mutex};

/// Derive connection type from the hidapi `BusType` — the authoritative signal on
/// all platforms. On Linux the value is read from the sysfs HID_ID uevent entry
/// (bus type 0x05 = Bluetooth, 0x03 = USB); on Windows the C hidapi backend
/// populates it from the HID device info query. The previous path-string heuristic
/// (detecting Windows BT GUIDs in the path) was always a no-op on Linux because
/// hidraw paths (`/dev/hidrawN`) carry no such marker.
fn conn_from_bus_type(bt: BusType) -> ConnectionType {
    match bt {
        BusType::Bluetooth => ConnectionType::Bluetooth,
        _ => ConnectionType::Usb,
    }
}

/// Does this hidapi error message mean the controller went away?
///
/// Windows: error 1167 (ERROR_DEVICE_NOT_CONNECTED) or "not connected".
/// Linux hidraw: the kernel returns ENODEV ("No such device") on USB unplug
/// or Bluetooth link drop. Both must funnel into the same USB-priority /
/// BT-fallback reconnect path in `main.rs` instead of a silent error loop.
fn is_disconnect_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    msg.contains("1167") || lower.contains("not connected") || lower.contains("no such device")
}

/// Does this hidapi open error look like a hidraw permission problem?
#[cfg(target_os = "linux")]
fn looks_like_permission_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("permission denied") || lower.contains("operation not permitted")
}

/// Information about a discovered controller.
pub struct ControllerInfo {
    pub controller_type: ControllerType,
    pub connection_type: ConnectionType,
    pub path: String,
}

/// Find all supported controllers, sorted with USB devices first.
/// When a controller is connected via both USB and Bluetooth simultaneously,
/// USB will always appear first — callers can `.next()` to pick the preferred one.
pub fn find_all_controllers(api: &HidApi) -> Vec<ControllerInfo> {
    let mut usb = Vec::new();
    let mut bt = Vec::new();

    for dev in api.device_list() {
        if dev.usage_page() != GAMEPAD_USAGE_PAGE || dev.usage() != GAMEPAD_USAGE {
            continue;
        }

        if let Some(ct) = controller::identify(dev.vendor_id(), dev.product_id()) {
            let path = dev.path().to_string_lossy().to_string();
            let conn = conn_from_bus_type(dev.bus_type());
            log::info!("Found {} ({}) at {}", ct, conn, &path[..path.len().min(60)]);
            let info = ControllerInfo {
                controller_type: ct,
                connection_type: conn,
                path,
            };
            match conn {
                ConnectionType::Usb => usb.push(info),
                ConnectionType::Bluetooth => bt.push(info),
            }
        }
    }

    usb.extend(bt);
    usb
}

/// Quick check: is there a USB controller present?
/// Used by the background USB scanner thread — avoids allocating a Vec.
pub fn has_usb_controller(api: &HidApi) -> bool {
    api.device_list().any(|dev| {
        dev.usage_page() == GAMEPAD_USAGE_PAGE
            && dev.usage() == GAMEPAD_USAGE
            && controller::identify(dev.vendor_id(), dev.product_id()).is_some()
            && conn_from_bus_type(dev.bus_type()) == ConnectionType::Usb
    })
}

/// Open the controller device.
pub fn open_device(api: &HidApi, info: &ControllerInfo) -> Result<HidDevice, hidapi::HidError> {
    let cpath = std::ffi::CString::new(info.path.as_bytes()).map_err(|_| {
        hidapi::HidError::HidApiError {
            message: "Invalid device path".into(),
        }
    })?;
    let device = match api.open_path(&cpath) {
        Ok(device) => device,
        Err(e) => {
            // Linux hidraw nodes (/dev/hidrawN) are root:input 0660 by
            // default — a permission error here means the udev rule isn't
            // installed. Log precise remediation; the reconnect loop will
            // keep retrying, so this stays non-fatal like any open failure.
            #[cfg(target_os = "linux")]
            if looks_like_permission_error(&format!("{e}")) {
                log::error!(
                    "Permission denied opening {} — hidraw access requires the ds4cc udev rule. \
                     Remediation: sudo install -m644 packaging/linux/99-ds4cc.rules \
                     /etc/udev/rules.d/ && sudo udevadm control --reload-rules && \
                     sudo udevadm trigger; add your user to the 'input' group and re-login.",
                    info.path
                );
            }
            return Err(e);
        }
    };
    device.set_blocking_mode(false)?;
    Ok(device)
}

/// Activate Bluetooth extended mode by reading the appropriate feature report.
/// DualSense: feature report 0x05
/// DS4: feature report 0x02
pub fn activate_bt_extended_mode(
    device: &HidDevice,
    ct: ControllerType,
) -> Result<(), hidapi::HidError> {
    let report_id = if ct.is_dualsense() { 0x05 } else { 0x02 };
    let mut buf = [0u8; 64];
    buf[0] = report_id;
    match device.get_feature_report(&mut buf) {
        Ok(n) => {
            log::info!("BT extended mode activated (feature report 0x{report_id:02X}, {n} bytes)");
            Ok(())
        }
        Err(e) => {
            log::warn!("Failed to read feature report 0x{report_id:02X}: {e}");
            Err(e)
        }
    }
}

/// Wrapper around HidDevice for thread-safe write access.
/// Reads happen on the dedicated HID thread; writes can come from the lightbar/rumble tasks.
pub struct HidHandle {
    device: Arc<Mutex<HidDevice>>,
}

impl HidHandle {
    pub fn new(device: HidDevice) -> Self {
        Self {
            device: Arc::new(Mutex::new(device)),
        }
    }

    /// Clone the handle for sharing across tasks.
    pub fn clone_handle(&self) -> Self {
        Self {
            device: Arc::clone(&self.device),
        }
    }

    /// Read an input report on the blocking pool.
    ///
    /// `read_timeout` is a blocking syscall (up to 5 ms), so it runs inside
    /// `spawn_blocking` — never on an async worker thread — and the returned
    /// future is awaited so the caller's async runtime is not starved. The
    /// device mutex is locked and released entirely within the blocking closure,
    /// so no lock is held across an `.await` point.
    ///
    /// Returns `Ok(bytes)` (empty = no data available within the timeout), or
    /// `Err(())` if the device is disconnected.
    pub async fn read(&self) -> Result<Vec<u8>, ()> {
        let device = Arc::clone(&self.device);
        tokio::task::spawn_blocking(move || {
            let dev = device.lock().unwrap();
            let mut buf = [0u8; 128];
            match dev.read_timeout(&mut buf, 5) {
                Ok(n) => Ok(buf[..n].to_vec()),
                Err(e) => {
                    let msg = format!("{e}");
                    if is_disconnect_error(&msg) {
                        Err(()) // device disconnected
                    } else {
                        log::error!("HID read error: {e}");
                        Ok(Vec::new())
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|e| {
            log::error!("HID read task failed to join: {e}");
            Err(())
        })
    }

    /// Write an output report on the blocking pool. Errors are logged but not
    /// propagated (non-fatal). The blocking `write` syscall runs inside
    /// `spawn_blocking` and is awaited, mirroring [`read`](Self::read).
    pub async fn write(&self, report: Vec<u8>) -> bool {
        let device = Arc::clone(&self.device);
        tokio::task::spawn_blocking(move || {
            let dev = device.lock().unwrap();
            match dev.write(&report) {
                Ok(_) => true,
                Err(e) => {
                    log::debug!("HID write error (non-fatal): {e}");
                    false
                }
            }
        })
        .await
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_sorts_before_bt() {
        let bt = ControllerInfo {
            controller_type: ControllerType::DualSense,
            connection_type: ConnectionType::Bluetooth,
            path: "bt_path".into(),
        };
        let usb = ControllerInfo {
            controller_type: ControllerType::DualSense,
            connection_type: ConnectionType::Usb,
            path: "usb_path".into(),
        };
        // Simulate the two-vec ordering from find_all_controllers
        let mut usb_vec = vec![usb];
        let bt_vec = vec![bt];
        usb_vec.extend(bt_vec);
        assert_eq!(usb_vec[0].connection_type, ConnectionType::Usb);
        assert_eq!(usb_vec[1].connection_type, ConnectionType::Bluetooth);
    }

    #[test]
    fn single_bt_when_no_usb() {
        let bt = ControllerInfo {
            controller_type: ControllerType::DualSense,
            connection_type: ConnectionType::Bluetooth,
            path: "bt_path".into(),
        };
        let mut usb_vec: Vec<ControllerInfo> = Vec::new();
        let bt_vec = vec![bt];
        usb_vec.extend(bt_vec);
        assert_eq!(usb_vec.len(), 1);
        assert_eq!(usb_vec[0].connection_type, ConnectionType::Bluetooth);
    }

    #[test]
    fn conn_from_bus_type_bluetooth() {
        // BusType::Bluetooth must map to ConnectionType::Bluetooth so that BT
        // controllers on Linux (hidraw paths carry no Windows GUID markers) are
        // correctly identified and receive CRC validation + extended-mode activation.
        assert_eq!(
            conn_from_bus_type(BusType::Bluetooth),
            ConnectionType::Bluetooth
        );
    }

    #[test]
    fn conn_from_bus_type_usb_and_unknown() {
        assert_eq!(conn_from_bus_type(BusType::Usb), ConnectionType::Usb);
        // Unknown/I2C/SPI bus types fall back to USB (safest default: skips BT CRC).
        assert_eq!(conn_from_bus_type(BusType::Unknown), ConnectionType::Usb);
    }

    #[test]
    fn disconnect_detection_covers_windows_and_linux() {
        // Windows ERROR_DEVICE_NOT_CONNECTED
        assert!(is_disconnect_error("hidapi error 1167"));
        assert!(is_disconnect_error("The device is not connected."));
        // Linux hidraw ENODEV on USB unplug / BT link drop
        assert!(is_disconnect_error("No such device"));
        assert!(is_disconnect_error("hid_read_timeout: no such device"));
        // Transient errors must NOT be treated as disconnects
        assert!(!is_disconnect_error("Input/output error"));
        assert!(!is_disconnect_error("Interrupted system call"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn permission_error_detection() {
        assert!(looks_like_permission_error(
            "Permission denied (os error 13)"
        ));
        assert!(looks_like_permission_error("Operation not permitted"));
        assert!(!looks_like_permission_error("No such device"));
        assert!(!looks_like_permission_error("unable to open device"));
    }
}
