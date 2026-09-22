mod menu;
mod operations;

use std::{env, process};

/// Commands available via environment variables (legacy mode).
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

/// Run in legacy mode using environment variable commands.
fn run_legacy_mode(command: Command) {
    let ledger_api = match operations::connect_ledger() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let result = match command {
        Command::GetInfo => operations::print_ledger_info(&ledger_api),
        Command::GenuineCheck => {
            println!("Querying Ledger's remote HSM to perform the genuine check. You might have to confirm the operation on your device.");
            operations::perform_genuine_check(&ledger_api)
        }
        Command::InstallMainApp => {
            println!("You may have to allow on your device 1) listing installed apps 2) the Ledger manager to install the app.");
            operations::install_app(&ledger_api, false)
        }
        Command::InstallTestApp => {
            println!("You may have to allow on your device 1) listing installed apps 2) the Ledger manager to install the app.");
            operations::install_app(&ledger_api, true)
        }
        Command::UpdateMainApp => {
            println!("You may have to allow on your device 1) listing installed apps 2) the Ledger manager to update the app.");
            operations::update_app(&ledger_api, false)
        }
        Command::UpdateTestApp => {
            println!("You may have to allow on your device 1) listing installed apps 2) the Ledger manager to update the app.");
            operations::update_app(&ledger_api, true)
        }
        Command::OpenMainApp => operations::open_app(&ledger_api, false),
        Command::OpenTestApp => operations::open_app(&ledger_api, true),
        Command::UpdateFirmware => {
            eprintln!("Firmware update is not yet implemented.");
            process::exit(1);
        }
    };

    match result {
        Ok(msg) => println!("{msg}"),
        Err(msg) => {
            eprintln!("{msg}");
            process::exit(1);
        }
    }
}

fn main() {
    // Check if LEDGER_COMMAND is set for backward compatibility (legacy mode)
    if let Some(cmd) = Command::get() {
        run_legacy_mode(cmd);
    } else if env::var("LEDGER_COMMAND").is_ok() {
        // LEDGER_COMMAND was set but invalid
        eprintln!("Invalid command specified. Valid commands: getinfo, genuinecheck, installapp, updateapp, openapp, updatefirm");
        eprintln!("Set LEDGER_TESTNET to use the Bitcoin testnet app where applicable.");
        process::exit(1);
    } else {
        // No environment variable set - run interactive mode
        menu::run_interactive_mode();
    }
}
