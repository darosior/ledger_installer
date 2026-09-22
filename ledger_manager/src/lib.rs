//! Ledger Manager.
//!
//! This implements utility functions to manage the applications installed on your Ledger device.
//! This is performed by both talking to the Ledger device connected by USB but also by making HTTP
//! request to the Ledger API used by Ledger Live.
//!
//! # Features
//!
//! - `desktop` (default): Native USB HID transport via `ledger-transport-hidapi`
//! - `web`: `WebHID` transport for browser/WASM environments

// Constants module - APDU commands and API endpoints
pub mod constants;

// Error types module
pub mod error;

// Firmware update module - desktop only (uses minreq, tungstenite, hidapi)
#[cfg(feature = "desktop")]
pub mod firmware;

// Transport abstraction layer
pub mod transport;

pub use constants::{
    BASE_API_V1_URL, BASE_API_V2_URL, BASE_SOCKET_URL, LIVE_COMMON_VERSION, PROVIDER,
};
pub use error::{DeviceError, InstallErr, StatusCode, UpdateErr};
pub use ledger_apdu;

#[cfg(feature = "desktop")]
pub use ledger_transport_hidapi;

#[cfg(feature = "desktop")]
use {
    constants::{
        CONTINUE_LIST_APPS_COMMAND, GET_VERSION_COMMAND, LIST_APPS_COMMAND,
        OPEN_APP_COMMAND_TEMPLATE,
    },
    form_urlencoded::Serializer as UrlSerializer,
    ledger_apdu::APDUCommand,
    ledger_transport_hidapi::TransportNativeHID,
};

use serde_derive::Deserialize;
use std::str;

/// Information queried from a Ledger device.
// NOTE: MCU target id is always == target_id in Ledger Live
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub target_id: u32,
    pub version: String,
    pub flags: Vec<u8>,
    pub is_bootloader: bool,
    pub se_version: Option<String>,
    pub se_target_id: u32,
    pub mcu_version: Option<String>,
}

#[cfg(feature = "desktop")]
impl DeviceInfo {
    /// Query information about this device.
    ///
    /// Adapted from <https://github.com/LedgerHQ/ledger-live/blob/dd1d17fd3ce7ed42558204b2f93707fb9b1599de/libs/device-core/src/commands/use-cases/parseGetVersionResponse.ts>
    pub fn new(ledger_api: &TransportNativeHID) -> Result<Self, Box<dyn std::error::Error>> {
        let ver_answer = ledger_api.exchange(&GET_VERSION_COMMAND)?;
        let ret = ver_answer.retcode();
        if ret == StatusCode::LockedDevice as u16 {
            return Err("Device is locked.".into());
        } else if ret != StatusCode::OK as u16 {
            return Err(format!("Device isn't ready. Return code: {ret}.").into());
        }

        let data = ver_answer.data();
        let mut i = 0;

        if data.len() < 5 {
            return Err("Not enough data".into());
        }
        let target_id = u32::from_be_bytes(data[i..i + 4].try_into()?);
        i += 4;
        let raw_ver_len = data[i] as usize;
        i += 1;

        if data.len() < i + raw_ver_len + 1 {
            return Err("Not enough data".into());
        }
        let raw_ver = &data[i..i + raw_ver_len];
        i += raw_ver_len;
        let version = str::from_utf8(raw_ver)?;
        let flags_len = data[i] as usize;
        i += 1;

        if data.len() < i + flags_len {
            return Err("Not enough data".into());
        }
        let flags = &data[i..i + flags_len];
        i += flags_len;

        let is_bootloader = (target_id & 4026531840) != 805306368;
        Ok(if is_bootloader {
            if data.len() < i + 1 {
                return Err("Not enough data".into());
            }
            let part1_len = data[i] as usize;
            i += 1;

            if data.len() < i + part1_len {
                return Err("Not enough data".into());
            }
            let part1 = &data[i..i + part1_len];
            i += part1_len;

            if part1_len >= 5 {
                let se_version = str::from_utf8(part1)?;

                if data.len() < i + 1 {
                    return Err("Not enough data".into());
                }
                let part2_len = data[i] as usize;
                i += 1;

                if data.len() < i + part2_len {
                    return Err("Not enough data".into());
                }
                let part2 = &data[i..i + part2_len];
                //i += part2_len;
                let se_target_id = u32::from_be_bytes(
                    part2
                        .try_into()
                        .map_err(|_| "Invalid SE target ID length")?,
                );

                Self {
                    target_id,
                    version: version.to_string(),
                    flags: flags.to_vec(),
                    is_bootloader,
                    se_version: Some(se_version.to_string()),
                    se_target_id,
                    mcu_version: None,
                }
            } else {
                let se_target_id = u32::from_be_bytes(
                    part1
                        .try_into()
                        .map_err(|_| "Invalid SE target ID length")?,
                );

                Self {
                    target_id,
                    version: version.to_string(),
                    flags: flags.to_vec(),
                    is_bootloader,
                    se_version: None,
                    se_target_id,
                    mcu_version: None,
                }
            }
        } else {
            if data.len() < i + 1 {
                return Err("Not enough data".into());
            }
            let mcu_len = data[i] as usize;
            i += 1;

            if data.len() < i + mcu_len {
                return Err("Not enough data".into());
            }
            let mcu = &data[i..i + mcu_len];
            //i += mcu_len;
            let mcu = if mcu[mcu.len() - 1] == 0 {
                &mcu[..mcu.len() - 1]
            } else {
                mcu
            };
            let mcu_version = str::from_utf8(mcu)?;

            //let osu_str = b"-osu";
            //if raw_ver.windows(osu_str.len()).any(|w| w == osu_str) {}
            //TODO. See https://github.com/LedgerHQ/ledger-live/blob/dcbda65e65ead4014e767778da6022b78d8eddad/libs/ledgerjs/packages/devices/src/index.ts#L3-L156

            Self {
                target_id,
                version: version.to_string(),
                flags: flags.to_vec(),
                is_bootloader,
                se_version: Some(version.to_string()),
                se_target_id: target_id,
                mcu_version: Some(mcu_version.to_string()),
            }
        })
    }
}

/// Information about an application as queried directly from the device.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub name: String,
    pub hash: Vec<u8>,
    pub hash_code_data: Vec<u8>,
    pub blocks: u16,
    pub flags: u16,
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum HsmMessageData {
    Command(String),
    CommandList(Vec<String>),
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HsmMessage {
    pub query: String,
    pub nonce: u32,
    pub data: Option<HsmMessageData>,
}

#[cfg(feature = "desktop")]
pub(crate) fn deser_apdu_command(
    hex_str: &str,
) -> Result<APDUCommand<Vec<u8>>, Box<dyn std::error::Error>> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() < 5 {
        return Err("Invalid command".into());
    }

    let (cla, ins, p1, p2, data_len) = (bytes[0], bytes[1], bytes[2], bytes[3], bytes[4] as usize);
    if bytes.len() != 5 + data_len {
        return Err("Invalid command".into());
    }

    Ok(APDUCommand {
        cla,
        ins,
        p1,
        p2,
        data: bytes[5..].to_vec(),
    })
}

/// Some actions, such as installing apps or upgrading the firmware, are done in Ledger Live by
/// opening a socket so a remote server communicates directly with the Ledger. It appears to be
/// talking to an HSM up there which would manage sensitive actions.
/// Parameters are passed directly in the url. Don't forget to escape the necessary characters!
#[cfg(feature = "desktop")]
pub fn query_via_websocket(
    ledger_api: &TransportNativeHID,
    url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut socket, _) = tungstenite::connect(url)?;

    // https://github.com/LedgerHQ/ledger-live/blob/99879eb5bada1ecaea7a02d8886e16b44657af6d/libs/ledger-live-common/src/socket/index.ts#L95
    loop {
        let msg = socket.read()?;
        match msg {
            // It appears they only exchange JSON text messages.
            tungstenite::Message::Text(text) => {
                let msg: HsmMessage = serde_json::from_str(&text)?;

                // The dance is usually:
                // - first the HSM sends a few standalone commands;
                // - then it sends a bunch in bulk;
                // - finally it sends a success.
                if msg.query == "exchange" {
                    let command_hex = match msg.data {
                        Some(HsmMessageData::Command(h)) => h,
                        _ => return Err("A single command is expected in 'exchange' mode.".into()),
                    };
                    let command = deser_apdu_command(&command_hex)?;

                    // NOTE: the HSM expects only the data, not the last two bytes of the raw
                    // response (the status) in the "data" field below.
                    let resp = ledger_api.exchange(&command)?;
                    let response = if resp.retcode() == StatusCode::OK as u16 {
                        "success"
                    } else {
                        eprintln!(
                            "Error when installing app. Error code: {:#02x}. Resp: {:?}.",
                            resp.retcode(),
                            resp
                        );
                        "error"
                    };
                    let resp_data = hex::encode(resp.data());

                    let ws_resp = serde_json::json!({
                        "nonce": msg.nonce,
                        "response": response,
                        "data": resp_data,
                    });
                    socket.send(tungstenite::Message::Text(serde_json::to_string(&ws_resp)?))?;
                } else if msg.query == "bulk" {
                    // Ledger Live closes the socket immediately after receiving a bulk. It doesn't
                    // appear to be necessary, on the contrary if we don't we get a clean "success"
                    // response back. So we might as well do that.
                    //socket.close(None).unwrap();

                    let commands = match msg.data {
                        Some(HsmMessageData::CommandList(l)) => l,
                        _ => return Err("Expecting a list of commands in bulk mode.".into()),
                    };
                    for cmd_hex in commands {
                        if cmd_hex.is_empty() {
                            continue;
                        }
                        let command = deser_apdu_command(&cmd_hex)?;
                        let _ = ledger_api.exchange(&command)?;
                    }

                    let ws_resp = serde_json::json!({
                        "nonce": msg.nonce,
                        "response": "success",
                        "data": "",
                    });
                    socket.send(tungstenite::Message::Text(serde_json::to_string(&ws_resp)?))?;
                } else if msg.query == "success" {
                    return Ok(());
                } else if msg.query == "error" {
                    return Err(
                        format!("Got an 'error' query on the ws. Full message: {text}.").into(),
                    );
                } else if msg.query == "warning" {
                    eprintln!("Got a 'warning' query on the ws. Full message: {text}.");
                } else {
                    return Err(format!(
                        "Got an unsupported query on the ws. Full message: {text}."
                    )
                    .into());
                }
            }
            _ => {
                return Err(
                    format!("Got an unsupported message type on the ws. Message: {msg:?}.").into(),
                )
            }
        }
    }
}

/// Get a list of applications installed on this device.
#[cfg(feature = "desktop")]
pub fn list_installed_apps_raw(
    ledger_api: &TransportNativeHID,
) -> Result<Vec<InstalledApp>, Box<dyn std::error::Error>> {
    let mut answer = ledger_api.exchange(&LIST_APPS_COMMAND)?;
    let mut data = answer.data();

    // See https://github.com/LedgerHQ/ledger-live/blob/99879eb5bada1ecaea7a02d8886e16b44657af6d/libs/ledger-live-common/src/hw/listApps.ts#L9
    let mut installed_apps = Vec::new();
    while !data.is_empty() {
        let mut i = 0;
        assert_eq!(data[i], 0x01);
        i += 1;

        while i < data.len() {
            if data.len() < i + 1 + 2 + 2 + 32 + 32 + 1 {
                return Err("Not enough data".into());
            }

            let len = data[i] as usize;
            i += 1;
            let blocks = u16::from_be_bytes(data[i..i + 2].try_into()?);
            i += 2;
            let flags = u16::from_be_bytes(data[i..i + 2].try_into()?);
            i += 2;
            let hash_code_data = data[i..i + 32].to_vec();
            i += 32;
            let hash = data[i..i + 32].to_vec();
            i += 32;
            let name_len = data[i] as usize;
            i += 1;

            if data.len() < i + name_len {
                return Err("Not enough data".into());
            }
            if len != name_len + 70 {
                return Err("Invalid listApps length data.".into());
            }
            let name = str::from_utf8(&data[i..i + name_len])?.to_string();
            i += name_len;

            installed_apps.push(InstalledApp {
                name,
                hash,
                hash_code_data,
                blocks,
                flags,
            });
        }

        answer = ledger_api.exchange(&CONTINUE_LIST_APPS_COMMAND)?;
        data = answer.data();
    }

    Ok(installed_apps)
}

/// Get the metadata of the applications installed on the device. This calls the Ledger API, to
/// only query the data available from the device see `list_installed_apps_raw`.
#[cfg(feature = "desktop")]
pub fn list_installed_apps(
    ledger_api: &TransportNativeHID,
) -> Result<Vec<Option<BitcoinAppInfo>>, Box<dyn std::error::Error>> {
    let hashes = list_installed_apps_raw(ledger_api)?
        .into_iter()
        .map(|a| a.hash)
        .collect::<Vec<_>>();
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    bitcoin_apps_by_hashes(hashes)
}

/// Get the installed Bitcoin app, if any. Set `is_testnet` to look for the testnet Bitcoin app.
#[cfg(feature = "desktop")]
pub fn bitcoin_app_installed(
    ledger_api: &TransportNativeHID,
    is_testnet: bool,
) -> Result<Option<InstalledApp>, Box<dyn std::error::Error>> {
    let lowercase_app_name = if is_testnet {
        "bitcoin test"
    } else {
        "bitcoin"
    };
    Ok(list_installed_apps_raw(ledger_api)?
        .into_iter()
        .find(|app| app.name.to_lowercase() == lowercase_app_name))
}

/// Whether the Bitcoin app is installed on this device.
#[cfg(feature = "desktop")]
pub fn is_bitcoin_app_installed(
    ledger_api: &TransportNativeHID,
    is_testnet: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(bitcoin_app_installed(ledger_api, is_testnet)?.is_some())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceVersion {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FirmwareInfo {
    pub perso: String,
}

#[cfg(feature = "desktop")]
impl FirmwareInfo {
    /// Fetch firmware info from Ledger API for the given device.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or returns invalid data.
    pub fn from_device(device_info: &DeviceInfo) -> Result<Self, Box<dyn std::error::Error>> {
        let dev_ver_resp = minreq::Request::new(
            minreq::Method::Post,
            format!("{BASE_API_V1_URL}/get_device_version"),
        )
        .with_param("livecommonversion", LIVE_COMMON_VERSION)
        .with_json(&serde_json::json!({
            "provider": PROVIDER,
            "target_id": device_info.target_id,
        }))?
        .send()?;
        let device_version = dev_ver_resp.json::<DeviceVersion>()?;

        let firm_resp = minreq::Request::new(
            minreq::Method::Post,
            format!("{BASE_API_V1_URL}/get_firmware_version"),
        )
        .with_param("livecommonversion", LIVE_COMMON_VERSION)
        .with_json(&serde_json::json!({
            "provider": PROVIDER,
            "device_version": device_version.id,
            "version_name": &device_info.version,
        }))?
        .send()?;
        Ok(firm_resp.json::<FirmwareInfo>()?)
    }
}

/// Information about a Bitcoin application as queried from the Ledger API (not the Ledger device).
#[derive(Debug, Clone, Deserialize)]
pub struct BitcoinAppInfo {
    #[serde(rename = "versionName")]
    pub version_name: String,
    #[serde(rename = "versionId")]
    pub version_id: u32,
    pub version: String,
    pub perso: String,
    #[serde(rename = "deleteKey")]
    pub delete_key: String,
    pub firmware: String,
    #[serde(rename = "firmwareKey")]
    pub firmware_key: String,
    pub hash: String,
}

// Returns a Vec of Options as some elements in the response's JSON array may be `null`.
/// Get metadata about a list of Bitcoin apps identified by their hash. Elements returned seem to
/// be in the same order as the hashes, with `None` for not found.
#[cfg(feature = "desktop")]
pub fn bitcoin_apps_by_hashes(
    hashes: Vec<Vec<u8>>,
) -> Result<Vec<Option<BitcoinAppInfo>>, Box<dyn std::error::Error>> {
    if hashes.is_empty() {
        let e: Vec<Option<BitcoinAppInfo>> = Vec::new();
        return Ok(e);
    }
    let hashes_hex: Vec<_> = hashes.into_iter().map(|h| hex::encode(&h).into()).collect();
    let resp_apps =
        minreq::Request::new(minreq::Method::Post, format!("{BASE_API_V2_URL}/apps/hash"))
            .with_param("livecommonversion", LIVE_COMMON_VERSION)
            .with_json(&serde_json::Value::Array(hashes_hex))?
            .send()?;
    Ok(resp_apps.json::<Vec<_>>()?.into_iter().collect())
}

/// Get the Bitcoin apps information for this device from the "catalog" (as Ledger Live calls it).
// This uses the v2 API. See for reference:
// - https://github.com/LedgerHQ/ledger-live/blob/5a0a1aa5dc183116839851b79bceb6704f1de4b9/libs/ledger-live-common/src/apps/listApps/v2.ts
// - https://github.com/LedgerHQ/ledger-live/blob/5a0a1aa5dc183116839851b79bceb6704f1de4b9/libs/device-core/src/managerApi/repositories/HttpManagerApiRepository.ts#L211
// There is also another way which seems to be the API v1 way of getting the app info. See
// https://github.com/LedgerHQ/ledger-live/blob/99879eb5bada1ecaea7a02d8886e16b44657af6d/libs/ledger-live-common/src/manager/index.ts#L103-L104.
#[cfg(feature = "desktop")]
pub fn get_latest_apps(
    device_info: &DeviceInfo,
) -> Result<(Option<BitcoinAppInfo>, Option<BitcoinAppInfo>), Box<dyn std::error::Error>> {
    let mut bitcoin = None;
    let mut test = None;

    let resp_apps = minreq::Request::new(
        minreq::Method::Get,
        format!("{BASE_API_V2_URL}/apps/by-target"),
    )
    .with_param("livecommonversion", LIVE_COMMON_VERSION)
    .with_param("provider", PROVIDER.to_string())
    .with_param("target_id", device_info.target_id.to_string())
    .with_param("firmware_version_name", device_info.version.clone())
    .send()?;
    resp_apps
        .json::<Vec<BitcoinAppInfo>>()?
        .into_iter()
        .for_each(|app| {
            // FIXME: is versionName guaranteed to be the name? What's "version" for?
            if app.version_name.to_lowercase() == "bitcoin" {
                bitcoin = Some(app);
            } else if app.version_name.to_lowercase() == "bitcoin test" {
                test = Some(app);
            }
        });

    Ok((bitcoin, test))
}

/// Get the Bitcoin app information for this device from the "catalog" (as Ledger Live calls it).
/// Set `is_testnet` to `true` to get the Test app instead.
#[cfg(feature = "desktop")]
pub fn bitcoin_latest_app(
    device_info: &DeviceInfo,
    is_testnet: bool,
) -> Result<Option<BitcoinAppInfo>, Box<dyn std::error::Error>> {
    let apps = get_latest_apps(device_info)?;
    Ok(if is_testnet { apps.1 } else { apps.0 })
}

/// Open the given application on the device.
#[cfg(feature = "desktop")]
pub fn open_bitcoin_app(
    ledger_api: &TransportNativeHID,
    is_testnet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = OPEN_APP_COMMAND_TEMPLATE;
    command.data = if is_testnet {
        b"Bitcoin Test"
    } else {
        b"Bitcoin"
    };

    let resp = ledger_api.exchange(&command)?;
    if resp.retcode() != StatusCode::OK as u16 {
        return Err(format!("Error opening app. Ledger response: {resp:#x?}.").into());
    }

    Ok(())
}

/// Check whether the Ledger device is genuine.
#[cfg(feature = "desktop")]
pub fn genuine_check(ledger_api: &TransportNativeHID) -> Result<(), Box<dyn std::error::Error>> {
    let device_info = DeviceInfo::new(ledger_api)?;
    let firmware_info = FirmwareInfo::from_device(&device_info)?;

    let genuine_ws_url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/genuine?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("perso", &firmware_info.perso)
        .finish();
    query_via_websocket(ledger_api, &genuine_ws_url)
}

#[cfg(feature = "desktop")]
fn install_app(
    ledger_api: &TransportNativeHID,
    device_info: &DeviceInfo,
    app: &BitcoinAppInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    // Make sure to properly escape the parameters in the request's parameter.
    let install_ws_url = UrlSerializer::new(format!("{BASE_SOCKET_URL}/install?"))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("perso", &app.perso)
        .append_pair("deleteKey", &app.delete_key)
        .append_pair("firmware", &app.firmware)
        .append_pair("firmwareKey", &app.firmware_key)
        .append_pair("hash", &app.hash)
        .finish();
    query_via_websocket(ledger_api, &install_ws_url)
}

/// Install the Bitcoin application on this device. Set `is_testnet` to `true` to install the
/// testnet app instead.
#[cfg(feature = "desktop")]
pub fn install_bitcoin_app(
    ledger_api: &TransportNativeHID,
    is_testnet: bool,
) -> Result<(), InstallErr> {
    // First of all make sure it's not already installed.
    if is_bitcoin_app_installed(ledger_api, is_testnet).map_err(InstallErr::Any)? {
        return Err(InstallErr::AlreadyInstalled);
    }

    // Get the app info, necessary for the websocket query below.
    let device_info = DeviceInfo::new(ledger_api).map_err(InstallErr::Any)?;
    let bitcoin_app = bitcoin_latest_app(&device_info, is_testnet)
        .map_err(InstallErr::Any)?
        .ok_or(InstallErr::AppNotFound)?;

    // Now install the app by connecting through their websocket thing to their HSM.
    install_app(ledger_api, &device_info, &bitcoin_app).map_err(InstallErr::Any)?;

    Ok(())
}

/// Update the Bitcoin application on this device. Set `is_testnet` to `true` to install the
/// testnet app instead.
#[cfg(feature = "desktop")]
pub fn update_bitcoin_app(
    ledger_api: &TransportNativeHID,
    is_testnet: bool,
) -> Result<(), UpdateErr> {
    // First of all make sure the app is installed. Get its details.
    let app = bitcoin_app_installed(ledger_api, is_testnet)
        .map_err(UpdateErr::Any)?
        .ok_or(UpdateErr::NotInstalled)?;
    let installed_app = bitcoin_apps_by_hashes(vec![app.hash])
        .map_err(UpdateErr::Any)?
        .into_iter()
        .next()
        .ok_or(UpdateErr::AppNotFound)?;

    // Get the latest app info, necessary for the websocket query below.
    let device_info = DeviceInfo::new(ledger_api).map_err(UpdateErr::Any)?;
    let latest_app = bitcoin_latest_app(&device_info, is_testnet)
        .map_err(UpdateErr::Any)?
        .ok_or(UpdateErr::AppNotFound)?;

    // It doesn't make a whole lot of sense to not check the version is indeed superior to the
    // version of the installed app. But this is the check Ledger Live does. And it also never uses
    // versionId as far as i can tell. So, do like Ledger.
    if installed_app.is_some_and(|app| app.version == latest_app.version) {
        return Err(UpdateErr::AlreadyLatest);
    }

    // Now install the app by connecting through their websocket thing to their HSM.
    install_app(ledger_api, &device_info, &latest_app).map_err(UpdateErr::Any)?;

    Ok(())
}
