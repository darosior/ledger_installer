//! Interactive menu module for the Ledger Manager CLI.
//!
//! Provides a text-based user interface using dialoguer for navigating
//! all Ledger device operations without requiring environment variables.

use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Select};

use crate::operations;

/// Main menu choices
enum MainMenuChoice {
    GetInfo,
    GenuineCheck,
    BitcoinMainnet,
    BitcoinTestnet,
    UpdateFirmware,
    Exit,
}

/// App submenu choices (same for mainnet and testnet)
enum AppMenuChoice {
    Install,
    Update,
    Open,
    Back,
}

/// Print the application header/banner.
fn print_header(term: &Term) {
    let _ = term.clear_screen();
    println!(
        "{}",
        style("═══════════════════════════════════════").cyan()
    );
    println!(
        "{}",
        style("  Bacca - Ledger Bitcoin Manager").cyan().bold()
    );
    println!(
        "{}",
        style("═══════════════════════════════════════").cyan()
    );
    println!();
}

/// Display the main menu and get user selection.
fn show_main_menu() -> MainMenuChoice {
    let selections = &[
        "Get Device Info",
        "Genuine Check",
        "Bitcoin (Mainnet)",
        "Bitcoin Test (Testnet)",
        "Update Firmware",
        "Exit",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select an option")
        .items(selections)
        .default(0)
        .interact()
        .unwrap_or(5); // Default to Exit on error

    match selection {
        0 => MainMenuChoice::GetInfo,
        1 => MainMenuChoice::GenuineCheck,
        2 => MainMenuChoice::BitcoinMainnet,
        3 => MainMenuChoice::BitcoinTestnet,
        4 => MainMenuChoice::UpdateFirmware,
        _ => MainMenuChoice::Exit,
    }
}

/// Display the app submenu (install/update/open) and get user selection.
fn show_app_menu(is_testnet: bool) -> AppMenuChoice {
    let network = if is_testnet { "Testnet" } else { "Mainnet" };
    println!(
        "{}",
        style(format!("── Bitcoin {network} ──")).yellow().bold()
    );
    println!();

    let selections = &[
        "Install App",
        "Update App",
        "Open App",
        "← Back to Main Menu",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select an option")
        .items(selections)
        .default(0)
        .interact()
        .unwrap_or(3); // Default to Back on error

    match selection {
        0 => AppMenuChoice::Install,
        1 => AppMenuChoice::Update,
        2 => AppMenuChoice::Open,
        _ => AppMenuChoice::Back,
    }
}

/// Execute an operation and display the result, then wait for user input.
fn execute_and_wait<F>(term: &Term, operation: F)
where
    F: FnOnce() -> Result<String, String>,
{
    println!();
    match operation() {
        Ok(msg) => println!("{}", style(msg).green()),
        Err(msg) => println!("{}", style(format!("Error: {msg}")).red()),
    }
    println!();
    println!("{}", style("Press Enter to continue...").dim());
    let _ = term.read_line();
}

/// Run the Bitcoin app submenu loop.
fn run_app_submenu(term: &Term, is_testnet: bool) {
    loop {
        print_header(term);

        match show_app_menu(is_testnet) {
            AppMenuChoice::Install => {
                println!();
                println!(
                    "{}",
                    style("You may need to confirm on your device:").yellow()
                );
                println!("  1) Allow listing installed apps");
                println!("  2) Allow the Ledger manager to install the app");
                println!();

                execute_and_wait(term, || {
                    let ledger_api = operations::connect_ledger()?;
                    operations::install_app(&ledger_api, is_testnet)
                });
            }
            AppMenuChoice::Update => {
                println!();
                println!(
                    "{}",
                    style("You may need to confirm on your device:").yellow()
                );
                println!("  1) Allow listing installed apps");
                println!("  2) Allow the Ledger manager to update the app");
                println!();

                execute_and_wait(term, || {
                    let ledger_api = operations::connect_ledger()?;
                    operations::update_app(&ledger_api, is_testnet)
                });
            }
            AppMenuChoice::Open => {
                execute_and_wait(term, || {
                    let ledger_api = operations::connect_ledger()?;
                    operations::open_app(&ledger_api, is_testnet)
                });
            }
            AppMenuChoice::Back => break,
        }
    }
}

/// Run the interactive menu loop.
pub fn run_interactive_mode() {
    let term = Term::stdout();

    loop {
        print_header(&term);

        match show_main_menu() {
            MainMenuChoice::GetInfo => {
                println!();
                println!(
                    "{}",
                    style("You may need to confirm on your device to list installed apps.")
                        .yellow()
                );

                execute_and_wait(&term, || {
                    let ledger_api = operations::connect_ledger()?;
                    operations::print_ledger_info(&ledger_api)
                });
            }
            MainMenuChoice::GenuineCheck => {
                println!();
                println!(
                    "{}",
                    style("Querying Ledger's remote HSM. You may need to confirm on your device.")
                        .yellow()
                );

                execute_and_wait(&term, || {
                    let ledger_api = operations::connect_ledger()?;
                    operations::perform_genuine_check(&ledger_api)
                });
            }
            MainMenuChoice::BitcoinMainnet => {
                run_app_submenu(&term, false);
            }
            MainMenuChoice::BitcoinTestnet => {
                run_app_submenu(&term, true);
            }
            MainMenuChoice::UpdateFirmware => {
                println!();
                println!(
                    "{}",
                    style("WARNING: Do NOT disconnect your device during the update!")
                        .red()
                        .bold()
                );
                println!(
                    "{}",
                    style("You may need to confirm operations on your device.").yellow()
                );
                println!();

                execute_and_wait(&term, operations::perform_firmware_update);
            }
            MainMenuChoice::Exit => {
                println!();
                println!("{}", style("Goodbye!").cyan());
                break;
            }
        }
    }
}
