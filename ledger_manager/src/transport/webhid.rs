//! WebHID transport for Ledger devices in WASM/web environments.
//!
//! This module implements the Ledger HID transport protocol using the WebHID API
//! exposed through web-sys. It enables browser-based communication with Ledger devices.
//!
//! Reference: https://developer.mozilla.org/en-US/docs/Web/API/WebHID_API

#![cfg(web_sys_unstable_apis)]

use super::protocol::{
    frame_apdu, parse_response, ApduError, DEFAULT_CHANNEL_ID, HID_PACKET_SIZE, LEDGER_VENDOR_ID,
};
use super::{LedgerTransport, TransportError, TransportResult};
use js_sys::{Array, Promise, Reflect, Uint8Array};
use ledger_apdu::{APDUAnswer, APDUCommand};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Hid, HidDevice, HidDeviceFilter, HidDeviceRequestOptions, HidInputReportEvent};

/// WebHID transport for Ledger devices.
///
/// Provides low-level HID communication with Ledger hardware wallets
/// using the browser's WebHID API.
pub struct WebHidTransport {
    device: HidDevice,
    channel_id: u16,
    /// Buffer for collecting response packets - shared between transport and event handler
    response_buffer: Rc<RefCell<Vec<Vec<u8>>>>,
    /// Closure for handling input reports (must be kept alive)
    _input_closure: Closure<dyn Fn(HidInputReportEvent)>,
}

impl WebHidTransport {
    /// Request access to a Ledger device via the WebHID device picker.
    ///
    /// This must be called in response to a user gesture (e.g., button click).
    /// The browser will show a device picker dialog for the user to select a device.
    pub async fn request_device() -> TransportResult<Self> {
        let hid = get_hid()?;

        // Create filter for Ledger devices
        let mut filter = HidDeviceFilter::new();
        filter.vendor_id(LEDGER_VENDOR_ID as u32);

        let filters = Array::new();
        filters.push(&filter);

        let options = HidDeviceRequestOptions::new(&filters);

        // Request device access - this shows the browser's device picker
        let promise: Promise = hid.request_device(&options);
        let result = JsFuture::from(promise)
            .await
            .map_err(|e| TransportError::OpenFailed(js_error_to_string(e)))?;

        // Result is an array of devices
        let devices: Array = result
            .dyn_into()
            .map_err(|_| TransportError::UserCancelled)?;

        if devices.length() == 0 {
            return Err(TransportError::UserCancelled);
        }

        let device: HidDevice = devices
            .get(0)
            .dyn_into()
            .map_err(|_| TransportError::UserCancelled)?;

        Self::from_device(device).await
    }

    /// Try to connect to an already-authorized Ledger device.
    ///
    /// This doesn't require a user gesture but only works if the user has
    /// previously granted permission to the device.
    pub async fn connect_authorized() -> TransportResult<Self> {
        let hid = get_hid()?;

        // Get list of authorized devices
        let promise = hid.get_devices();
        let result = JsFuture::from(promise)
            .await
            .map_err(|e| TransportError::OpenFailed(js_error_to_string(e)))?;

        let devices: Array = result
            .dyn_into()
            .map_err(|_| TransportError::DeviceNotFound)?;

        // Find a Ledger device
        for i in 0..devices.length() {
            let device: HidDevice = match devices.get(i).dyn_into() {
                Ok(d) => d,
                Err(_) => continue,
            };

            if device.vendor_id() == LEDGER_VENDOR_ID {
                return Self::from_device(device).await;
            }
        }

        Err(TransportError::DeviceNotFound)
    }

    /// Create a transport from an already-obtained HidDevice.
    async fn from_device(device: HidDevice) -> TransportResult<Self> {
        // Open the device if not already open
        if !device.opened() {
            let promise = device.open();
            JsFuture::from(promise)
                .await
                .map_err(|e| TransportError::OpenFailed(js_error_to_string(e)))?;
        }

        let response_buffer: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
        let buffer_clone = response_buffer.clone();

        // Set up input report handler
        let input_closure = Closure::wrap(Box::new(move |event: HidInputReportEvent| {
            let data = event.data();
            let array = Uint8Array::new(&data.buffer());
            let mut packet = vec![0u8; array.length() as usize];
            array.copy_to(&mut packet);
            buffer_clone.borrow_mut().push(packet);
        }) as Box<dyn Fn(HidInputReportEvent)>);

        device
            .add_event_listener_with_callback("inputreport", input_closure.as_ref().unchecked_ref())
            .map_err(|e| TransportError::OpenFailed(js_error_to_string(e)))?;

        Ok(Self {
            device,
            channel_id: DEFAULT_CHANNEL_ID,
            response_buffer,
            _input_closure: input_closure,
        })
    }

    /// Send raw HID packets to the device.
    async fn send_packets(&self, packets: &[[u8; HID_PACKET_SIZE]]) -> TransportResult<()> {
        for packet in packets {
            let mut data = packet.to_vec();
            let promise = self.device.send_report_with_u8_array(0, &mut data);
            JsFuture::from(promise)
                .await
                .map_err(|e| TransportError::SendFailed(js_error_to_string(e)))?;
        }
        Ok(())
    }

    /// Wait for response packets from the device.
    async fn receive_response(&self) -> TransportResult<(Vec<u8>, u16)> {
        // Wait for packets to arrive
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 100; // 10 seconds total
        const POLL_INTERVAL_MS: u32 = 100;

        loop {
            // Check if we have enough data
            {
                let buffer = self.response_buffer.borrow();
                if !buffer.is_empty() {
                    // Check if first packet has length info
                    if buffer[0].len() >= 7 {
                        let expected_len = ((buffer[0][5] as usize) << 8) | (buffer[0][6] as usize);
                        let received: usize = buffer
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                if i == 0 {
                                    p.len().saturating_sub(7)
                                } else {
                                    p.len().saturating_sub(5)
                                }
                            })
                            .sum();

                        if received >= expected_len {
                            // We have all the data
                            let packets: Vec<&[u8]> = buffer.iter().map(|p| p.as_slice()).collect();
                            let result = parse_response(&packets, self.channel_id)?;
                            drop(buffer);
                            self.response_buffer.borrow_mut().clear();
                            return Ok(result);
                        }
                    }
                }
            }

            attempts += 1;
            if attempts >= MAX_ATTEMPTS {
                return Err(TransportError::ReceiveFailed(
                    "Timeout waiting for response".into(),
                ));
            }

            // Wait a bit before checking again
            gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
        }
    }

    /// Exchange an APDU command with the device (async version).
    pub async fn exchange_async(
        &self,
        command: &APDUCommand<Vec<u8>>,
    ) -> TransportResult<APDUAnswer<Vec<u8>>> {
        // Clear any stale data in the buffer
        self.response_buffer.borrow_mut().clear();

        // Serialize the APDU command
        // Always include Lc byte for Ledger devices
        let mut data = vec![command.cla, command.ins, command.p1, command.p2];
        data.push(command.data.len() as u8);
        data.extend_from_slice(&command.data);

        // Frame and send
        let packets = frame_apdu(&data, self.channel_id);
        self.send_packets(&packets).await?;

        // Wait for response
        let (response_data, sw) = self.receive_response().await?;

        // Build APDUAnswer
        let mut full_response = response_data;
        full_response.push((sw >> 8) as u8);
        full_response.push(sw as u8);

        Ok(APDUAnswer::from_answer(full_response)
            .map_err(|e| TransportError::ProtocolError(ApduError::FramingError(e.to_string())))?)
    }

    /// Check if the device is still connected.
    pub fn is_device_open(&self) -> bool {
        self.device.opened()
    }

    /// Close the device connection.
    pub async fn close(&self) -> TransportResult<()> {
        let promise = self.device.close();
        JsFuture::from(promise)
            .await
            .map_err(|e| TransportError::Other(js_error_to_string(e)))?;
        Ok(())
    }
}

impl LedgerTransport for WebHidTransport {
    fn exchange(&self, _command: &APDUCommand<Vec<u8>>) -> TransportResult<APDUAnswer<Vec<u8>>> {
        // Note: This is a blocking wrapper around the async exchange.
        // In WASM, you should prefer using exchange_async directly.
        // This implementation uses wasm_bindgen_futures to block.
        Err(TransportError::Other(
            "Use exchange_async for WebHID transport".into(),
        ))
    }

    fn is_connected(&self) -> bool {
        self.is_device_open()
    }
}

/// Check if WebHID is supported in the current browser.
pub fn is_webhid_supported() -> bool {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let hid_result = Reflect::get(&navigator, &"hid".into());
        if let Ok(hid) = hid_result {
            return !hid.is_undefined() && !hid.is_null();
        }
    }
    false
}

/// Get the HID interface from navigator.
fn get_hid() -> TransportResult<Hid> {
    let window = web_sys::window().ok_or(TransportError::NotSupported)?;
    let navigator = window.navigator();

    let hid_val =
        Reflect::get(&navigator, &"hid".into()).map_err(|_| TransportError::NotSupported)?;

    if hid_val.is_undefined() || hid_val.is_null() {
        return Err(TransportError::NotSupported);
    }

    hid_val
        .dyn_into::<Hid>()
        .map_err(|_| TransportError::NotSupported)
}

/// Convert a JsValue error to a string.
fn js_error_to_string(e: JsValue) -> String {
    if let Some(s) = e.as_string() {
        s
    } else if let Some(obj) = e.dyn_ref::<js_sys::Object>() {
        if let Ok(msg) = Reflect::get(obj, &"message".into()) {
            msg.as_string().unwrap_or_else(|| format!("{:?}", e))
        } else {
            format!("{:?}", e)
        }
    } else {
        format!("{:?}", e)
    }
}
