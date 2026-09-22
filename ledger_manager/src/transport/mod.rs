//! Transport abstraction layer for Ledger device communication.
//!
//! This module provides a platform-agnostic interface for communicating with
//! Ledger hardware wallets. The actual transport implementation varies by platform:
//! - Desktop: USB HID via `ledger-transport-hidapi`
//! - Web: `WebHID` API via `web-sys`

pub mod protocol;

#[cfg(feature = "desktop")]
pub mod native;

#[cfg(all(feature = "web", web_sys_unstable_apis))]
pub mod webhid;

pub use protocol::*;

use ledger_apdu::{APDUAnswer, APDUCommand};
use thiserror::Error;

/// Errors that can occur during transport operations
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Device not found or not connected")]
    DeviceNotFound,

    #[error("Device is locked - please unlock your Ledger")]
    DeviceLocked,

    #[error("Failed to open device: {0}")]
    OpenFailed(String),

    #[error("Device disconnected")]
    Disconnected,

    #[error("Failed to send command: {0}")]
    SendFailed(String),

    #[error("Failed to receive response: {0}")]
    ReceiveFailed(String),

    #[error("APDU protocol error: {0}")]
    ProtocolError(#[from] ApduError),

    #[error("User cancelled operation")]
    UserCancelled,

    #[error("WebHID not supported in this browser")]
    NotSupported,

    #[error("Transport error: {0}")]
    Other(String),
}

/// Result type for transport operations
pub type TransportResult<T> = Result<T, TransportError>;

/// Platform-agnostic transport trait for Ledger device communication.
///
/// This trait abstracts over the underlying transport mechanism (USB HID for desktop,
/// `WebHID` for web) to provide a unified interface for APDU command exchange.
pub trait LedgerTransport {
    /// Exchange an APDU command with the device and receive a response.
    ///
    /// # Arguments
    /// * `command` - The APDU command to send
    ///
    /// # Returns
    /// The APDU response from the device
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> TransportResult<APDUAnswer<Vec<u8>>>;

    /// Check if the transport is still connected to a device.
    fn is_connected(&self) -> bool;
}

// Re-export the native transport when on desktop
#[cfg(feature = "desktop")]
pub use native::NativeTransport;

// Re-export the WebHID transport when on web
#[cfg(all(feature = "web", web_sys_unstable_apis))]
pub use webhid::{is_webhid_supported, WebHidTransport};
