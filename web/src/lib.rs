//! Web WASM bindings for Ledger Manager.
//!
//! This crate provides JavaScript-callable functions for managing Ledger devices
//! from a web browser using the WebHID API.

mod apdu;
mod parsing;
mod types;

use apdu::{deser_apdu_command, CONTINUE_LIST_APPS, GET_VERSION, LIST_APPS};
use gloo_net::http::Request;
use gloo_net::websocket::{futures::WebSocket, Message};
use ledger_manager::ledger_apdu::APDUCommand;
use ledger_manager::transport::{is_webhid_supported, WebHidTransport};
use ledger_manager::{
    BASE_API_V1_URL, BASE_API_V2_URL, BASE_SOCKET_URL, LIVE_COMMON_VERSION, PROVIDER,
};
use parsing::{parse_installed_apps, parse_version_info};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use types::{
    BitcoinAppInfo, DeviceVersion, DeviceVersionResponseApi, FinalFirmwareApi,
    FirmwareInfoResponse, FirmwareVersionIdResponse, HsmMessage, HsmMessageData,
    LatestFirmwareResponseApi, OsuFirmwareApi,
};
use wasm_bindgen::prelude::*;

// Global transport instance (stored between calls)
thread_local! {
    static TRANSPORT: RefCell<Option<Rc<WebHidTransport>>> = const { RefCell::new(None) };
}

/// Initialize panic hook for better error messages in console.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Device information returned from get_device_info.
#[derive(Serialize, Deserialize)]
pub struct DeviceInfo {
    pub connected: bool,
    pub model: Option<String>,
    pub version: Option<String>,
    pub mcu_version: Option<String>,
    pub target_id: Option<u32>,
    pub bitcoin_installed: bool,
    pub bitcoin_version: Option<String>,
    pub bitcoin_test_installed: bool,
    pub bitcoin_test_version: Option<String>,
}

/// Result of an operation.
#[derive(Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
}

/// Check if WebHID is supported in the current browser.
#[wasm_bindgen]
pub fn check_webhid_support() -> bool {
    is_webhid_supported()
}

/// Request access to a Ledger device.
#[wasm_bindgen]
pub async fn connect_device() -> Result<JsValue, JsValue> {
    match WebHidTransport::request_device().await {
        Ok(transport) => {
            let transport = Rc::new(transport);
            TRANSPORT.with(|t| {
                *t.borrow_mut() = Some(transport);
            });

            let result = OperationResult {
                success: true,
                message: "Device connected successfully".to_string(),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Failed to connect: {}", e),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}

/// Stored device info for API calls
#[derive(Clone)]
struct StoredDeviceInfo {
    target_id: u32,
    version: String,
    bitcoin_installed: bool,
    bitcoin_version: Option<String>,
    bitcoin_test_installed: bool,
    bitcoin_test_version: Option<String>,
}

thread_local! {
    static DEVICE_INFO: RefCell<Option<StoredDeviceInfo>> = const { RefCell::new(None) };
}

/// Query app info by hashes from Ledger API
async fn get_apps_by_hashes(hashes: Vec<Vec<u8>>) -> Result<Vec<Option<BitcoinAppInfo>>, String> {
    if hashes.is_empty() {
        return Ok(Vec::new());
    }

    let hashes_hex: Vec<serde_json::Value> = hashes
        .into_iter()
        .map(|h| serde_json::Value::String(hex::encode(&h)))
        .collect();

    let url = format!(
        "{}/apps/hash?livecommonversion={}",
        BASE_API_V2_URL, LIVE_COMMON_VERSION
    );

    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::Value::Array(hashes_hex).to_string())
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Failed to get apps by hash: {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

/// Get device information.
#[wasm_bindgen]
pub async fn get_device_info() -> Result<JsValue, JsValue> {
    let transport = TRANSPORT.with(|t| t.borrow().clone());

    let transport = match transport {
        Some(t) if t.is_device_open() => t,
        _ => match WebHidTransport::connect_authorized().await {
            Ok(t) => {
                let t = Rc::new(t);
                TRANSPORT.with(|tr| {
                    *tr.borrow_mut() = Some(t.clone());
                });
                t
            }
            Err(_) => {
                let info = DeviceInfo {
                    connected: false,
                    model: None,
                    version: None,
                    mcu_version: None,
                    target_id: None,
                    bitcoin_installed: false,
                    bitcoin_version: None,
                    bitcoin_test_installed: false,
                    bitcoin_test_version: None,
                };
                return Ok(serde_wasm_bindgen::to_value(&info)?);
            }
        },
    };

    // Get version info
    let cmd = APDUCommand {
        cla: GET_VERSION.cla,
        ins: GET_VERSION.ins,
        p1: GET_VERSION.p1,
        p2: GET_VERSION.p2,
        data: vec![],
    };

    let (target_id, version, mcu_version) = match transport.exchange_async(&cmd).await {
        Ok(answer) => {
            if answer.retcode() == 0x9000 {
                parse_version_info(answer.data()).unwrap_or((0, String::new(), None))
            } else {
                (0, String::new(), None)
            }
        }
        Err(_) => (0, String::new(), None),
    };

    // Get installed apps
    let cmd = APDUCommand {
        cla: LIST_APPS.cla,
        ins: LIST_APPS.ins,
        p1: LIST_APPS.p1,
        p2: LIST_APPS.p2,
        data: vec![],
    };

    let mut all_apps = Vec::new();

    if let Ok(answer) = transport.exchange_async(&cmd).await {
        if answer.retcode() == 0x9000 {
            all_apps.extend(parse_installed_apps(answer.data()));

            loop {
                let cmd = APDUCommand {
                    cla: CONTINUE_LIST_APPS.cla,
                    ins: CONTINUE_LIST_APPS.ins,
                    p1: CONTINUE_LIST_APPS.p1,
                    p2: CONTINUE_LIST_APPS.p2,
                    data: vec![],
                };

                if let Ok(answer) = transport.exchange_async(&cmd).await {
                    let data = answer.data();
                    if data.is_empty() {
                        break;
                    }
                    all_apps.extend(parse_installed_apps(data));
                } else {
                    break;
                }
            }
        }
    }

    // Find Bitcoin and Bitcoin Test apps and their hashes
    let bitcoin_app = all_apps
        .iter()
        .find(|(name, _)| name.to_lowercase() == "bitcoin");
    let bitcoin_test_app = all_apps
        .iter()
        .find(|(name, _)| name.to_lowercase() == "bitcoin test");

    let bitcoin_installed = bitcoin_app.is_some();
    let bitcoin_test_installed = bitcoin_test_app.is_some();

    // Get version info by querying API with hashes
    let mut bitcoin_version = None;
    let mut bitcoin_test_version = None;

    let mut hashes_to_query = Vec::new();
    if let Some((_, hash)) = bitcoin_app {
        hashes_to_query.push(hash.clone());
    }
    if let Some((_, hash)) = bitcoin_test_app {
        hashes_to_query.push(hash.clone());
    }

    if !hashes_to_query.is_empty() {
        if let Ok(app_infos) = get_apps_by_hashes(hashes_to_query).await {
            for app_info in app_infos.into_iter().flatten() {
                let name_lower = app_info.version_name.to_lowercase();
                if name_lower == "bitcoin" {
                    bitcoin_version = Some(app_info.version);
                } else if name_lower == "bitcoin test" {
                    bitcoin_test_version = Some(app_info.version);
                }
            }
        }
    }

    // Store device info for later API calls
    if target_id != 0 && !version.is_empty() {
        DEVICE_INFO.with(|d| {
            *d.borrow_mut() = Some(StoredDeviceInfo {
                target_id,
                version: version.clone(),
                bitcoin_installed,
                bitcoin_version: bitcoin_version.clone(),
                bitcoin_test_installed,
                bitcoin_test_version: bitcoin_test_version.clone(),
            });
        });
    }

    let info = DeviceInfo {
        connected: true,
        model: Some("Ledger".to_string()),
        version: if version.is_empty() {
            None
        } else {
            Some(version)
        },
        mcu_version,
        target_id: if target_id != 0 {
            Some(target_id)
        } else {
            None
        },
        bitcoin_installed,
        bitcoin_version,
        bitcoin_test_installed,
        bitcoin_test_version,
    };

    Ok(serde_wasm_bindgen::to_value(&info)?)
}

/// Open the Bitcoin app on the device.
#[wasm_bindgen]
pub async fn open_bitcoin_app(testnet: bool) -> Result<JsValue, JsValue> {
    let app_name = if testnet { "Bitcoin Test" } else { "Bitcoin" };

    let transport = TRANSPORT.with(|t| t.borrow().clone());

    let transport = match transport {
        Some(t) if t.is_device_open() => t,
        _ => {
            let result = OperationResult {
                success: false,
                message: "Device not connected. Please connect first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    let cmd = APDUCommand {
        cla: 0xe0,
        ins: 0xd8,
        p1: 0x00,
        p2: 0x00,
        data: app_name.as_bytes().to_vec(),
    };

    match transport.exchange_async(&cmd).await {
        Ok(answer) => {
            if answer.retcode() == 0x9000 {
                let result = OperationResult {
                    success: true,
                    message: format!("{} app opened successfully", app_name),
                };
                Ok(serde_wasm_bindgen::to_value(&result)?)
            } else {
                let result = OperationResult {
                    success: false,
                    message: format!(
                        "Failed to open {} app. Error code: 0x{:04x}",
                        app_name,
                        answer.retcode()
                    ),
                };
                Ok(serde_wasm_bindgen::to_value(&result)?)
            }
        }
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Failed to open {} app: {}", app_name, e),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}

/// Get firmware info from Ledger API
async fn get_firmware_info(target_id: u32, version: &str) -> Result<String, String> {
    // First get device version ID
    let dev_ver_url = format!(
        "{}/get_device_version?livecommonversion={}",
        BASE_API_V1_URL, LIVE_COMMON_VERSION
    );

    let dev_ver_body = serde_json::json!({
        "provider": PROVIDER,
        "target_id": target_id,
    });

    let dev_ver_resp = Request::post(&dev_ver_url)
        .header("Content-Type", "application/json")
        .body(dev_ver_body.to_string())
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !dev_ver_resp.ok() {
        return Err(format!(
            "Failed to get device version: {}",
            dev_ver_resp.status()
        ));
    }

    let device_version: DeviceVersion = dev_ver_resp.json().await.map_err(|e| e.to_string())?;

    // Now get firmware info
    let firm_url = format!(
        "{}/get_firmware_version?livecommonversion={}",
        BASE_API_V1_URL, LIVE_COMMON_VERSION
    );

    let firm_body = serde_json::json!({
        "provider": PROVIDER,
        "device_version": device_version.id,
        "version_name": version,
    });

    let firm_resp = Request::post(&firm_url)
        .header("Content-Type", "application/json")
        .body(firm_body.to_string())
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !firm_resp.ok() {
        return Err(format!(
            "Failed to get firmware info: {}",
            firm_resp.status()
        ));
    }

    let firmware_info: FirmwareInfoResponse = firm_resp.json().await.map_err(|e| e.to_string())?;
    Ok(firmware_info.perso)
}

/// Get Bitcoin app info from Ledger API
async fn get_bitcoin_app_info(
    target_id: u32,
    version: &str,
    testnet: bool,
) -> Result<BitcoinAppInfo, String> {
    let url = format!(
        "{}/apps/by-target?livecommonversion={}&provider={}&target_id={}&firmware_version_name={}",
        BASE_API_V2_URL, LIVE_COMMON_VERSION, PROVIDER, target_id, version
    );

    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Failed to get app info: {}", resp.status()));
    }

    let apps: Vec<BitcoinAppInfo> = resp.json().await.map_err(|e| e.to_string())?;

    let app_name = if testnet { "bitcoin test" } else { "bitcoin" };
    apps.into_iter()
        .find(|app| app.version_name.to_lowercase() == app_name)
        .ok_or_else(|| "Bitcoin app not found for this device".to_string())
}

/// Execute WebSocket communication with Ledger HSM
async fn query_via_websocket(transport: Rc<WebHidTransport>, url: &str) -> Result<(), String> {
    use futures::{SinkExt, StreamExt};

    let ws =
        WebSocket::open(url).map_err(|e| format!("Failed to connect to WebSocket: {:?}", e))?;
    let (mut write, mut read) = ws.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let hsm_msg: HsmMessage = serde_json::from_str(&text)
                    .map_err(|e| format!("Failed to parse HSM message: {}", e))?;

                if hsm_msg.query == "exchange" {
                    let command_hex = match hsm_msg.data {
                        Some(HsmMessageData::Command(h)) => h,
                        _ => return Err("Expected single command in exchange mode".into()),
                    };
                    let command = deser_apdu_command(&command_hex)?;

                    let resp = transport
                        .exchange_async(&command)
                        .await
                        .map_err(|e| format!("APDU exchange failed: {}", e))?;

                    let response = if resp.retcode() == 0x9000 {
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

                    write
                        .send(Message::Text(ws_resp.to_string()))
                        .await
                        .map_err(|e| format!("Failed to send WebSocket message: {:?}", e))?;
                } else if hsm_msg.query == "bulk" {
                    let commands = match hsm_msg.data {
                        Some(HsmMessageData::CommandList(l)) => l,
                        _ => return Err("Expected command list in bulk mode".into()),
                    };

                    for cmd_hex in commands {
                        if cmd_hex.is_empty() {
                            continue;
                        }
                        let command = deser_apdu_command(&cmd_hex)?;
                        let _ = transport
                            .exchange_async(&command)
                            .await
                            .map_err(|e| format!("APDU exchange failed: {}", e))?;
                    }

                    let ws_resp = serde_json::json!({
                        "nonce": hsm_msg.nonce,
                        "response": "success",
                        "data": "",
                    });

                    write
                        .send(Message::Text(ws_resp.to_string()))
                        .await
                        .map_err(|e| format!("Failed to send WebSocket message: {:?}", e))?;
                } else if hsm_msg.query == "success" {
                    return Ok(());
                } else if hsm_msg.query == "error" {
                    return Err(format!("HSM returned error: {}", text));
                } else if hsm_msg.query == "warning" {
                    // Log warning but continue
                    web_sys::console::warn_1(&JsValue::from_str(&format!("HSM warning: {}", text)));
                }
            }
            Ok(Message::Bytes(_)) => {
                return Err("Unexpected binary message from WebSocket".into());
            }
            Err(e) => {
                return Err(format!("WebSocket error: {:?}", e));
            }
        }
    }

    Err("WebSocket closed unexpectedly".into())
}

/// Perform genuine check on the device.
#[wasm_bindgen]
pub async fn genuine_check() -> Result<JsValue, JsValue> {
    let transport = TRANSPORT.with(|t| t.borrow().clone());
    let device_info = DEVICE_INFO.with(|d| d.borrow().clone());

    let transport = match transport {
        Some(t) if t.is_device_open() => t,
        _ => {
            let result = OperationResult {
                success: false,
                message: "Device not connected. Please connect and get device info first."
                    .to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    let device_info = match device_info {
        Some(info) => info,
        None => {
            let result = OperationResult {
                success: false,
                message: "Device info not available. Please get device info first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Get firmware perso from API
    let perso = match get_firmware_info(device_info.target_id, &device_info.version).await {
        Ok(p) => p,
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Failed to get firmware info: {}", e),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Build WebSocket URL for genuine check
    let ws_url = format!(
        "{}/genuine?targetId={}&perso={}",
        BASE_SOCKET_URL,
        device_info.target_id,
        urlencoding::encode(&perso)
    );

    match query_via_websocket(transport, &ws_url).await {
        Ok(()) => {
            let result = OperationResult {
                success: true,
                message: "Device is genuine!".to_string(),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Genuine check failed: {}", e),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}

/// Install the Bitcoin app on the device (only if not already installed).
#[wasm_bindgen]
pub async fn install_bitcoin_app(testnet: bool) -> Result<JsValue, JsValue> {
    let app_name = if testnet { "Bitcoin Test" } else { "Bitcoin" };

    let transport = TRANSPORT.with(|t| t.borrow().clone());
    let device_info = DEVICE_INFO.with(|d| d.borrow().clone());

    let transport = match transport {
        Some(t) if t.is_device_open() => t,
        _ => {
            let result = OperationResult {
                success: false,
                message: "Device not connected. Please connect and get device info first."
                    .to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    let device_info = match device_info {
        Some(info) => info,
        None => {
            let result = OperationResult {
                success: false,
                message: "Device info not available. Please get device info first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Check if app is already installed
    let is_installed = if testnet {
        device_info.bitcoin_test_installed
    } else {
        device_info.bitcoin_installed
    };

    if is_installed {
        let result = OperationResult {
            success: false,
            message: format!("{} app is already installed. Use update instead.", app_name),
        };
        return Ok(serde_wasm_bindgen::to_value(&result)?);
    }

    // Get Bitcoin app info from API
    let app_info =
        match get_bitcoin_app_info(device_info.target_id, &device_info.version, testnet).await {
            Ok(info) => info,
            Err(e) => {
                let result = OperationResult {
                    success: false,
                    message: format!("Failed to get {} app info: {}", app_name, e),
                };
                return Ok(serde_wasm_bindgen::to_value(&result)?);
            }
        };

    // Build WebSocket URL for install
    let ws_url = format!(
        "{}/install?targetId={}&perso={}&deleteKey={}&firmware={}&firmwareKey={}&hash={}",
        BASE_SOCKET_URL,
        device_info.target_id,
        urlencoding::encode(&app_info.perso),
        urlencoding::encode(&app_info.delete_key),
        urlencoding::encode(&app_info.firmware),
        urlencoding::encode(&app_info.firmware_key),
        urlencoding::encode(&app_info.hash)
    );

    match query_via_websocket(transport, &ws_url).await {
        Ok(()) => {
            let result = OperationResult {
                success: true,
                message: format!("{} app installed successfully!", app_name),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Failed to install {} app: {}", app_name, e),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}

// ============================================================================
// App Update Types and Functions
// ============================================================================

/// App update info returned to JavaScript
#[derive(Serialize, Deserialize)]
pub struct AppUpdateInfo {
    pub installed: bool,
    pub update_available: bool,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub message: String,
}

/// Check if a Bitcoin app update is available.
#[wasm_bindgen]
pub async fn check_bitcoin_app_update(testnet: bool) -> Result<JsValue, JsValue> {
    let app_name = if testnet { "Bitcoin Test" } else { "Bitcoin" };

    let device_info = DEVICE_INFO.with(|d| d.borrow().clone());

    let device_info = match device_info {
        Some(info) => info,
        None => {
            let result = AppUpdateInfo {
                installed: false,
                update_available: false,
                current_version: None,
                latest_version: None,
                message: "Device info not available. Please get device info first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    let (is_installed, current_version) = if testnet {
        (
            device_info.bitcoin_test_installed,
            device_info.bitcoin_test_version.clone(),
        )
    } else {
        (
            device_info.bitcoin_installed,
            device_info.bitcoin_version.clone(),
        )
    };

    // Get latest app info from API
    let latest_app_info =
        match get_bitcoin_app_info(device_info.target_id, &device_info.version, testnet).await {
            Ok(info) => info,
            Err(e) => {
                let result = AppUpdateInfo {
                    installed: is_installed,
                    update_available: false,
                    current_version,
                    latest_version: None,
                    message: format!("Failed to get latest {} app info: {}", app_name, e),
                };
                return Ok(serde_wasm_bindgen::to_value(&result)?);
            }
        };

    let latest_version = latest_app_info.version.clone();

    if !is_installed {
        let result = AppUpdateInfo {
            installed: false,
            update_available: false,
            current_version: None,
            latest_version: Some(latest_version),
            message: format!(
                "{} app is not installed. Latest version: {}",
                app_name, latest_app_info.version
            ),
        };
        return Ok(serde_wasm_bindgen::to_value(&result)?);
    }

    // Compare versions
    let update_available = match &current_version {
        Some(current) => current != &latest_version,
        None => true, // If we can't get current version, assume update is needed
    };

    let result = if update_available {
        let msg = format!(
            "{} update available: {} -> {}",
            app_name,
            current_version.as_deref().unwrap_or("unknown"),
            latest_version
        );
        AppUpdateInfo {
            installed: true,
            update_available: true,
            current_version,
            latest_version: Some(latest_version),
            message: msg,
        }
    } else {
        AppUpdateInfo {
            installed: true,
            update_available: false,
            current_version,
            latest_version: Some(latest_version),
            message: format!("{} app is up to date", app_name),
        }
    };

    Ok(serde_wasm_bindgen::to_value(&result)?)
}

/// Update the Bitcoin app on the device (only if already installed).
#[wasm_bindgen]
pub async fn update_bitcoin_app(testnet: bool) -> Result<JsValue, JsValue> {
    let app_name = if testnet { "Bitcoin Test" } else { "Bitcoin" };

    let transport = TRANSPORT.with(|t| t.borrow().clone());
    let device_info = DEVICE_INFO.with(|d| d.borrow().clone());

    let transport = match transport {
        Some(t) if t.is_device_open() => t,
        _ => {
            let result = OperationResult {
                success: false,
                message: "Device not connected. Please connect and get device info first."
                    .to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    let device_info = match device_info {
        Some(info) => info,
        None => {
            let result = OperationResult {
                success: false,
                message: "Device info not available. Please get device info first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Check if app is installed
    let (is_installed, current_version) = if testnet {
        (
            device_info.bitcoin_test_installed,
            device_info.bitcoin_test_version.clone(),
        )
    } else {
        (
            device_info.bitcoin_installed,
            device_info.bitcoin_version.clone(),
        )
    };

    if !is_installed {
        let result = OperationResult {
            success: false,
            message: format!("{} app is not installed. Use install instead.", app_name),
        };
        return Ok(serde_wasm_bindgen::to_value(&result)?);
    }

    // Get Bitcoin app info from API
    let app_info =
        match get_bitcoin_app_info(device_info.target_id, &device_info.version, testnet).await {
            Ok(info) => info,
            Err(e) => {
                let result = OperationResult {
                    success: false,
                    message: format!("Failed to get {} app info: {}", app_name, e),
                };
                return Ok(serde_wasm_bindgen::to_value(&result)?);
            }
        };

    // Check if update is needed
    if let Some(current) = &current_version {
        if current == &app_info.version {
            let result = OperationResult {
                success: true,
                message: format!(
                    "{} app is already at the latest version ({})",
                    app_name, current
                ),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    }

    // Build WebSocket URL for install (update uses same endpoint)
    let ws_url = format!(
        "{}/install?targetId={}&perso={}&deleteKey={}&firmware={}&firmwareKey={}&hash={}",
        BASE_SOCKET_URL,
        device_info.target_id,
        urlencoding::encode(&app_info.perso),
        urlencoding::encode(&app_info.delete_key),
        urlencoding::encode(&app_info.firmware),
        urlencoding::encode(&app_info.firmware_key),
        urlencoding::encode(&app_info.hash)
    );

    match query_via_websocket(transport, &ws_url).await {
        Ok(()) => {
            let result = OperationResult {
                success: true,
                message: format!(
                    "{} app updated successfully to version {}!",
                    app_name, app_info.version
                ),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
        Err(e) => {
            let result = OperationResult {
                success: false,
                message: format!("Failed to update {} app: {}", app_name, e),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}

/// Log a message to the browser console.
#[wasm_bindgen]
pub fn log(message: &str) {
    web_sys::console::log_1(&JsValue::from_str(message));
}

// ============================================================================
// Firmware Update Types and Functions
// ============================================================================

/// Firmware update info returned to JavaScript
#[derive(Serialize, Deserialize)]
pub struct FirmwareUpdateInfo {
    pub update_available: bool,
    pub current_version: Option<String>,
    pub target_version: Option<String>,
    pub message: String,
}

/// Generate a firmware salt (pseudo-random)
fn generate_firmware_salt() -> String {
    use js_sys::Date;
    let timestamp = Date::now() as u64;
    let hash_input = format!("{}|firmwareSalt", timestamp);
    let bytes = hash_input.as_bytes();
    let mut hash: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        hash = hash.wrapping_add((b as u32).wrapping_mul((i as u32).wrapping_add(1)));
        hash = hash.wrapping_mul(31);
    }
    format!("{:06x}", hash & 0xFFFFFF)
}

/// Get the device version info from the Ledger API
async fn get_device_version_from_api_web(
    target_id: u32,
) -> Result<DeviceVersionResponseApi, String> {
    let url = format!(
        "{}/get_device_version?livecommonversion={}&provider={}&target_id={}",
        BASE_API_V1_URL, LIVE_COMMON_VERSION, PROVIDER, target_id
    );

    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "HTTP {}: Failed to get device version",
            resp.status()
        ));
    }

    resp.json().await.map_err(|e| e.to_string())
}

/// Get the current firmware version ID
async fn get_current_firmware_version_id_web(
    device_version_id: i64,
    version_name: &str,
) -> Result<i64, String> {
    let url = format!(
        "{}/get_firmware_version?livecommonversion={}&device_version={}&version_name={}&provider={}",
        BASE_API_V1_URL, LIVE_COMMON_VERSION, device_version_id, version_name, PROVIDER
    );

    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "HTTP {}: Failed to get firmware version",
            resp.status()
        ));
    }

    let firmware_info: FirmwareVersionIdResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(firmware_info.id)
}

/// Get the latest available firmware
async fn get_latest_firmware_from_api_web(
    current_version: i64,
    device_version: i64,
) -> Result<Option<OsuFirmwareApi>, String> {
    let salt = generate_firmware_salt();

    let url = format!(
        "{}/get_latest_firmware?livecommonversion={}&salt={}&current_se_firmware_final_version={}&device_version={}&provider={}",
        BASE_API_V1_URL, LIVE_COMMON_VERSION, salt, current_version, device_version, PROVIDER
    );

    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "HTTP {}: Failed to get latest firmware",
            resp.status()
        ));
    }

    let latest: LatestFirmwareResponseApi = resp.json().await.map_err(|e| e.to_string())?;
    Ok(latest.se_firmware_osu_version)
}

/// Get final firmware by ID
async fn get_final_firmware_by_id_web(id: i64) -> Result<FinalFirmwareApi, String> {
    let url = format!(
        "{}/firmware_final_versions/{}?livecommonversion={}",
        BASE_API_V1_URL, id, LIVE_COMMON_VERSION
    );

    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "HTTP {}: Failed to get final firmware",
            resp.status()
        ));
    }

    resp.json().await.map_err(|e| e.to_string())
}

/// Check if a firmware update is available for the connected device.
#[wasm_bindgen]
pub async fn check_firmware_update() -> Result<JsValue, JsValue> {
    let device_info = DEVICE_INFO.with(|d| d.borrow().clone());

    let device_info = match device_info {
        Some(info) => info,
        None => {
            let result = FirmwareUpdateInfo {
                update_available: false,
                current_version: None,
                target_version: None,
                message: "Device info not available. Please get device info first.".to_string(),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Step 1: Get device version from API
    let device_version_api = match get_device_version_from_api_web(device_info.target_id).await {
        Ok(v) => v,
        Err(e) => {
            let result = FirmwareUpdateInfo {
                update_available: false,
                current_version: Some(device_info.version.clone()),
                target_version: None,
                message: format!("Failed to get device version from API: {}", e),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Step 2: Get current firmware version ID
    let current_firmware_id = match get_current_firmware_version_id_web(
        device_version_api.id,
        &device_info.version,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            let result = FirmwareUpdateInfo {
                update_available: false,
                current_version: Some(device_info.version.clone()),
                target_version: None,
                message: format!("Failed to get current firmware version: {}", e),
            };
            return Ok(serde_wasm_bindgen::to_value(&result)?);
        }
    };

    // Step 3: Check for latest firmware
    let latest_osu =
        match get_latest_firmware_from_api_web(current_firmware_id, device_version_api.id).await {
            Ok(osu) => osu,
            Err(e) => {
                let result = FirmwareUpdateInfo {
                    update_available: false,
                    current_version: Some(device_info.version.clone()),
                    target_version: None,
                    message: format!("Failed to check for updates: {}", e),
                };
                return Ok(serde_wasm_bindgen::to_value(&result)?);
            }
        };

    // Step 4: If OSU is available, get the final firmware version info
    match latest_osu {
        Some(osu) if osu.next_se_firmware_final_version > 0 => {
            match get_final_firmware_by_id_web(osu.next_se_firmware_final_version).await {
                Ok(final_fw) => {
                    // Use version if available, otherwise fall back to name
                    let target_version = if !final_fw.version.is_empty() {
                        final_fw.version.clone()
                    } else if !final_fw.name.is_empty() {
                        final_fw.name.clone()
                    } else {
                        format!("id:{}", final_fw.id)
                    };

                    let result = FirmwareUpdateInfo {
                        update_available: true,
                        current_version: Some(device_info.version.clone()),
                        target_version: Some(target_version.clone()),
                        message: format!(
                            "Update available: {} -> {}",
                            device_info.version, target_version
                        ),
                    };
                    Ok(serde_wasm_bindgen::to_value(&result)?)
                }
                Err(e) => {
                    let result = FirmwareUpdateInfo {
                        update_available: true,
                        current_version: Some(device_info.version.clone()),
                        target_version: None,
                        message: format!("Update available but failed to get details: {}", e),
                    };
                    Ok(serde_wasm_bindgen::to_value(&result)?)
                }
            }
        }
        _ => {
            let result = FirmwareUpdateInfo {
                update_available: false,
                current_version: Some(device_info.version.clone()),
                target_version: None,
                message: format!("Firmware is up to date ({})", device_info.version),
            };
            Ok(serde_wasm_bindgen::to_value(&result)?)
        }
    }
}
