<div align="center">

*Brought to you by*

  <a href="https://wizardsardine.com" target="_blank">
    <img src="ws_logo.png" width="400px" />
  </a>

</div>

# Bacca

Your Ledger companion.

**WARNING: this is alpha software. Only use for testing.**

A minimalistic software to install and upgrade the Bitcoin application on Ledger Nano S, S plus and
X.

![](./bacca_software_screenshot.png)

## Why?

Ledger makes great hardware. However their software is lacking.

The Ledger Nano S plus and X are secure, user-friendly and up to date with the latest Bitcoin
technologies. They are securely accessible to beginners, while letting their users benefit from
advancements and newer standards.

Ledger Live is a cluttered software to manage your device where development resources are allocated
toward scammy altcoins instead of making a decent Bitcoin wallet. The trajectory taken by Ledger
Live has become increasingly worrying to me and other users of Liana: will my beneficiaries at all
be able to navigate through the nudges toward Ponzi schemes and go through the unnecessary
complicated procedure of setting up their device to be used with a Bitcoin wallet?

This software offers a simple, straight-to-the-point, alternative.

The state of this project is nowhere near the point where it can stably replace Ledger Live for
non tech-savvy bitcoiners, yet. That said we hope to start pulling some of the functionalities into
[Liana](https://github.com/wizardsardine/liana).

## Features

- **Genuine Check**: Verify your Ledger device is authentic using Ledger's HSM
- **Install Bitcoin App**: Install the Bitcoin application (mainnet or testnet)
- **Update Bitcoin App**: Update to the latest Bitcoin application version
- **Firmware Updates**: Check for and install firmware updates (GUI/CLI)
- **Device Info**: View device model, firmware version, and installed apps
- **Open Bitcoin App**: Launch the Bitcoin app directly from the interface

## Supported Devices

- Ledger Nano S
- Ledger Nano S Plus
- Ledger Nano X

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) (1.70 or later recommended)
- For desktop builds: USB access to Ledger device
- For web builds: [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

### Building from Source

Clone the repository:
```bash
git clone https://github.com/AntoinePorins/bacca.git
cd bacca
```

Build all desktop components:
```bash
cargo build --release
```

## Usage

**This is a PoC. Use at your own risk.**

This software can be used in three ways:
1. Through a **Graphical User Interface** (GUI)
2. Through a **Command Line Interface** (CLI)
3. Through a **Web Interface** (Browser with WebHID support)
4. As a **Rust library** for other projects to integrate

### GUI

The recommended way to use this software is through the GUI. Simply connect your Ledger Nano S, S
plus or X to the USB port and run:
```bash
cargo run -p ledger_manager_gui --release
```

We plan on releasing binaries in the future.

### CLI

Another way of using this is the CLI, which directly hooks up into the functionalities offered by
the Rust crate. The CLI will talk to a Ledger device connected by USB.

#### Interactive Mode (Recommended)

Simply run the CLI without any arguments to enter interactive mode:
```bash
cargo run -p ledger_manager_cli --release
```

This presents a menu where you can navigate all available options for both mainnet and testnet
Bitcoin apps without needing to restart the application.

#### Legacy Mode (Environment Variables)

For scripting or automation, commands can be passed via environment variables. Set `LEDGER_COMMAND`
to specify the action. Set `LEDGER_TESTNET` to any value to target the Bitcoin Test app instead.

Available commands:
- `getinfo`: get information (such as the list of installed apps) for your device
- `genuinecheck`: check your Ledger device is genuine
- `installapp`: install the Bitcoin app on your device
- `updateapp`: update the Bitcoin app on your device
- `openapp`: open the Bitcoin app on your device
- `updatefirm`: update the device firmware
- `repairdevice`: repair a device stuck in bootloader mode

### Examples

#### Using Interactive Mode

```
cargo run -p ledger_manager_cli
```
```
═══════════════════════════════════════
  Bacca - Ledger Bitcoin Manager
═══════════════════════════════════════

? Select an option ›
❯ Get Device Info
  Genuine Check
  Bitcoin (Mainnet)
  Bitcoin Test (Testnet)
  Update Firmware
  Repair Device (Bootloader)
  Exit
```

#### Checking your Ledger is genuine (Legacy Mode)

```
LEDGER_COMMAND=genuinecheck cargo run -p ledger_manager_cli
```
```
Querying Ledger's remote HSM to perform the genuine check. You might have to confirm the operation on your device.
Success. Your Ledger is genuine.
```

#### Installing the Bitcoin Test app on your Ledger (Legacy Mode)

```
LEDGER_TESTNET=1 LEDGER_COMMAND=installapp cargo run -p ledger_manager_cli
```
```
Querying installed applications from your Ledger. You might have to confirm on your device.
Querying Ledger's remote HSM to install the app. You might have to confirm the operation on your device.
Successfully installed the app.
```

### Web Interface

Bacca can run in modern browsers that support WebHID (Chrome, Edge, Opera).

#### Building for Web

```bash
# Install wasm-pack if not already installed
cargo install wasm-pack

# Build the WASM package
RUSTFLAGS="--cfg=web_sys_unstable_apis" wasm-pack build --target web web/
```

#### Running Locally

```bash
# Serve the web directory (using Python)
cd web
python3 -m http.server 8080

# Or using Node.js
npx serve web/
```

Then open http://localhost:8080 in a WebHID-compatible browser.

**Note**: WebHID requires HTTPS in production, but `localhost` is treated as a secure context for development.

## Architecture

Bacca is organized as a Rust workspace with four crates:

```
bacca/
├── ledger_manager/     # Core library
│   └── src/
│       ├── lib.rs           # Main library: device communication, app management
│       ├── firmware.rs      # Firmware update functionality
│       ├── constants.rs     # API constants and APDU commands
│       ├── error.rs         # Error types (DeviceError, InstallErr, UpdateErr)
│       └── transport/       # Hardware communication layer
│           ├── mod.rs       # Transport trait definition
│           ├── native.rs    # USB HID transport (desktop)
│           ├── webhid.rs    # WebHID transport (browser)
│           └── protocol.rs  # Ledger HID framing protocol
├── cli/                # Command-line interface
│   └── src/
│       ├── main.rs          # Entry point
│       ├── menu.rs          # Interactive menu system
│       └── operations.rs    # CLI operations implementation
├── gui/                # Graphical interface (iced framework)
│   └── src/
│       ├── main.rs          # Entry point, tokio runtime setup
│       ├── gui.rs           # Iced application and UI
│       ├── ledger_service.rs# Background service for device operations
│       ├── service.rs       # Service trait and macros
│       └── theme/           # Custom UI theme
└── web/                # Web interface (WASM)
    ├── src/
    │   ├── lib.rs           # wasm-bindgen exports
    │   ├── types.rs         # API response types
    │   ├── apdu.rs          # APDU command definitions
    │   └── parsing.rs       # Device response parsing
    ├── index.html           # Web UI
    ├── app.js               # JavaScript application logic
    └── style.css            # Styling
```

### Core Library (`ledger_manager`)

The core library provides platform-agnostic functionality with feature flags:

- **`desktop`** (default): Enables USB HID transport via `ledger-transport-hidapi`, HTTP via `minreq`, WebSocket via `tungstenite`
- **`web`**: Enables WebHID transport via `web-sys`, HTTP via `gloo-net`

Key modules:
- **Device Communication**: APDU command exchange over USB HID or WebHID
- **App Management**: Install, update, and manage Bitcoin applications
- **Genuine Check**: Verify device authenticity via Ledger's HSM
- **Firmware Updates**: Full firmware update flow including MCU/bootloader

### Communication Flow

```
┌─────────────┐      ┌──────────────────┐      ┌─────────────────┐
│   GUI/CLI   │ ───► │  ledger_manager  │ ───► │  Ledger Device  │
│   or Web    │      │    (library)     │      │   (USB/WebHID)  │
└─────────────┘      └──────────────────┘      └─────────────────┘
                              │
                              ▼
                     ┌──────────────────┐
                     │   Ledger APIs    │
                     │  - Manager API   │
                     │  - HSM WebSocket │
                     └──────────────────┘
```

The library communicates with:
1. **Ledger Device**: Via APDU commands over USB HID (desktop) or WebHID (browser)
2. **Ledger Manager API**: REST API for app metadata, device info, firmware versions
3. **Ledger HSM**: WebSocket connection for genuine checks and secure app installation

## Development

### Code Style

The project uses strict Rust linting:
- `unsafe_code` is denied
- `clippy::pedantic` warnings enabled
- `unwrap_used` and `expect_used` warnings enabled

Run checks before submitting:
```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --workspace --exclude ledger_manager_web --all-targets

# Run tests
cargo test --workspace --exclude ledger_manager_web

# Check web build
RUSTFLAGS="--cfg=web_sys_unstable_apis" cargo clippy -p ledger_manager_web
```

### Testing with a Device

Most functionality requires a physical Ledger device. Connect your device and ensure it's unlocked on the dashboard before running commands.

## Future

We are looking into people to help test this and confirm it works in as many scenarii as possible.

~~We are probably going to have to introduce an `upgradefirmware` command.~~ ✓ Firmware updates are now supported!

Contributions welcome! If you are interested, get in touch on the [Liana
Discord](https://discord.gg/QJUp67zSN4).

NOTE: i am not interested in supporting altcoins. If you want to add support for one, feel free to
fork the project.

## License

MIT License. See [LICENCE](LICENCE) for details.
