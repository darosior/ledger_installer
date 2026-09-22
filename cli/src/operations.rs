//! Operations module - extracted functions for Ledger device operations.
//!
//! All functions return `Result<String, String>` for consistent error handling
//! that works with both interactive menu and legacy CLI modes.

use ledger_manager::{
    firmware::{
        get_latest_firmware_for_device, repair_device_in_bootloader_with_reconnect,
        update_firmware, FirmwareUpdatePhase,
    },
    genuine_check, install_bitcoin_app,
    ledger_transport_hidapi::{hidapi::HidApi, TransportNativeHID},
    list_installed_apps, open_bitcoin_app, update_bitcoin_app, DeviceInfo, InstallErr, UpdateErr,
};

/// Initialize the HID API and connect to a Ledger device.
pub fn connect_ledger() -> Result<TransportNativeHID, String> {
    let hid_api = HidApi::new().map_err(|e| format!("Error initializing HID API: {e}."))?;
    TransportNativeHID::new(&hid_api)
        .map_err(|e| format!("Error connecting to Ledger device: {e}."))
}

/// Create a new transport - used for reconnection after device reboots.
pub fn create_transport() -> Result<TransportNativeHID, Box<dyn std::error::Error>> {
    let hid_api = Box::new(HidApi::new()?);
    let transport = TransportNativeHID::new(&hid_api)?;
    Ok(transport)
}

/// Get device information from a connected Ledger.
pub fn get_device_info(ledger_api: &TransportNativeHID) -> Result<DeviceInfo, String> {
    DeviceInfo::new(ledger_api).map_err(|e| format!("Error fetching device info: {e}"))
}

/// Get and display information about the device and installed applications.
pub fn print_ledger_info(ledger_api: &TransportNativeHID) -> Result<String, String> {
    let device_info = get_device_info(ledger_api)?;
    let mut output = format!("Information about the device: {device_info:#?}\n");

    output.push_str(
        "Querying installed applications from your Ledger. You might have to confirm on your device.\n",
    );

    let apps = list_installed_apps(ledger_api)
        .map_err(|e| format!("Error listing installed applications: {e}."))?;

    output.push_str("Installed applications:\n");
    for app in apps {
        output.push_str(&format!("  - {app:?}\n"));
    }

    Ok(output)
}

/// Perform a genuine check on the Ledger device.
pub fn perform_genuine_check(ledger_api: &TransportNativeHID) -> Result<String, String> {
    genuine_check(ledger_api).map_err(|e| format!("Error when performing genuine check: {e}"))?;
    Ok("Success. Your Ledger is genuine.".to_string())
}

/// Install the Bitcoin app on the device.
pub fn install_app(ledger_api: &TransportNativeHID, is_testnet: bool) -> Result<String, String> {
    let app_name = if is_testnet {
        "Bitcoin Test"
    } else {
        "Bitcoin"
    };

    match install_bitcoin_app(ledger_api, is_testnet) {
        Ok(()) => Ok(format!("Successfully installed the {app_name} app.")),
        Err(InstallErr::AlreadyInstalled) => Err(format!(
            "{app_name} app already installed. Use the update command to update it."
        )),
        Err(InstallErr::AppNotFound) => Err(format!("Could not get info about {app_name} app.")),
        Err(InstallErr::Any(e)) => Err(format!("Error installing {app_name} app: {e}.")),
        Err(_) => Err(format!("Unknown error installing {app_name} app.")),
    }
}

/// Update the Bitcoin app on the device.
pub fn update_app(ledger_api: &TransportNativeHID, is_testnet: bool) -> Result<String, String> {
    let app_name = if is_testnet {
        "Bitcoin Test"
    } else {
        "Bitcoin"
    };

    match update_bitcoin_app(ledger_api, is_testnet) {
        Ok(()) => Ok(format!("Successfully updated the {app_name} app.")),
        Err(UpdateErr::NotInstalled) => Err(format!(
            "{app_name} app isn't installed. Use the install command instead."
        )),
        Err(UpdateErr::AppNotFound) => Err(format!("Could not get info about {app_name} app.")),
        Err(UpdateErr::AlreadyLatest) => {
            Err(format!("{app_name} app is already at the latest version."))
        }
        Err(UpdateErr::Any(e)) => Err(format!("Error updating {app_name} app: {e}.")),
        Err(_) => Err(format!("Unknown error updating {app_name} app.")),
    }
}

/// Open the Bitcoin app on the device.
pub fn open_app(ledger_api: &TransportNativeHID, is_testnet: bool) -> Result<String, String> {
    let app_name = if is_testnet {
        "Bitcoin Test"
    } else {
        "Bitcoin"
    };

    open_bitcoin_app(ledger_api, is_testnet)
        .map_err(|e| format!("Error opening {app_name} app: {e}"))?;
    Ok(format!("{app_name} app opened successfully."))
}

/// Print firmware update phase progress.
fn print_firmware_phase(phase: &FirmwareUpdatePhase) {
    match phase {
        FirmwareUpdatePhase::CheckingForUpdates => {
            println!("Checking for updates...");
        }
        FirmwareUpdatePhase::DownloadingMetadata => {
            println!("Downloading firmware metadata...");
        }
        FirmwareUpdatePhase::InstallingOsu { progress } => {
            print!("\rInstalling OSU: {:>5.1}%", progress * 100.0);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        FirmwareUpdatePhase::WaitingForBootloader => {
            println!("\nWaiting for device to reboot into bootloader...");
        }
        FirmwareUpdatePhase::FlashingMcu {
            iteration,
            progress,
        } => {
            print!(
                "\rFlashing MCU ({}/5): {:>5.1}%",
                iteration,
                progress * 100.0
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        FirmwareUpdatePhase::InstallingFinalFirmware { progress } => {
            print!("\rInstalling final firmware: {:>5.1}%", progress * 100.0);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        FirmwareUpdatePhase::Completed => {
            println!("\nUpdate completed!");
        }
        FirmwareUpdatePhase::Failed { error } => {
            println!("\nUpdate failed: {error}");
        }
    }
}

/// Check for and perform firmware update.
/// Returns ownership of transport back if update not needed, or String result.
pub fn perform_firmware_update() -> Result<String, String> {
    // Connect and get device info
    let ledger_api = connect_ledger()?;
    let device_info = get_device_info(&ledger_api)?;

    // Drop the transport to free the HID device handle
    drop(ledger_api);

    // Check if device is in bootloader mode (repair mode)
    if device_info.is_bootloader {
        println!("Device is in bootloader mode. Attempting repair...");
        println!("  Target ID: {:#x}", device_info.target_id);
        println!("  SE Target ID: {:#x}", device_info.se_target_id);
        println!("  Bootloader Version: {}", device_info.version);
        println!("  MCU Version: {:?}", device_info.mcu_version);
        println!("You may need to confirm operations on your device.");

        match repair_device_in_bootloader_with_reconnect(create_transport, |phase| {
            print_firmware_phase(&phase);
        }) {
            Ok(()) => return Ok("Device repaired successfully!".to_string()),
            Err(e) => return Err(format!("Repair failed: {e}")),
        }
    }

    // Check for firmware updates
    println!("Checking for firmware updates...");
    let update_context = match get_latest_firmware_for_device(&device_info) {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return Ok(format!(
                "Firmware is already up to date (version {}).",
                device_info.version
            ));
        }
        Err(e) => return Err(format!("Error checking for updates: {e}")),
    };

    println!(
        "Update available: {} -> {}",
        device_info.version, update_context.final_firmware.version
    );
    println!("OSU: {}", update_context.osu.name);
    if update_context.should_flash_mcu {
        println!("Note: MCU will also be updated (device will reboot multiple times).");
    }
    println!();
    println!("You may need to confirm operations on your device.");
    println!("DO NOT disconnect your device during the update!");
    println!();

    // Create a fresh transport for the update
    let update_transport =
        create_transport().map_err(|e| format!("Error connecting to Ledger device: {e}"))?;

    // Perform the update
    match update_firmware(&update_transport, &update_context, |phase| {
        print_firmware_phase(&phase);
    }) {
        Ok(()) => Ok("Firmware updated successfully!".to_string()),
        Err(e) => Err(format!("Firmware update failed: {e}")),
    }
}
