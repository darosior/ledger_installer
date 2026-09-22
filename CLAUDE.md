# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Bacca is a minimalistic alternative to Ledger Live for installing and upgrading the Bitcoin application on Ledger hardware wallets (Nano S, S+, X). It's Bitcoin-only by design—no altcoin support.

**Status**: Alpha software - for testing only

## Build and Development Commands

```bash
# Build entire workspace
cargo build
cargo build --release

# Run GUI (primary interface)
cargo run -p ledger_manager_gui

# Run CLI with specific commands
LEDGER_COMMAND=getinfo cargo run -p ledger_manager_cli
LEDGER_COMMAND=genuinecheck cargo run -p ledger_manager_cli
LEDGER_COMMAND=installapp cargo run -p ledger_manager_cli
LEDGER_COMMAND=updateapp cargo run -p ledger_manager_cli
LEDGER_COMMAND=openapp cargo run -p ledger_manager_cli

# Testnet mode
LEDGER_TESTNET=1 LEDGER_COMMAND=installapp cargo run -p ledger_manager_cli

# Linting and formatting
cargo clippy
cargo clippy --fix
cargo fmt
cargo fmt --check

# Tests
cargo test
cargo test -p ledger_manager
```

## Architecture

Rust monorepo workspace with 3 crates:

### `ledger_manager` (Core Library)
- `/ledger_manager/src/lib.rs` - All core business logic
- Device communication via APDU commands over USB HID
- HTTP integration with Ledger's Manager API (`manager.api.live.ledger.com`)
- WebSocket communication with Ledger's HSM for genuine checks and installations
- Key functions: `list_installed_apps()`, `install_bitcoin_app()`, `update_bitcoin_app()`, `genuine_check()`

### `ledger_manager_cli` (Command Line Interface)
- `/cli/src/main.rs` - Thin wrapper around library
- Command selection via `LEDGER_COMMAND` environment variable
- Network selection via `LEDGER_TESTNET` environment variable

### `ledger_manager_gui` (Graphical Interface)
- `/gui/src/main.rs` - Tokio runtime setup and channel creation
- `/gui/src/gui.rs` - Iced framework app with message-based event system
- `/gui/src/ledger_service.rs` - Background async service for device operations
- `/gui/src/service.rs` - Generic service trait with `listener!` macro
- Uses async_channel for bidirectional GUI↔Service communication

## Key Patterns

- **Environment Variables**: Runtime configuration via env vars (no config files)
- **Error Enums**: Specialized `InstallErr` and `UpdateErr` types with detailed variants
- **Async Architecture**: GUI uses tokio + async_channel + iced subscriptions; service runs in background task
- **No Persistence**: All state is in-memory
- **Ledger API**: Reverse-engineered from Ledger Live; may break with upstream changes

## Important Constraints

- **Bitcoin-only**: Do not add altcoin support
- **Hardware Required**: Most functionality requires a physical Ledger device
- **Single Genuine Check**: GUI allows genuine check only once per launch
