//! Controller **state reading** — raw HID → validated [`UnifiedInput`].
//!
//! ## Boundary (locked)
//!
//! ```text
//! hid::HidHandle::read  →  state::decode_report  →  UnifiedInput
//! UnifiedInput          →  mapper::MapperState::update  →  Action[]
//! Action                →  mapper::execute_action  →  OS inject
//! ```
//!
//! **State reading owns:** connection type (USB/BT), BT CRC gate, report
//! offsets, DualSense/DS4 payload layout, touchpad decode.
//!
//! **Mapper owns:** rising-edge, holds, button→combo resolution, inject.
//! Mapper code must not call HID or CRC; it only sees [`UnifiedInput`].
//!
//! Focus order for active work: **mapper correctness** + **Bluetooth
//! decode** (this module + `input`/`hid`/`crc32`). Inject packaging is
//! orthogonal and does not belong in the mapper.

use crate::controller::{ConnectionType, ControllerType};
use crate::input::{self, ParseError, UnifiedInput};

/// Identity of the open controller link used while decoding reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerLink {
    pub controller: ControllerType,
    pub connection: ConnectionType,
}

impl ControllerLink {
    pub fn new(controller: ControllerType, connection: ConnectionType) -> Self {
        Self {
            controller,
            connection,
        }
    }

    pub fn is_bluetooth(self) -> bool {
        self.connection == ConnectionType::Bluetooth
    }
}

/// Outcome of turning one raw HID read into normalized state.
#[derive(Debug)]
pub enum DecodeError {
    /// Bluetooth CRC-32 over the full report failed (drop frame, keep link).
    BtCrcInvalid,
    /// Payload too short or otherwise unparseable.
    Parse(ParseError),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::BtCrcInvalid => write!(f, "bluetooth CRC validation failed"),
            DecodeError::Parse(e) => write!(f, "report parse: {e}"),
        }
    }
}

/// Decode one raw HID report into [`UnifiedInput`].
///
/// For Bluetooth links, validates the DualSense/DS4 input CRC **before**
/// parsing. Returns [`DecodeError::BtCrcInvalid`] so the input loop can
/// count and drop the frame without touching the mapper.
pub fn decode_report(link: ControllerLink, raw: &[u8]) -> Result<UnifiedInput, DecodeError> {
    if link.is_bluetooth() && !input::validate_bt_crc(link.controller, raw) {
        return Err(DecodeError::BtCrcInvalid);
    }
    input::parse(link.controller, link.connection, raw).map_err(DecodeError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc32;
    use crate::input::{DPad, parse};

    fn dualsense_usb_cross_report() -> Vec<u8> {
        let mut data = vec![0u8; 64];
        data[0] = 10;
        data[1] = 250;
        data[2] = 128;
        data[3] = 128;
        data[7] = (1 << 5) | 8; // cross + neutral hat
        data
    }

    /// DualSense BT extended report skeleton with correct CRC stamp.
    fn dualsense_bt_cross_report() -> Vec<u8> {
        // Report ID 0x31, 1-byte BT header, then USB-like payload; CRC at end.
        let mut data = vec![0u8; 78];
        data[0] = 0x31;
        data[1] = 0x00; // BT header
        // payload starts at offset 2 (parse_dualsense_bt with report ID present)
        data[2] = 10; // LX
        data[3] = 250; // LY
        data[4] = 128;
        data[5] = 128;
        data[9] = (1 << 5) | 8; // buttons at off+7
        let crc_off = data.len() - 4;
        crc32::stamp(crc32::SEED_INPUT, &mut data, crc_off);
        data
    }

    #[test]
    fn usb_decode_matches_input_parse() {
        let raw = dualsense_usb_cross_report();
        let link = ControllerLink::new(ControllerType::DualSense, ConnectionType::Usb);
        let via_state = decode_report(link, &raw).expect("usb decode");
        let via_input = parse(ControllerType::DualSense, ConnectionType::Usb, &raw).unwrap();
        assert_eq!(via_state.buttons.cross, via_input.buttons.cross);
        assert!(via_state.buttons.cross);
        assert_eq!(via_state.left_stick, (10, 250));
        assert_eq!(via_state.buttons.dpad, DPad::Neutral);
    }

    #[test]
    fn bluetooth_decode_accepts_valid_crc_and_cross() {
        let raw = dualsense_bt_cross_report();
        assert!(
            input::validate_bt_crc(ControllerType::DualSense, &raw),
            "fixture must have valid BT CRC"
        );
        let link = ControllerLink::new(ControllerType::DualSense, ConnectionType::Bluetooth);
        let unified = decode_report(link, &raw).expect("bt decode");
        assert!(unified.buttons.cross);
        assert_eq!(unified.left_stick, (10, 250));
    }

    #[test]
    fn bluetooth_decode_rejects_bad_crc_without_parse() {
        let mut raw = dualsense_bt_cross_report();
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        let link = ControllerLink::new(ControllerType::DualSense, ConnectionType::Bluetooth);
        match decode_report(link, &raw) {
            Err(DecodeError::BtCrcInvalid) => {}
            other => panic!("expected BtCrcInvalid, got {other:?}"),
        }
    }

    #[test]
    fn usb_never_runs_bt_crc_gate() {
        // Corrupt "CRC-looking" tail must not matter on USB.
        let mut raw = dualsense_usb_cross_report();
        if raw.len() >= 4 {
            let n = raw.len();
            raw[n - 1] = 0xDE;
            raw[n - 2] = 0xAD;
        }
        let link = ControllerLink::new(ControllerType::DualSense, ConnectionType::Usb);
        assert!(decode_report(link, &raw).is_ok());
    }
}
