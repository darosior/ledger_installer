use std::{env, process};

use form_urlencoded::Serializer as UrlSerializer;
use ledger_manager::{
    bitcoin_app,
    ledger_transport_hidapi::{hidapi::HidApi, TransportNativeHID},
    list_installed_apps, open_bitcoin_app, query_via_websocket, DeviceInfo, FirmwareInfo,
    BASE_SOCKET_URL,
};

// Print on stderr and exit with 1.
macro_rules! error {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        process::exit(1);
    }};
}

#[derive(Debug, Clone, Copy)]
enum Command {
    GetInfo,
    GenuineCheck,
    InstallMainApp,
    UpdateMainApp,
    OpenMainApp,
    InstallTestApp,
    UpdateTestApp,
    OpenTestApp,
    UpdateFirmware,
}

impl Command {
    /// Read command from environment variables.
    pub fn get() -> Option<Self> {
        let is_testnet = env::var("LEDGER_TESTNET").is_ok();
        let cmd_str = env::var("LEDGER_COMMAND").ok()?;

        if cmd_str == "getinfo" {
            Some(Self::GetInfo)
        } else if cmd_str == "genuinecheck" {
            Some(Self::GenuineCheck)
        } else if cmd_str == "installapp" {
            Some(if is_testnet {
                Self::InstallTestApp
            } else {
                Self::InstallMainApp
            })
        } else if cmd_str == "updateapp" {
            Some(if is_testnet {
                Self::UpdateTestApp
            } else {
                Self::UpdateMainApp
            })
        } else if cmd_str == "openapp" {
            Some(if is_testnet {
                Self::OpenTestApp
            } else {
                Self::OpenMainApp
            })
        } else if cmd_str == "updatefirm" {
            Some(Self::UpdateFirmware)
        } else {
            None
        }
    }
}

fn ledger_api() -> TransportNativeHID {
    let hid_api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => error!("Error initializing HDI api: {}.", e),
    };
    match TransportNativeHID::new(&hid_api) {
        Ok(a) => a,
        Err(e) => error!("Error connecting to Ledger device: {}.", e),
    }
}

fn device_info(ledger_api: &TransportNativeHID) -> DeviceInfo {
    match DeviceInfo::new(ledger_api) {
        Ok(i) => i,
        Err(e) => error!("Error fetching device info: {}. Is the Ledger unlocked?", e),
    }
}

fn print_ledger_info(ledger_api: &TransportNativeHID) {
    let device_info = device_info(ledger_api);
    println!("Information about the device: {:#?}", device_info);

    println!("Querying installed applications from your Ledger. You might have to confirm on your device.");
    let apps = match list_installed_apps(ledger_api) {
        Ok(a) => a,
        Err(e) => error!("Error listing installed applications: {}.", e),
    };
    println!("Installed applications:");
    for app in apps {
        println!("  - {:?}", app);
    }
}

fn genuine_check(ledger_api: &TransportNativeHID) {
    let device_info = device_info(ledger_api);
    let firmware_info = FirmwareInfo::from_device(&device_info);

    println!("Querying Ledger's remote HSM to perform the genuine check. You might have to confirm the operation on your device.");
    let genuine_ws_url = UrlSerializer::new(format!("{}/genuine?", BASE_SOCKET_URL))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("perso", &firmware_info.perso)
        .finish();
    if let Err(e) = query_via_websocket(ledger_api, &genuine_ws_url) {
        error!("Error when performing genuine check: {}.", e);
    }
    println!("Success. Your Ledger is genuine.");
}

// Install the Bitcoin app on the device.
fn install_app(ledger_api: &TransportNativeHID, is_testnet: bool) {
    // First of all make sure it's not already installed.
    println!("Querying installed applications from your Ledger. You might have to confirm on your device.");
    let lowercase_app_name = if is_testnet {
        "bitcoin test"
    } else {
        "bitcoin"
    };
    let apps = match list_installed_apps(ledger_api) {
        Ok(a) => a,
        Err(e) => error!("Error listing installed applications: {}.", e),
    };
    if apps
        .iter()
        .any(|app| app.name.to_lowercase() == lowercase_app_name)
    {
        error!("Bitcoin app already installed. Use the update command to update it.");
    }

    let device_info = device_info(ledger_api);
    let bitcoin_app = match bitcoin_app(&device_info, is_testnet) {
        Ok(Some(a)) => a,
        Ok(None) => error!("Could not get info about Bitcoin app.",),
        Err(e) => error!("Error querying info about Bitcoin app: {}.", e),
    };

    // Now install the app by connecting through their websocket thing to their HSM. Make sure to
    // properly escape the parameters in the request's parameter.
    let install_ws_url = UrlSerializer::new(format!("{}/install?", BASE_SOCKET_URL))
        .append_pair("targetId", &device_info.target_id.to_string())
        .append_pair("perso", &bitcoin_app.perso)
        .append_pair("deleteKey", &bitcoin_app.delete_key)
        .append_pair("firmware", &bitcoin_app.firmware)
        .append_pair("firmwareKey", &bitcoin_app.firmware_key)
        .append_pair("hash", &bitcoin_app.hash)
        .finish();
    println!("Querying Ledger's remote HSM to install the app. You might have to confirm the operation on your device.");
    if let Err(e) = query_via_websocket(ledger_api, &install_ws_url) {
        error!(
            "Got an error when installing Bitcoin app from Ledger's remote HSM: {}.",
            e
        );
    }
    println!("Successfully installed the app.");
}

fn open_app(ledger_api: &TransportNativeHID, is_testnet: bool) {
    if let Err(e) = open_bitcoin_app(ledger_api, is_testnet) {
        error!("Error opening Bitcoin app: {}", e);
    }
}

fn main() {
    let command = if let Some(cmd) = Command::get() {
        cmd
    } else {
        error!("Invalid or no command specified. The command must be passed through the LEDGER_COMMAND env var. Set LEDGER_TESTNET to use the Bitcoin testnet app instead where applicable.");
    };

    let ledger_api = ledger_api();
    match command {
        Command::GetInfo => {
            print_ledger_info(&ledger_api);
        }
        Command::GenuineCheck => {
            genuine_check(&ledger_api);
        }
        Command::InstallMainApp => {
            install_app(&ledger_api, false);
        }
        Command::InstallTestApp => {
            install_app(&ledger_api, true);
        }
        Command::OpenMainApp => {
            open_app(&ledger_api, false);
        }
        Command::OpenTestApp => {
            open_app(&ledger_api, true);
        }
        Command::UpdateMainApp | Command::UpdateTestApp | Command::UpdateFirmware => {
            unimplemented!()
        }
    }
}
