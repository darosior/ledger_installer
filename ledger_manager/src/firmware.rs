//! Firmware update functionality for Ledger devices.
//!
//! This module provides functions to check for firmware updates and perform
//! the complete firmware update flow including OSU installation, MCU/bootloader
//! flashing, and final firmware installation.

use crate::{
    DeviceInfo, HsmMessage, HsmMessageData, StatusCode, BASE_API_V1_URL, BASE_SOCKET_URL,
    LIVE_COMMON_VERSION, PROVIDER,
};
use form_urlencoded::Serializer as UrlSerializer;
use ledger_transport_hidapi::TransportNativeHID;
use serde_derive::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

/// Maximum number of MCU/bootloader flash iterations before giving up.
pub const MAX_MCU_ITERATIONS: u32 = 5;

// ============================================================================
// Types
// ============================================================================

/// OSU (Operating System Updater) firmware metadata from Ledger API.
///
/// The OSU is the first step in a firmware update - it prepares the device
/// for receiving the final firmware.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OsuFirmware {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub perso: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub firmware_key: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub date_creation: String,
    #[serde(default)]
    pub date_last_modified: String,
    #[serde(default)]
    pub device_versions: Vec<i64>,
    #[serde(default)]
    pub providers: Vec<i64>,
    /// ID of the final firmware that will be installed after this OSU.
    #[serde(default)]
    pub next_se_firmware_final_version: i64,
    /// IDs of firmware versions that can upgrade to this OSU.
    #[serde(default)]
    pub previous_se_firmware_final_version: Vec<i64>,
}

/// Final firmware metadata from Ledger API.
///
/// This is the actual firmware that runs on the device after the update.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FinalFirmware {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub perso: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub firmware_key: String,
    #[serde(default)]
    pub hash: String,
    /// Firmware version string (e.g., "2.1.0").
    #[serde(default)]
    pub version: String,
    /// ID of the SE firmware this belongs to.
    #[serde(default)]
    pub se_firmware: i64,
    /// OSU versions that can install this firmware.
    #[serde(default)]
    pub osu_versions: Vec<OsuFirmware>,
    /// IDs of compatible MCU versions.
    #[serde(default)]
    pub mcu_versions: Vec<i64>,
    /// IDs of application versions compatible with this firmware.
    #[serde(default)]
    pub application_versions: Vec<i64>,
    /// Size of the firmware in bytes.
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub date_creation: String,
    #[serde(default)]
    pub date_last_modified: String,
    #[serde(default)]
    pub device_versions: Vec<i64>,
    #[serde(default)]
    pub providers: Vec<i64>,
}

/// MCU (Microcontroller Unit) version metadata from Ledger API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McuVersion {
    pub id: i64,
    /// MCU identifier.
    pub mcu: i64,
    /// MCU version name (e.g., "1.12").
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub providers: Vec<i64>,
    /// The bootloader version required to flash this MCU.
    pub from_bootloader_version: String,
    #[serde(default)]
    pub device_versions: Vec<i64>,
    #[serde(default)]
    pub se_firmware_final_versions: Vec<i64>,
    #[serde(default)]
    pub date_creation: String,
    #[serde(default)]
    pub date_last_modified: String,
}

/// Context containing all information needed to perform a firmware update.
#[derive(Debug, Clone)]
pub struct FirmwareUpdateContext {
    /// OSU firmware to install first.
    pub osu: OsuFirmware,
    /// Final firmware to install after OSU.
    pub final_firmware: FinalFirmware,
    /// Whether MCU needs to be flashed (device will go through bootloader).
    pub should_flash_mcu: bool,
}

/// Current phase of the firmware update process.
#[derive(Debug, Clone, PartialEq)]
pub enum FirmwareUpdatePhase {
    /// Checking for available firmware updates.
    CheckingForUpdates,
    /// Downloading firmware metadata from API.
    DownloadingMetadata,
    /// Installing OSU (Operating System Updater).
    InstallingOsu {
        /// Progress from 0.0 to 1.0.
        progress: f32,
    },
    /// Waiting for device to reboot into bootloader mode.
    WaitingForBootloader,
    /// Flashing MCU or bootloader firmware.
    FlashingMcu {
        /// Current flash iteration (1 to `MAX_MCU_ITERATIONS`).
        iteration: u32,
        /// Progress from 0.0 to 1.0.
        progress: f32,
    },
    /// Installing final firmware.
    InstallingFinalFirmware {
        /// Progress from 0.0 to 1.0.
        progress: f32,
    },
    /// Update completed successfully.
    Completed,
    /// Update failed with an error.
    Failed {
        /// Error description.
        error: String,
    },
}

/// WebSocket event types for tracking progress during firmware operations.
#[derive(Debug, Clone)]
pub enum WebSocketEvent {
    /// WebSocket connection opened.
    Opened,
    /// Bulk operation progress update.
    BulkProgress {
        /// Progress from 0.0 to 1.0.
        progress: f32,
        /// Current command index.
        index: usize,
        /// Total number of commands.
        total: usize,
    },
    /// APDU exchange completed.
    Exchange {
        /// Message nonce.
        nonce: u32,
        /// Response status code.
        status: u16,
    },
    /// HSM operation succeeded.
    Success,
    /// HSM operation failed.
    Error(String),
    /// HSM sent a warning (non-fatal).
    Warning(String),
    /// WebSocket connection closed.
    Closed,
}

/// Errors that can occur during firmware update operations.
#[derive(Debug, Clone)]
pub enum FirmwareUpdateError {
    /// Firmware is already up to date.
    AlreadyUpToDate,
    /// Device must be on dashboard (not in bootloader/OSU mode).
    DeviceNotOnDashboard,
    /// Device must be in OSU mode to install final firmware.
    DeviceNotInOsuMode,
    /// Device is not onboarded (no PIN set).
    DeviceNotOnboarded,
    /// Not enough storage space on device.
    NotEnoughSpace,
    /// User refused the update on the device.
    UserRefused,
    /// Too many MCU/bootloader flash iterations (exceeded `MAX_MCU_ITERATIONS`).
    TooManyMcuIterations,
    /// Device disconnected during update.
    DeviceDisconnected,
    /// API request failed.
    ApiError(String),
    /// WebSocket communication error.
    WebSocketError(String),
    /// Unknown or incompatible MCU version.
    UnknownMcu,
    /// No firmware update available for this device.
    NoUpdateAvailable,
    /// Generic error.
    Other(String),
}

impl fmt::Display for FirmwareUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FirmwareUpdateError::AlreadyUpToDate => write!(f, "Firmware is already up to date"),
            FirmwareUpdateError::DeviceNotOnDashboard => {
                write!(f, "Device must be on dashboard to start update")
            }
            FirmwareUpdateError::DeviceNotInOsuMode => {
                write!(f, "Device must be in OSU mode to install final firmware")
            }
            FirmwareUpdateError::DeviceNotOnboarded => {
                write!(
                    f,
                    "Device is not onboarded. Please complete device setup first."
                )
            }
            FirmwareUpdateError::NotEnoughSpace => {
                write!(f, "Not enough space on device. Please uninstall some apps.")
            }
            FirmwareUpdateError::UserRefused => {
                write!(f, "Update was cancelled on the device")
            }
            FirmwareUpdateError::TooManyMcuIterations => {
                write!(f, "MCU flash failed after {MAX_MCU_ITERATIONS} attempts")
            }
            FirmwareUpdateError::DeviceDisconnected => {
                write!(f, "Device disconnected during update")
            }
            FirmwareUpdateError::ApiError(e) => write!(f, "API error: {e}"),
            FirmwareUpdateError::WebSocketError(e) => write!(f, "WebSocket error: {e}"),
            FirmwareUpdateError::UnknownMcu => write!(f, "Unknown or incompatible MCU version"),
            FirmwareUpdateError::NoUpdateAvailable => {
                write!(f, "No firmware update available for this device")
            }
            FirmwareUpdateError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for FirmwareUpdateError {}

impl From<Box<dyn Error>> for FirmwareUpdateError {
    fn from(e: Box<dyn Error>) -> Self {
        FirmwareUpdateError::Other(e.to_string())
    }
}

// ============================================================================
// API Response Types (internal)
// ============================================================================

/// Response from /`get_device_version` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceVersionResponse {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    pub target_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub device: i64,
    #[serde(default)]
    pub providers: Vec<i64>,
    #[serde(default)]
    pub mcu_versions: Vec<i64>,
    #[serde(default)]
    pub se_firmware_final_versions: Vec<i64>,
    #[serde(default)]
    pub osu_versions: Vec<i64>,
}

/// Response from /`get_latest_firmware` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct LatestFirmwareResponse {
    pub result: String,
    #[serde(default)]
    pub se_firmware_osu_version: Option<OsuFirmware>,
}

// ============================================================================
// API Functions
// ============================================================================

/// Get the device version info from the Ledger API.
pub fn get_device_version_from_api(
    target_id: u32,
) -> Result<DeviceVersionResponse, FirmwareUpdateError> {
    let url = format!(
        "{BASE_API_V1_URL}/get_device_version?livecommonversion={LIVE_COMMON_VERSION}&provider={PROVIDER}&target_id={target_id}"
    );

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    resp.json::<DeviceVersionResponse>()
        .map_err(|e| FirmwareUpdateError::ApiError(format!("Failed to parse response: {e}")))
}

/// Generate a firmware salt from a user ID.
/// The salt is SHA256(userId + "|firmwareSalt") truncated to 6 hex characters.
/// We generate a random UUID-like string as user ID since we don't have a real one.
fn generate_firmware_salt() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate a pseudo-random user ID based on system time and random data
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Create a simple hash from the timestamp to generate a salt
    // We just need something that looks like a valid salt (6 hex chars)
    let hash_input = format!("{timestamp}|firmwareSalt");

    // Simple hash: sum of bytes mod some primes
    let bytes = hash_input.as_bytes();
    let mut hash: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        hash = hash.wrapping_add(u32::from(b).wrapping_mul((i as u32).wrapping_add(1)));
        hash = hash.wrapping_mul(31);
    }

    // Take 6 hex characters
    format!("{:06x}", hash & 0xFFFFFF)
}

/// Get the latest available firmware for a device.
pub fn get_latest_firmware_from_api(
    current_version: i64,
    device_version: i64,
) -> Result<Option<OsuFirmware>, FirmwareUpdateError> {
    let salt = generate_firmware_salt();

    let url = format!(
        "{BASE_API_V1_URL}/get_latest_firmware?livecommonversion={LIVE_COMMON_VERSION}&salt={salt}&current_se_firmware_final_version={current_version}&device_version={device_version}&provider={PROVIDER}"
    );

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    let latest: LatestFirmwareResponse = resp
        .json()
        .map_err(|e| FirmwareUpdateError::ApiError(format!("Failed to parse response: {e}")))?;

    Ok(latest.se_firmware_osu_version)
}

/// Get OSU firmware metadata by version name.
pub fn get_osu_firmware(
    device_version_id: i64,
    version_name: &str,
) -> Result<OsuFirmware, FirmwareUpdateError> {
    // OSU version name has "-osu" suffix
    let osu_version_name = if version_name.ends_with("-osu") {
        version_name.to_string()
    } else {
        format!("{version_name}-osu")
    };

    let url = format!(
        "{BASE_API_V1_URL}/get_osu_version?livecommonversion={LIVE_COMMON_VERSION}&device_version={device_version_id}&version_name={osu_version_name}&provider={PROVIDER}"
    );

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    resp.json::<OsuFirmware>()
        .map_err(|e| FirmwareUpdateError::ApiError(format!("Failed to parse OSU firmware: {e}")))
}

/// Get final firmware by ID.
pub fn get_final_firmware_by_id(id: i64) -> Result<FinalFirmware, FirmwareUpdateError> {
    let url = format!(
        "{BASE_API_V1_URL}/firmware_final_versions/{id}?livecommonversion={LIVE_COMMON_VERSION}"
    );

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    resp.json::<FinalFirmware>()
        .map_err(|e| FirmwareUpdateError::ApiError(format!("Failed to parse final firmware: {e}")))
}

/// Get all MCU versions from the API.
pub fn get_all_mcu_versions() -> Result<Vec<McuVersion>, FirmwareUpdateError> {
    let url = format!("{BASE_API_V1_URL}/mcu_versions?livecommonversion={LIVE_COMMON_VERSION}");

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    resp.json::<Vec<McuVersion>>()
        .map_err(|e| FirmwareUpdateError::ApiError(format!("Failed to parse MCU versions: {e}")))
}

/// Get the current firmware version ID for a device.
pub fn get_current_firmware_version_id(
    device_version_id: i64,
    version_name: &str,
) -> Result<i64, FirmwareUpdateError> {
    let url = format!(
        "{BASE_API_V1_URL}/get_firmware_version?livecommonversion={LIVE_COMMON_VERSION}&device_version={device_version_id}&version_name={version_name}&provider={PROVIDER}"
    );

    let resp = minreq::get(&url)
        .send()
        .map_err(|e| FirmwareUpdateError::ApiError(e.to_string()))?;

    if resp.status_code != 200 {
        return Err(FirmwareUpdateError::ApiError(format!(
            "HTTP {}: {}",
            resp.status_code,
            resp.as_str().unwrap_or("Unknown error")
        )));
    }

    #[derive(Deserialize)]
    struct FirmwareVersionResponse {
        id: i64,
    }

    let fw: FirmwareVersionResponse = resp.json().map_err(|e| {
        FirmwareUpdateError::ApiError(format!("Failed to parse firmware version: {e}"))
    })?;

    Ok(fw.id)
}

// ============================================================================
// Main Firmware Update Functions
// ============================================================================

/// Check if a firmware update is available for the device.
///
/// Returns `Some(FirmwareUpdateContext)` if an update is available,
/// `None` if the firmware is already up to date.
pub fn get_latest_firmware_for_device(
    device_info: &DeviceInfo,
) -> Result<Option<FirmwareUpdateContext>, FirmwareUpdateError> {
    // 1. Get device version from API
    let device_version = get_device_version_from_api(device_info.target_id)?;

    // 2. Get current firmware version ID
    let current_fw_id =
        match get_current_firmware_version_id(device_version.id, &device_info.version) {
            Ok(id) => id,
            Err(e) => {
                // If we can't get the current firmware ID, the device might have a
                // version that's not in the API database (e.g., very new or dev firmware)
                log::warn!("Could not get current firmware version ID: {}", e);
                return Ok(None);
            }
        };

    // 3. Check for latest firmware
    let osu = match get_latest_firmware_from_api(current_fw_id, device_version.id)? {
        Some(osu) => {
            // Verify the OSU has valid data (non-zero next firmware ID)
            if osu.next_se_firmware_final_version == 0 {
                log::info!("OSU has no next firmware version, device is up to date");
                return Ok(None);
            }
            osu
        }
        None => return Ok(None), // Already up to date
    };

    // 4. Get final firmware info
    let final_firmware = match get_final_firmware_by_id(osu.next_se_firmware_final_version) {
        Ok(fw) => fw,
        Err(e) => {
            // If we can't parse the final firmware, it might have null fields
            // indicating no valid update is available
            log::warn!("Could not get final firmware info: {}", e);
            return Ok(None);
        }
    };

    // Verify the final firmware has valid version info
    if final_firmware.version.is_empty() {
        log::info!("Final firmware has no version, device is up to date");
        return Ok(None);
    }

    // 5. Check if MCU needs flashing
    let mcus = get_all_mcu_versions()?;
    let current_mcu_name = device_info.mcu_version.as_deref().unwrap_or("");

    // Find current MCU version
    let current_mcu = mcus.iter().find(|m| m.name == current_mcu_name);

    // MCU needs flashing if current MCU is not in the list of compatible MCUs
    let should_flash_mcu = match current_mcu {
        Some(mcu) => !final_firmware.mcu_versions.contains(&mcu.id),
        None => true, // Unknown MCU, assume it needs flashing
    };

    Ok(Some(FirmwareUpdateContext {
        osu,
        final_firmware,
        should_flash_mcu,
    }))
}

/// Execute a WebSocket operation with progress callbacks.
///
/// This is an enhanced version of `query_via_websocket` that reports progress
/// for bulk operations.
pub fn query_via_websocket_with_progress<F>(
    ledger_api: &TransportNativeHID,
    url: &str,
    event_callback: F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(WebSocketEvent),
{
    use tungstenite::{connect, Message};

    let (mut socket, _) = connect(url)
        .map_err(|e| FirmwareUpdateError::WebSocketError(format!("Failed to connect: {e}")))?;

    event_callback(WebSocketEvent::Opened);

    loop {
        let msg = socket
            .read()
            .map_err(|e| FirmwareUpdateError::WebSocketError(format!("Read error: {e}")))?;

        match msg {
            Message::Text(text) => {
                let hsm_msg: HsmMessage = serde_json::from_str(&text).map_err(|e| {
                    FirmwareUpdateError::WebSocketError(format!("Parse error: {e}"))
                })?;

                if hsm_msg.query == "exchange" {
                    let command_hex = match hsm_msg.data {
                        Some(HsmMessageData::Command(h)) => h,
                        _ => {
                            return Err(FirmwareUpdateError::WebSocketError(
                                "Expected single command in exchange mode".into(),
                            ))
                        }
                    };

                    let command = crate::deser_apdu_command(&command_hex).map_err(|e| {
                        FirmwareUpdateError::WebSocketError(format!("APDU parse error: {e}"))
                    })?;

                    let resp = ledger_api
                        .exchange(&command)
                        .map_err(|_| FirmwareUpdateError::DeviceDisconnected)?;

                    // Map specific error codes
                    let status = resp.retcode();
                    if status == 0x6985 || status == 0x5501 {
                        return Err(FirmwareUpdateError::UserRefused);
                    } else if status == 0x6a84 || status == 0x5103 {
                        return Err(FirmwareUpdateError::NotEnoughSpace);
                    } else if status == 0x6d07 || status == 0x6611 {
                        return Err(FirmwareUpdateError::DeviceNotOnboarded);
                    }

                    event_callback(WebSocketEvent::Exchange {
                        nonce: hsm_msg.nonce,
                        status,
                    });

                    let response = if status == StatusCode::OK as u16 {
                        "success"
                    } else {
                        "error"
                    };

                    let resp_data = hex::encode(resp.data());
                    let ws_resp = serde_json::json!({
                        "nonce": hsm_msg.nonce,
                        "response": response,
                        "data": resp_data,
                    });

                    let json_str = serde_json::to_string(&ws_resp).map_err(|e| {
                        FirmwareUpdateError::WebSocketError(format!("JSON serialize error: {e}"))
                    })?;
                    socket.send(Message::Text(json_str)).map_err(|e| {
                        FirmwareUpdateError::WebSocketError(format!("Send error: {e}"))
                    })?;
                } else if hsm_msg.query == "bulk" {
                    let commands = match hsm_msg.data {
                        Some(HsmMessageData::CommandList(l)) => l,
                        _ => {
                            return Err(FirmwareUpdateError::WebSocketError(
                                "Expected command list in bulk mode".into(),
                            ))
                        }
                    };

                    let total = commands.len();
                    for (index, cmd_hex) in commands.iter().enumerate() {
                        if cmd_hex.is_empty() {
                            continue;
                        }

                        // Report progress
                        let progress = (index as f32 + 1.0) / total as f32;
                        event_callback(WebSocketEvent::BulkProgress {
                            progress,
                            index,
                            total,
                        });

                        let command = crate::deser_apdu_command(cmd_hex).map_err(|e| {
                            FirmwareUpdateError::WebSocketError(format!("APDU parse error: {e}"))
                        })?;

                        let _ = ledger_api
                            .exchange(&command)
                            .map_err(|_| FirmwareUpdateError::DeviceDisconnected)?;
                    }

                    let ws_resp = serde_json::json!({
                        "nonce": hsm_msg.nonce,
                        "response": "success",
                        "data": "",
                    });

                    let json_str = serde_json::to_string(&ws_resp).map_err(|e| {
                        FirmwareUpdateError::WebSocketError(format!("JSON serialize error: {e}"))
                    })?;
                    socket.send(Message::Text(json_str)).map_err(|e| {
                        FirmwareUpdateError::WebSocketError(format!("Send error: {e}"))
                    })?;
                } else if hsm_msg.query == "success" {
                    event_callback(WebSocketEvent::Success);
                    return Ok(());
                } else if hsm_msg.query == "error" {
                    event_callback(WebSocketEvent::Error(text.clone()));
                    return Err(FirmwareUpdateError::WebSocketError(format!(
                        "HSM error: {text}"
                    )));
                } else if hsm_msg.query == "warning" {
                    event_callback(WebSocketEvent::Warning(text));
                }
            }
            Message::Close(_) => {
                event_callback(WebSocketEvent::Closed);
                return Err(FirmwareUpdateError::WebSocketError(
                    "WebSocket closed unexpectedly".into(),
                ));
            }
            _ => {}
        }
    }
}

/// Install OSU firmware via WebSocket.
fn install_osu<F>(
    ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
    osu: &OsuFirmware,
    progress_callback: &F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    // Device must be on dashboard (not in bootloader)
    if device_info.is_bootloader {
        return Err(FirmwareUpdateError::DeviceNotOnDashboard);
    }

    let url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/install?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("firmware", &osu.firmware)
        .append_pair("firmwareKey", &osu.firmware_key)
        .append_pair("perso", &osu.perso)
        .append_pair("livecommonversion", LIVE_COMMON_VERSION)
        .finish();

    query_via_websocket_with_progress(ledger_api, &url, |event| {
        if let WebSocketEvent::BulkProgress { progress, .. } = event {
            progress_callback(FirmwareUpdatePhase::InstallingOsu { progress });
        }
    })
}

/// Flash MCU or bootloader via WebSocket.
fn flash_mcu<F>(
    ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
    mcu_version: &str,
    iteration: u32,
    progress_callback: &F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    let url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/mcu?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("version", mcu_version)
        .append_pair("livecommonversion", LIVE_COMMON_VERSION)
        .finish();

    query_via_websocket_with_progress(ledger_api, &url, |event| {
        if let WebSocketEvent::BulkProgress { progress, .. } = event {
            progress_callback(FirmwareUpdatePhase::FlashingMcu {
                iteration,
                progress,
            });
        }
    })
}

/// Install final firmware via WebSocket.
fn install_final_firmware<F>(
    ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
    firmware: &FinalFirmware,
    progress_callback: &F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    // Device must be in OSU mode for final firmware
    if !device_info.version.contains("-osu") {
        return Err(FirmwareUpdateError::DeviceNotInOsuMode);
    }

    let url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/install?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("firmware", &firmware.firmware)
        .append_pair("firmwareKey", &firmware.firmware_key)
        .append_pair("perso", &firmware.perso)
        .append_pair("livecommonversion", LIVE_COMMON_VERSION)
        .finish();

    query_via_websocket_with_progress(ledger_api, &url, |event| {
        if let WebSocketEvent::BulkProgress { progress, .. } = event {
            progress_callback(FirmwareUpdatePhase::InstallingFinalFirmware { progress });
        }
    })
}

/// Find the best MCU version to flash based on current device state and target firmware.
fn find_best_mcu_version(
    mcus: &[McuVersion],
    device_info: &DeviceInfo,
    target_mcu_ids: &[i64],
) -> Result<String, FirmwareUpdateError> {
    // Get current bootloader version (majMin from device info)
    // In bootloader mode, we can check which MCU can be flashed

    // Find MCUs that are compatible with the target firmware
    let compatible_mcus: Vec<&McuVersion> = mcus
        .iter()
        .filter(|m| target_mcu_ids.contains(&m.id))
        .collect();

    if compatible_mcus.is_empty() {
        return Err(FirmwareUpdateError::UnknownMcu);
    }

    // If in bootloader, check bootloader compatibility
    if device_info.is_bootloader {
        // The from_bootloader_version field indicates which bootloader version is needed
        // to flash this MCU. We need to find one that matches.

        // For now, return the first compatible MCU version name
        // In a more sophisticated implementation, we'd check bootloader compatibility
        return Ok(compatible_mcus[0].name.clone());
    }

    // Return first compatible MCU
    Ok(compatible_mcus[0].name.clone())
}

/// Execute the complete firmware update process.
///
/// This function performs the full firmware update flow:
/// 1. Install OSU (Operating System Updater)
/// 2. Flash MCU/bootloader if needed (device reboots to bootloader)
/// 3. Install final firmware
///
/// The `progress_callback` is called with the current phase throughout the process.
pub fn update_firmware<F>(
    ledger_api: &TransportNativeHID,
    context: &FirmwareUpdateContext,
    progress_callback: F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    // Get current device info
    let device_info = DeviceInfo::new(ledger_api)
        .map_err(|e| FirmwareUpdateError::Other(format!("Failed to get device info: {e}")))?;

    // Phase 1: Install OSU
    progress_callback(FirmwareUpdatePhase::InstallingOsu { progress: 0.0 });
    install_osu(ledger_api, &device_info, &context.osu, &progress_callback)?;

    // Phase 2: MCU/Bootloader (if needed)
    if context.should_flash_mcu {
        progress_callback(FirmwareUpdatePhase::WaitingForBootloader);

        // Device will reboot after OSU installation
        // Wait a bit for reboot
        std::thread::sleep(std::time::Duration::from_secs(3));

        // Get MCU versions for flashing
        let mcus = get_all_mcu_versions()?;

        // Flash MCU/Bootloader (loop up to MAX_MCU_ITERATIONS times)
        for iteration in 1..=MAX_MCU_ITERATIONS {
            progress_callback(FirmwareUpdatePhase::FlashingMcu {
                iteration,
                progress: 0.0,
            });

            // Re-get device info after reboot
            let current_info = DeviceInfo::new(ledger_api).map_err(|e| {
                FirmwareUpdateError::Other(format!("Failed to get device info: {e}"))
            })?;

            // If no longer in bootloader, we're done with MCU flashing
            if !current_info.is_bootloader {
                break;
            }

            // Find and flash the appropriate MCU
            let mcu_version =
                find_best_mcu_version(&mcus, &current_info, &context.final_firmware.mcu_versions)?;

            flash_mcu(
                ledger_api,
                &current_info,
                &mcu_version,
                iteration,
                &progress_callback,
            )?;

            // Wait for device to process
            std::thread::sleep(std::time::Duration::from_secs(2));

            if iteration == MAX_MCU_ITERATIONS {
                // Check one more time if we exited bootloader
                let check_info = DeviceInfo::new(ledger_api)
                    .map_err(|_| FirmwareUpdateError::TooManyMcuIterations)?;

                if check_info.is_bootloader {
                    return Err(FirmwareUpdateError::TooManyMcuIterations);
                }
            }
        }
    }

    // Phase 3: Install Final Firmware
    // Re-get device info (should be in OSU mode now)
    let osu_device_info = DeviceInfo::new(ledger_api)
        .map_err(|e| FirmwareUpdateError::Other(format!("Failed to get device info: {e}")))?;

    progress_callback(FirmwareUpdatePhase::InstallingFinalFirmware { progress: 0.0 });
    install_final_firmware(
        ledger_api,
        &osu_device_info,
        &context.final_firmware,
        &progress_callback,
    )?;

    progress_callback(FirmwareUpdatePhase::Completed);
    Ok(())
}

/// Repair a device that is stuck in bootloader mode.
///
/// This function attempts to recover a device by flashing the appropriate
/// MCU/bootloader based on the bootloader version. Follows the Ledger Live
/// repair flow which uses majMin (major.minor bootloader version) to determine
/// which MCU version to install.
///
/// Note: This version uses a single transport connection. If the device reboots
/// during repair, use `repair_device_in_bootloader_with_reconnect` instead.
pub fn repair_device_in_bootloader<F>(
    ledger_api: &TransportNativeHID,
    progress_callback: F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    // Get device info - must be in bootloader mode
    let device_info = DeviceInfo::new(ledger_api)
        .map_err(|e| FirmwareUpdateError::Other(format!("Failed to get device info: {e}")))?;

    if !device_info.is_bootloader {
        return Err(FirmwareUpdateError::Other(
            "Device is not in bootloader mode. Use normal update instead.".into(),
        ));
    }

    log::info!(
        "Device in bootloader mode. Version: {}, Target ID: {:#x}, SE Target ID: {:#x}",
        device_info.version,
        device_info.target_id,
        device_info.se_target_id
    );

    // For single-transport mode, we can only do one iteration
    // The device will reboot after MCU flash, invalidating the transport
    let iteration = 1;
    progress_callback(FirmwareUpdatePhase::FlashingMcu {
        iteration,
        progress: 0.0,
    });

    // Get the majMin (major.minor) bootloader version
    let maj_min = get_bootloader_maj_min(&device_info.version);
    log::info!(
        "Bootloader version: {}, majMin: {}",
        device_info.version,
        maj_min
    );

    // For Nano X with bootloader >= 1.4, we need to use SE target ID to query API
    // and find the correct MCU version
    let mcu_version_to_install = match maj_min.as_str() {
        // Legacy bootloader versions (Nano S primarily)
        "0.0" => "0.6".to_string(),
        "0.6" => "1.5".to_string(),
        "0.7" => "1.6".to_string(),
        "0.9" => "1.7".to_string(),
        // Modern bootloader versions - query API using SE target ID
        _ => {
            log::info!(
                "Querying API for MCU version compatible with bootloader {}",
                maj_min
            );
            find_mcu_for_repair_v2(&device_info)?
        }
    };

    println!(
        "Selected MCU version {mcu_version_to_install} for bootloader {maj_min} (iteration {iteration})"
    );
    log::info!(
        "Installing MCU version {} (iteration {})",
        mcu_version_to_install,
        iteration
    );

    // Flash the MCU version
    flash_mcu_by_version(
        ledger_api,
        &device_info,
        &mcu_version_to_install,
        iteration,
        &progress_callback,
    )?;

    // After MCU flash, device reboots.
    println!("MCU flash complete. Device is rebooting...");
    println!("Please run the command again after the device finishes rebooting.");
    progress_callback(FirmwareUpdatePhase::WaitingForBootloader);

    Ok(())
}

/// Repair a device that is stuck in bootloader mode, with automatic reconnection.
///
/// This function accepts a factory closure that creates new transport connections,
/// allowing it to reconnect after the device reboots during the repair process.
///
/// # Arguments
/// * `transport_factory` - A closure that creates a new `TransportNativeHID` connection
/// * `progress_callback` - A callback for progress updates
///
/// # Example
/// ```ignore
/// repair_device_in_bootloader_with_reconnect(
///     || {
///         let hid_api = HidApi::new()?;
///         TransportNativeHID::new(&hid_api)
///     },
///     |phase| println!("{:?}", phase),
/// )?;
/// ```
pub fn repair_device_in_bootloader_with_reconnect<T, F>(
    transport_factory: T,
    progress_callback: F,
) -> Result<(), FirmwareUpdateError>
where
    T: Fn() -> Result<TransportNativeHID, Box<dyn std::error::Error>>,
    F: Fn(FirmwareUpdatePhase),
{
    // Flash MCU/Bootloader until device exits bootloader
    // Following Ledger Live's repair flow based on bootloader version (majMin)
    for iteration in 1..=MAX_MCU_ITERATIONS {
        progress_callback(FirmwareUpdatePhase::FlashingMcu {
            iteration,
            progress: 0.0,
        });

        // Connect (or reconnect) to device - wait for it to be available
        println!("Connecting to device...");
        let ledger_api = wait_for_device_reconnect(&transport_factory, 15, 1000)?;

        // Get device info
        let current_info = DeviceInfo::new(&ledger_api)
            .map_err(|e| FirmwareUpdateError::Other(format!("Failed to get device info: {e}")))?;

        if !current_info.is_bootloader {
            // Device exited bootloader - repair complete!
            log::info!("Device exited bootloader mode successfully");
            progress_callback(FirmwareUpdatePhase::Completed);
            return Ok(());
        }

        log::info!(
            "Device in bootloader mode. Version: {}, Target ID: {:#x}, SE Target ID: {:#x}",
            current_info.version,
            current_info.target_id,
            current_info.se_target_id
        );

        // Get the majMin (major.minor) bootloader version
        let maj_min = get_bootloader_maj_min(&current_info.version);
        log::info!(
            "Bootloader version: {}, majMin: {}",
            current_info.version,
            maj_min
        );

        // For Nano X with bootloader >= 1.4, we need to use SE target ID to query API
        // and find the correct MCU version
        let mcu_version_to_install = match maj_min.as_str() {
            // Legacy bootloader versions (Nano S primarily)
            "0.0" => "0.6".to_string(),
            "0.6" => "1.5".to_string(),
            "0.7" => "1.6".to_string(),
            "0.9" => "1.7".to_string(),
            // Modern bootloader versions - query API using SE target ID
            _ => {
                log::info!(
                    "Querying API for MCU version compatible with bootloader {}",
                    maj_min
                );
                find_mcu_for_repair_v2(&current_info)?
            }
        };

        println!(
            "Selected MCU version {mcu_version_to_install} for bootloader {maj_min} (iteration {iteration})"
        );
        log::info!(
            "Installing MCU version {} (iteration {})",
            mcu_version_to_install,
            iteration
        );

        // Flash the MCU version
        flash_mcu_by_version(
            &ledger_api,
            &current_info,
            &mcu_version_to_install,
            iteration,
            &progress_callback,
        )?;

        // After MCU flash, device reboots. Wait a bit before reconnecting.
        println!("MCU flash complete. Waiting for device to reboot...");
        progress_callback(FirmwareUpdatePhase::WaitingForBootloader);
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    Err(FirmwareUpdateError::TooManyMcuIterations)
}

/// Wait for device to reconnect after a reboot.
/// Creates new transport connections using the factory until one succeeds.
fn wait_for_device_reconnect<T>(
    transport_factory: &T,
    max_retries: u32,
    delay_ms: u64,
) -> Result<TransportNativeHID, FirmwareUpdateError>
where
    T: Fn() -> Result<TransportNativeHID, Box<dyn std::error::Error>>,
{
    for attempt in 1..=max_retries {
        match transport_factory() {
            Ok(transport) => {
                log::info!("Connected to device on attempt {}", attempt);
                return Ok(transport);
            }
            Err(e) => {
                if attempt == max_retries {
                    return Err(FirmwareUpdateError::Other(format!(
                        "Failed to reconnect to device after {max_retries} attempts: {e}"
                    )));
                }
                log::info!(
                    "Device not ready (attempt {}/{}), waiting {}ms... ({})",
                    attempt,
                    max_retries,
                    delay_ms,
                    e
                );
                println!("Waiting for device... (attempt {attempt}/{max_retries})");
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
    }
    unreachable!()
}

/// Extract major.minor version from bootloader version string.
/// E.g., "0.6" from "0.6", "1.5" from "1.5-rc3", etc.
fn get_bootloader_maj_min(version: &str) -> String {
    // Take first two parts separated by '.'
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        // Handle versions like "1.5-rc3" by taking only the number part
        let minor = parts[1].split('-').next().unwrap_or(parts[1]);
        format!("{}.{}", parts[0], minor)
    } else {
        version.to_string()
    }
}

/// Find the best MCU version to install for repair by querying the API.
/// This is the legacy version that uses bootloader version matching.
#[allow(dead_code)]
fn find_mcu_for_repair(
    _ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
) -> Result<String, FirmwareUpdateError> {
    // Get all MCU versions from API
    let mcus = get_all_mcu_versions()?;

    // Get bootloader version
    let bl_version = get_bootloader_maj_min(&device_info.version);

    // Find MCUs that can be installed from this bootloader version
    let compatible_mcus: Vec<_> = mcus
        .iter()
        .filter(|mcu| {
            // MCU must have a from_bootloader_version that matches or is compatible
            let from_bl = get_bootloader_maj_min(&mcu.from_bootloader_version);
            from_bl == bl_version || mcu.from_bootloader_version == "none"
        })
        .collect();

    if compatible_mcus.is_empty() {
        // Fall back to finding any MCU with a higher version
        let bl_parts: Vec<u32> = bl_version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        for mcu in &mcus {
            let mcu_parts: Vec<u32> = mcu.name.split('.').filter_map(|s| s.parse().ok()).collect();

            if mcu_parts.len() >= 2 && bl_parts.len() >= 2 {
                // Try MCU versions that are higher than current bootloader
                if mcu_parts[0] > bl_parts[0]
                    || (mcu_parts[0] == bl_parts[0] && mcu_parts[1] > bl_parts[1])
                {
                    return Ok(mcu.name.clone());
                }
            }
        }

        return Err(FirmwareUpdateError::Other(format!(
            "No compatible MCU found for bootloader version {bl_version}"
        )));
    }

    // Return the highest version MCU that's compatible
    let best_mcu = compatible_mcus
        .iter()
        .max_by(|a, b| {
            let a_parts: Vec<u32> = a.name.split('.').filter_map(|s| s.parse().ok()).collect();
            let b_parts: Vec<u32> = b.name.split('.').filter_map(|s| s.parse().ok()).collect();
            a_parts.cmp(&b_parts)
        })
        .ok_or_else(|| FirmwareUpdateError::Other("No compatible MCU found".to_string()))?;

    Ok(best_mcu.name.clone())
}

/// Find the best firmware/bootloader version to install for repair using SE target ID.
///
/// For modern devices like Nano X, when in bootloader mode:
/// - `target_id` is the MCU/bootloader target (e.g., 0x05010003)
/// - `se_target_id` is the Secure Element target (e.g., 0x33000004 for Nano X)
///
/// Following Ledger Live's repair logic:
/// 1. Query API with seTargetId to get device info
/// 2. Get latest firmware for the device
/// 3. Find MCU compatible with that firmware
/// 4. Compare MCU's `from_bootloader_version` with current bootloader:
///    - If same: install the MCU
///    - If different: install the bootloader version first
fn find_mcu_for_repair_v2(device_info: &DeviceInfo) -> Result<String, FirmwareUpdateError> {
    let current_bl_version = &device_info.version;
    log::info!(
        "Finding firmware for repair: se_target_id={:#x}, current_bootloader={}",
        device_info.se_target_id,
        current_bl_version
    );
    println!(
        "Querying API for device {:#x} with bootloader {}...",
        device_info.se_target_id, current_bl_version
    );

    // Use SE target ID to get the device version from API
    let device_version = get_device_version_from_api(device_info.se_target_id)?;
    log::info!(
        "Got device version: id={}, name={}, target_id={}",
        device_version.id,
        device_version.name,
        device_version.target_id
    );

    // Get all MCU versions
    let all_mcus = get_all_mcu_versions()?;
    log::info!("Got {} MCU versions from API", all_mcus.len());

    // Filter MCUs that are available (from_bootloader_version != "none" and not dev versions)
    let available_mcus: Vec<_> = all_mcus
        .iter()
        .filter(|mcu| {
            // Must have a real bootloader requirement
            let has_bl_req = mcu.from_bootloader_version != "none"
                && mcu.from_bootloader_version != "rien"
                && !mcu.from_bootloader_version.contains("noneee");

            // Filter out development/test versions (e.g., 2.99.x, 2.5-nordp2, etc.)
            let is_dev_version = mcu.name.contains("99")
                || mcu.name.contains("-nordp")
                || mcu.name.contains("-norpd")
                || mcu.name.contains("-dev")
                || mcu.name.contains("-rc");

            has_bl_req && !is_dev_version
        })
        .collect();

    log::info!(
        "Available production MCUs (with bootloader requirements): {}",
        available_mcus.len()
    );

    // Find MCUs compatible with this device version
    let device_mcus: Vec<_> = available_mcus
        .iter()
        .filter(|mcu| mcu.device_versions.contains(&device_version.id))
        .collect();

    log::info!(
        "MCUs compatible with device version {}: {}",
        device_version.id,
        device_mcus.len()
    );

    // Debug: print all device MCUs and their from_bootloader_version
    for mcu in &device_mcus {
        log::info!(
            "  MCU {} (id={}): from_bootloader_version={}",
            mcu.name,
            mcu.id,
            mcu.from_bootloader_version
        );
        println!(
            "  Available MCU {}: requires bootloader {}",
            mcu.name, mcu.from_bootloader_version
        );
    }

    // Find the best MCU (highest version number)
    let best_mcu = device_mcus.iter().max_by(|a, b| {
        let a_parts: Vec<u32> = a.name.split('.').filter_map(|s| s.parse().ok()).collect();
        let b_parts: Vec<u32> = b.name.split('.').filter_map(|s| s.parse().ok()).collect();
        a_parts.cmp(&b_parts)
    });

    if let Some(mcu) = best_mcu {
        let expected_bl = &mcu.from_bootloader_version;
        log::info!(
            "Best MCU: {} requires bootloader {}, current bootloader is {}",
            mcu.name,
            expected_bl,
            current_bl_version
        );

        // Compare bootloader versions (using major.minor)
        let expected_bl_maj_min = get_bootloader_maj_min(expected_bl);
        let current_bl_maj_min = get_bootloader_maj_min(current_bl_version);

        if expected_bl_maj_min == current_bl_maj_min {
            // Bootloader matches - install the MCU
            log::info!(
                "Bootloader version matches ({}), installing MCU {}",
                current_bl_maj_min,
                mcu.name
            );
            println!(
                "Bootloader {} matches required version, installing MCU {}",
                current_bl_maj_min, mcu.name
            );
            return Ok(mcu.name.clone());
        }
        // Bootloader doesn't match - install the required bootloader first
        log::info!(
            "Bootloader mismatch: have {}, need {}. Installing bootloader {} first.",
            current_bl_maj_min,
            expected_bl_maj_min,
            expected_bl
        );
        println!(
            "Bootloader mismatch: have {current_bl_maj_min}, need {expected_bl_maj_min}. Installing bootloader {expected_bl}..."
        );
        return Ok(expected_bl.clone());
    }

    // Fallback: no MCU found for device, try to find any MCU with matching bootloader
    log::warn!("No MCU found for device version. Trying fallback...");
    let current_bl_maj_min = get_bootloader_maj_min(current_bl_version);

    for mcu in &available_mcus {
        let from_bl = get_bootloader_maj_min(&mcu.from_bootloader_version);
        if from_bl == current_bl_maj_min {
            log::info!(
                "Fallback: found MCU {} with matching bootloader {}",
                mcu.name,
                from_bl
            );
            return Ok(mcu.name.clone());
        }
    }

    Err(FirmwareUpdateError::Other(format!(
        "No compatible firmware found for SE target {:#x} with bootloader {}. \
        Device version ID: {}, tried {} MCUs",
        device_info.se_target_id,
        current_bl_version,
        device_version.id,
        device_mcus.len()
    )))
}

/// Flash MCU by version name (e.g., "1.5", "1.6").
///
/// Note: After successful MCU flash, the device reboots which causes the
/// WebSocket connection to be reset. This is expected behavior and is
/// treated as success if progress reached 100%.
fn flash_mcu_by_version<F>(
    ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
    version: &str,
    iteration: u32,
    progress_callback: &F,
) -> Result<(), FirmwareUpdateError>
where
    F: Fn(FirmwareUpdatePhase),
{
    use std::sync::atomic::{AtomicU32, Ordering};

    // Build MCU WebSocket URL using proper URL encoding
    let url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/mcu?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("version", version)
        .append_pair("livecommonversion", LIVE_COMMON_VERSION)
        .finish();

    log::info!("Flashing MCU via WebSocket: {}", url);

    // Track progress to determine if disconnect after high progress is expected
    let last_progress = AtomicU32::new(0);

    let result = query_via_websocket_with_progress(ledger_api, &url, |event| {
        match event {
            WebSocketEvent::BulkProgress { progress, .. } => {
                // Store progress as integer percentage (0-100)
                last_progress.store((progress * 100.0) as u32, Ordering::SeqCst);
                progress_callback(FirmwareUpdatePhase::FlashingMcu {
                    iteration,
                    progress,
                });
            }
            WebSocketEvent::Success => {
                last_progress.store(100, Ordering::SeqCst);
                progress_callback(FirmwareUpdatePhase::FlashingMcu {
                    iteration,
                    progress: 1.0,
                });
            }
            WebSocketEvent::Error(e) => {
                log::error!("MCU flash error: {}", e);
            }
            _ => {}
        }
    });

    match result {
        Ok(()) => Ok(()),
        Err(FirmwareUpdateError::WebSocketError(ref msg)) => {
            let progress = last_progress.load(Ordering::SeqCst);

            // If progress was >= 90% and we got a connection reset, treat as success
            // The device reboots after successful MCU flash which disconnects WebSocket
            if progress >= 90 && (msg.contains("Connection reset") || msg.contains("closed")) {
                log::info!(
                    "MCU flash completed (progress {}%), device rebooting (expected disconnect)",
                    progress
                );
                progress_callback(FirmwareUpdatePhase::FlashingMcu {
                    iteration,
                    progress: 1.0,
                });
                Ok(())
            } else {
                log::error!("MCU flash failed at {}% progress: {}", progress, msg);
                Err(FirmwareUpdateError::WebSocketError(msg.clone()))
            }
        }
        Err(e) => Err(e),
    }
}
