//! Native USB HID transport for desktop platforms.
//!
//! This module wraps the `ledger-transport-hidapi` crate to provide
//! USB HID communication with Ledger devices on desktop platforms.

use super::{LedgerTransport, TransportError, TransportResult};
use ledger_apdu::{APDUAnswer, APDUCommand};
use ledger_transport_hidapi::{hidapi::HidApi, TransportNativeHID};

/// Native USB HID transport for Ledger devices.
///
/// This is a thin wrapper around `TransportNativeHID` that implements
/// the `LedgerTransport` trait for platform-agnostic device communication.
pub struct NativeTransport {
    inner: TransportNativeHID,
}

impl NativeTransport {
    /// Create a new native transport by connecting to a Ledger device.
    ///
    /// # Returns
    /// A new `NativeTransport` if a device is found, or an error otherwise.
    pub fn new() -> TransportResult<Self> {
        let hid_api =
            HidApi::new().map_err(|e| TransportError::OpenFailed(format!("HID API: {e}")))?;

        let transport = TransportNativeHID::new(&hid_api)
            .map_err(|e| TransportError::OpenFailed(format!("Ledger device: {e}")))?;

        Ok(Self { inner: transport })
    }

    /// Create a native transport from an existing HID API instance.
    ///
    /// This is useful when you want to reuse an existing HID API context.
    pub fn with_hid_api(hid_api: &HidApi) -> TransportResult<Self> {
        let transport = TransportNativeHID::new(hid_api)
            .map_err(|e| TransportError::OpenFailed(format!("Ledger device: {e}")))?;

        Ok(Self { inner: transport })
    }

    /// Get a reference to the underlying transport.
    ///
    /// This is useful for operations that need direct access to the native transport,
    /// such as the existing library functions that expect `&TransportNativeHID`.
    pub fn inner(&self) -> &TransportNativeHID {
        &self.inner
    }
}

impl LedgerTransport for NativeTransport {
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> TransportResult<APDUAnswer<Vec<u8>>> {
        self.inner
            .exchange(command)
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }

    fn is_connected(&self) -> bool {
        // The native transport doesn't have a direct way to check connection,
        // so we assume it's connected if we have a transport instance.
        // Actual disconnection would be detected on the next exchange.
        true
    }
}

/// Convenience function to create a new native transport.
pub fn connect() -> TransportResult<NativeTransport> {
    NativeTransport::new()
}
