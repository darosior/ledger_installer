//! Internal types for the web crate.
//!
//! These types are used for API responses and internal communication,
//! not exposed via wasm_bindgen.

use serde::Deserialize;

/// API response for device version lookup.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DeviceVersion {
    pub id: i64,
}

/// API response for firmware info.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FirmwareInfoResponse {
    pub perso: String,
}

/// Bitcoin app metadata from Ledger API.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BitcoinAppInfo {
    #[serde(rename = "versionName")]
    pub version_name: String,
    #[allow(dead_code)]
    pub version: String,
    pub perso: String,
    #[serde(rename = "deleteKey")]
    pub delete_key: String,
    pub firmware: String,
    #[serde(rename = "firmwareKey")]
    pub firmware_key: String,
    pub hash: String,
}

/// HSM WebSocket message data variants.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum HsmMessageData {
    Command(String),
    CommandList(Vec<String>),
}

/// HSM WebSocket message structure.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HsmMessage {
    pub query: String,
    pub nonce: u32,
    pub data: Option<HsmMessageData>,
}

/// Response from /get_device_version endpoint.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DeviceVersionResponseApi {
    pub id: i64,
    #[serde(default)]
    pub se_firmware_final_versions: Vec<i64>,
}

/// Response from /get_firmware_version endpoint for getting current version ID.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FirmwareVersionIdResponse {
    pub id: i64,
}

/// OSU firmware metadata.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OsuFirmwareApi {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub perso: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub firmware_key: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub next_se_firmware_final_version: i64,
}

/// Response from /get_latest_firmware endpoint.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LatestFirmwareResponseApi {
    pub result: String,
    #[serde(default)]
    pub se_firmware_osu_version: Option<OsuFirmwareApi>,
}

/// Final firmware metadata.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FinalFirmwareApi {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub perso: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub firmware_key: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub mcu_versions: Vec<i64>,
}
