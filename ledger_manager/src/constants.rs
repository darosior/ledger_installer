//! Constants used for communicating with Ledger devices and the Ledger Manager API.
//!
//! This module contains APDU commands for device communication and API endpoints
//! for the Ledger Manager service.

// ============================================================================
// APDU Commands (desktop only)
// ============================================================================

#[cfg(feature = "desktop")]
mod apdu {
    use ledger_apdu::APDUCommand;

    /// Command to query device version information.
    ///
    /// Reference: <https://github.com/LedgerHQ/ledger-live/blob/dd1d17fd3ce7ed42558204b2f93707fb9b1599de/libs/device-core/src/commands/use-cases/getVersion.ts#L6>
    pub const GET_VERSION_COMMAND: APDUCommand<&[u8]> = APDUCommand {
        cla: 0xe0,
        ins: 0x01,
        p1: 0x00,
        p2: 0x00,
        data: &[],
    };

    /// Command to list installed applications on the device.
    ///
    /// Reference: <https://github.com/LedgerHQ/ledger-live/blob/99879eb5bada1ecaea7a02d8886e16b44657af6d/libs/ledger-live-common/src/hw/listApps.ts#L5>
    pub const LIST_APPS_COMMAND: APDUCommand<&[u8]> = APDUCommand {
        cla: 0xe0,
        ins: 0xde,
        p1: 0x00,
        p2: 0x00,
        data: &[],
    };

    /// Command to continue listing applications (pagination).
    ///
    /// Reference: <https://github.com/LedgerHQ/ledger-live/blob/99879eb5bada1ecaea7a02d8886e16b44657af6d/libs/ledger-live-common/src/hw/listApps.ts#L47>
    pub const CONTINUE_LIST_APPS_COMMAND: APDUCommand<&[u8]> = APDUCommand {
        cla: 0xe0,
        ins: 0xdf,
        p1: 0x00,
        p2: 0x00,
        data: &[],
    };

    /// Template command for opening an application on the device.
    ///
    /// The actual app name must be appended to the data field.
    ///
    /// Reference: <https://github.com/LedgerHQ/ledger-live/blob/5a0a1aa5dc183116839851b79bceb6704f1de4b9/libs/ledger-live-common/src/hw/openApp.ts#L3>
    pub const OPEN_APP_COMMAND_TEMPLATE: APDUCommand<&[u8]> = APDUCommand {
        cla: 0xe0,
        ins: 0xd8,
        p1: 0x00,
        p2: 0x00,
        data: &[],
    };
}

#[cfg(feature = "desktop")]
pub(crate) use apdu::*;

// ============================================================================
// API Constants
// ============================================================================

/// The Ledger Live API version string used in requests.
///
/// This version is used in the `livecommonversion` query parameter when
/// communicating with Ledger's Manager API. The value was chosen as a
/// working version compatible with current API endpoints.
pub const LIVE_COMMON_VERSION: &str = "34.0.0";

/// The provider channel for downloading app binaries.
///
/// Provider 1 is the default channel. Provider 4 is "shitcoins" (altcoins).
/// Other values are undocumented.
///
/// Reference: <https://github.com/LedgerHQ/ledger-live/blob/4d1d7bb3462fd0c986ed587f0cf426afc96850c8/libs/device-core/src/managerApi/use-cases/getProviderIdUseCase.ts#L3-L9>
pub const PROVIDER: u32 = 1;

/// Base URL for Ledger Manager API v1 endpoints.
pub const BASE_API_V1_URL: &str = "https://manager.api.live.ledger.com/api";

/// Base URL for Ledger Manager API v2 endpoints.
pub const BASE_API_V2_URL: &str = "https://manager.api.live.ledger.com/api/v2";

/// WebSocket URL for HSM communication (genuine check, app installation).
pub const BASE_SOCKET_URL: &str = "wss://scriptrunner.api.live.ledger.com/update";
