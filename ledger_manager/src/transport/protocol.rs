//! Ledger HID protocol implementation (platform-agnostic)
//!
//! This module implements the Ledger HID framing protocol for APDU commands.
//! Reference: <https://github.com/LedgerHQ/ledger-live/tree/develop/libs/ledgerjs>
//!
//! The Ledger HID protocol wraps APDU commands into 64-byte HID packets:
//! - First packet: [`channel_id` (2)] [tag (1)] [seq (2)] [length (2)] [data (57)]
//! - Subsequent packets: [`channel_id` (2)] [tag (1)] [seq (2)] [data (59)]

use std::fmt;

/// Size of HID packets sent to/from Ledger devices
pub const HID_PACKET_SIZE: usize = 64;

/// Ledger vendor ID for device detection
pub const LEDGER_VENDOR_ID: u16 = 0x2c97;

/// Default HID channel ID
pub const DEFAULT_CHANNEL_ID: u16 = 0x0101;

/// Tag indicating APDU data
pub const TAG_APDU: u8 = 0x05;

/// Maximum data bytes in first packet (after 7-byte header)
const FIRST_PACKET_DATA_SIZE: usize = HID_PACKET_SIZE - 7;

/// Maximum data bytes in continuation packets (after 5-byte header)
const CONTINUATION_PACKET_DATA_SIZE: usize = HID_PACKET_SIZE - 5;

/// Errors that can occur during APDU protocol operations
#[derive(Debug, Clone, PartialEq)]
pub enum ApduError {
    /// Response is too short to contain status word
    ResponseTooShort,
    /// Channel ID mismatch in response
    ChannelMismatch { expected: u16, got: u16 },
    /// Invalid tag in response
    InvalidTag { expected: u8, got: u8 },
    /// Sequence number mismatch
    SequenceMismatch { expected: u16, got: u16 },
    /// APDU command execution error from device
    DeviceError { sw: u16, message: String },
    /// Protocol framing error
    FramingError(String),
}

impl fmt::Display for ApduError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResponseTooShort => write!(f, "Response too short"),
            Self::ChannelMismatch { expected, got } => {
                write!(
                    f,
                    "Channel mismatch: expected {expected:#06x}, got {got:#06x}"
                )
            }
            Self::InvalidTag { expected, got } => {
                write!(f, "Invalid tag: expected {expected:#04x}, got {got:#04x}")
            }
            Self::SequenceMismatch { expected, got } => {
                write!(f, "Sequence mismatch: expected {expected}, got {got}")
            }
            Self::DeviceError { sw, message } => {
                write!(f, "Device error (SW={sw:#06x}): {message}")
            }
            Self::FramingError(msg) => write!(f, "Framing error: {msg}"),
        }
    }
}

impl std::error::Error for ApduError {}

/// Frame APDU data into HID packets
///
/// # Arguments
/// * `data` - The APDU command data to frame
/// * `channel_id` - The channel ID to use
///
/// # Returns
/// A vector of 64-byte HID packets
#[must_use]
pub fn frame_apdu(data: &[u8], channel_id: u16) -> Vec<[u8; HID_PACKET_SIZE]> {
    let mut packets = Vec::new();
    let mut offset = 0;
    let mut sequence: u16 = 0;

    // First packet
    let mut packet = [0u8; HID_PACKET_SIZE];

    // Channel ID (big-endian)
    packet[0] = (channel_id >> 8) as u8;
    packet[1] = channel_id as u8;

    // Tag
    packet[2] = TAG_APDU;

    // Sequence number (big-endian)
    packet[3] = (sequence >> 8) as u8;
    packet[4] = sequence as u8;

    // Data length (big-endian)
    let data_len = data.len();
    packet[5] = (data_len >> 8) as u8;
    packet[6] = data_len as u8;

    // Copy first chunk of data
    let first_chunk_len = std::cmp::min(FIRST_PACKET_DATA_SIZE, data_len);
    packet[7..7 + first_chunk_len].copy_from_slice(&data[..first_chunk_len]);
    offset += first_chunk_len;

    packets.push(packet);

    // Continuation packets
    while offset < data_len {
        sequence += 1;
        let mut packet = [0u8; HID_PACKET_SIZE];

        // Channel ID (big-endian)
        packet[0] = (channel_id >> 8) as u8;
        packet[1] = channel_id as u8;

        // Tag
        packet[2] = TAG_APDU;

        // Sequence number (big-endian)
        packet[3] = (sequence >> 8) as u8;
        packet[4] = sequence as u8;

        // Copy data chunk
        let chunk_len = std::cmp::min(CONTINUATION_PACKET_DATA_SIZE, data_len - offset);
        packet[5..5 + chunk_len].copy_from_slice(&data[offset..offset + chunk_len]);
        offset += chunk_len;

        packets.push(packet);
    }

    packets
}

/// Parse response data from HID packets
///
/// # Arguments
/// * `packets` - Slice of HID packet data
/// * `channel_id` - Expected channel ID
///
/// # Returns
/// A tuple of (`response_data` without SW, `status_word`)
pub fn parse_response(packets: &[&[u8]], channel_id: u16) -> Result<(Vec<u8>, u16), ApduError> {
    if packets.is_empty() {
        return Err(ApduError::ResponseTooShort);
    }

    let mut data = Vec::new();
    let mut expected_length: Option<usize> = None;
    let mut sequence: u16 = 0;

    #[allow(clippy::explicit_counter_loop)]
    for (i, packet) in packets.iter().enumerate() {
        if packet.len() < 5 {
            return Err(ApduError::FramingError("Packet too short".to_string()));
        }

        // Verify channel ID
        let pkt_channel = (u16::from(packet[0]) << 8) | u16::from(packet[1]);
        if pkt_channel != channel_id {
            return Err(ApduError::ChannelMismatch {
                expected: channel_id,
                got: pkt_channel,
            });
        }

        // Verify tag
        if packet[2] != TAG_APDU {
            return Err(ApduError::InvalidTag {
                expected: TAG_APDU,
                got: packet[2],
            });
        }

        // Verify sequence number
        let pkt_sequence = (u16::from(packet[3]) << 8) | u16::from(packet[4]);
        if pkt_sequence != sequence {
            return Err(ApduError::SequenceMismatch {
                expected: sequence,
                got: pkt_sequence,
            });
        }

        if i == 0 {
            // First packet: extract length
            if packet.len() < 7 {
                return Err(ApduError::FramingError(
                    "First packet missing length field".to_string(),
                ));
            }
            let length = ((packet[5] as usize) << 8) | (packet[6] as usize);
            expected_length = Some(length);

            // Extract data from first packet
            let available = packet.len().saturating_sub(7);
            let to_copy = std::cmp::min(available, length);
            data.extend_from_slice(&packet[7..7 + to_copy]);
        } else {
            // Continuation packet
            let available = packet.len().saturating_sub(5);
            let remaining = expected_length.unwrap_or(0).saturating_sub(data.len());
            let to_copy = std::cmp::min(available, remaining);
            data.extend_from_slice(&packet[5..5 + to_copy]);
        }

        sequence += 1;

        // Check if we have all the data
        if let Some(exp_len) = expected_length {
            if data.len() >= exp_len {
                break;
            }
        }
    }

    // Verify we got enough data for at least the status word
    if data.len() < 2 {
        return Err(ApduError::ResponseTooShort);
    }

    // Extract status word (last 2 bytes)
    let sw_offset = data.len() - 2;
    let sw = (u16::from(data[sw_offset]) << 8) | u16::from(data[sw_offset + 1]);

    // Return data without status word
    data.truncate(sw_offset);

    Ok((data, sw))
}

/// Interpret a status word and return an error message if not success
pub fn interpret_status_word(sw: u16) -> Result<(), ApduError> {
    match sw {
        0x9000 => Ok(()), // Success
        0x6985 => Err(ApduError::DeviceError {
            sw,
            message: "Conditions not satisfied (user rejected or app not ready)".to_string(),
        }),
        0x5515 => Err(ApduError::DeviceError {
            sw,
            message: "Device is locked".to_string(),
        }),
        0x6A82 => Err(ApduError::DeviceError {
            sw,
            message: "File not found".to_string(),
        }),
        0x6A86 => Err(ApduError::DeviceError {
            sw,
            message: "Incorrect P1 or P2".to_string(),
        }),
        0x6A87 => Err(ApduError::DeviceError {
            sw,
            message: "Incorrect data length".to_string(),
        }),
        0x6B00 => Err(ApduError::DeviceError {
            sw,
            message: "Incorrect parameters".to_string(),
        }),
        0x6D00 => Err(ApduError::DeviceError {
            sw,
            message: "Instruction not supported".to_string(),
        }),
        0x6E00 => Err(ApduError::DeviceError {
            sw,
            message: "Class not supported".to_string(),
        }),
        0x6F00 => Err(ApduError::DeviceError {
            sw,
            message: "Unknown error".to_string(),
        }),
        sw if sw & 0xFF00 == 0x6100 => Ok(()), // More data available
        sw if sw & 0xFF00 == 0x6C00 => Err(ApduError::DeviceError {
            sw,
            message: format!("Wrong length, expected {} bytes", sw & 0x00FF),
        }),
        _ => Err(ApduError::DeviceError {
            sw,
            message: format!("Unknown status word: {sw:#06x}"),
        }),
    }
}
