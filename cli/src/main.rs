use std::{env, process};

use ledger_manager::{
    genuine_check, install_bitcoin_app,
    ledger_transport_hidapi::{hidapi::HidApi, TransportNativeHID},
    list_installed_apps, open_bitcoin_app, DeviceInfo, InstallErr,
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

fn perform_genuine_check(ledger_api: &TransportNativeHID) {
    println!("Querying Ledger's remote HSM to perform the genuine check. You might have to confirm the operation on your device.");
    if let Err(e) = genuine_check(ledger_api) {
        error!("Error when performing genuine check: {}", e);
    }
    println!("Success. Your Ledger is genuine.");
}

// Install the Bitcoin app on the device.
fn install_app(ledger_api: &TransportNativeHID, is_testnet: bool) {
    println!("You may have to allow on your device 1) listing installed apps 2) the Ledger manager to install the app.");
    match install_bitcoin_app(ledger_api, is_testnet) {
        Ok(()) => println!("Successfully installed the app."),
        Err(InstallErr::AlreadyInstalled) => {
            error!("Bitcoin app already installed. Use the update command to update it.")
        }
        Err(InstallErr::AppNotFound) => error!("Could not get info about Bitcoin app."),
        Err(InstallErr::Any(e)) => error!("Error installing Bitcoin app: {}.", e),
    }
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
            perform_genuine_check(&ledger_api);
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
