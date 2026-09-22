//! Error types for Ledger device communication and app management.
//!
//! This module provides unified error types for device operations, app installation,
//! and app updates.

use thiserror::Error;

/// Status codes returned by Ledger device APDU responses.
///
/// Reference: <https://github.com/LedgerHQ/ledger-live/blob/4d1d7bb3462fd0c986ed587f0cf426afc96850c8/libs/ledgerjs/packages/errors/src/index.ts#L233>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    /// Device is locked and requires PIN entry.
    LockedDevice = 0x5515,
    /// Operation completed successfully.
    OK = 0x9000,
}

/// Errors that can occur during device communication.
///
/// # Errors
///
/// Returned when communication with a Ledger device fails due to
/// device state, protocol errors, or invalid responses.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeviceError {
    /// Device is locked and requires PIN entry.
    #[error("device is locked")]
    Locked,

    /// Device returned an unexpected status code.
    #[error("device not ready: status code {0:#06x}")]
    NotReady(u16),

    /// Response did not contain enough data.
    #[error("insufficient data in response")]
    InsufficientData,

    /// Response contained invalid UTF-8.
    #[error("invalid UTF-8 in device response")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// Response data format was invalid.
    #[error("invalid data format: {0}")]
    InvalidFormat(String),
}

/// Errors that can occur during Bitcoin app installation.
///
/// # Errors
///
/// Returned when installing the Bitcoin application fails.
#[derive(Debug)]
#[non_exhaustive]
pub enum InstallErr {
    /// The Bitcoin application is already installed on the device.
    AlreadyInstalled,

    /// Could not find Bitcoin app information from the Ledger API.
    AppNotFound,

    /// A general error occurred during installation.
    Any(Box<dyn std::error::Error>),
}

/// Errors that can occur during Bitcoin app update.
///
/// # Errors
///
/// Returned when updating the Bitcoin application fails.
#[derive(Debug)]
#[non_exhaustive]
pub enum UpdateErr {
    /// The Bitcoin application is not installed on the device.
    NotInstalled,

    /// Could not find Bitcoin app information from the Ledger API.
    AppNotFound,

    /// The installed app is already at the latest version.
    AlreadyLatest,

    /// A general error occurred during update.
    Any(Box<dyn std::error::Error>),
}
