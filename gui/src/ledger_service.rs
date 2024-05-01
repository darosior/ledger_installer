use crate::listener;
use crate::{gui::Message, gui::Message::LedgerServiceMsg, service::ServiceFn};

use form_urlencoded::Serializer as UrlSerializer;
use ledger_manager::{
    bitcoin_app,
    ledger_transport_hidapi::{hidapi::HidApi, TransportNativeHID},
    list_installed_apps, query_via_websocket, DeviceInfo, BASE_SOCKET_URL,
};
use std::fmt::{Display, Formatter};
use std::time::Duration;

listener!(LedgerListener, LedgerMessage, Message, LedgerServiceMsg);

// TODO: those helpers, used by both the CLI and the GUI, should live in the lib somehow.

fn check_apps_installed<M>(
    transport: &TransportNativeHID,
    msg_callback: M,
) -> Result<(bool, bool), ()>
where
    M: Fn(&str, bool),
{
    log::info!("ledger::check_apps_installed()");
    msg_callback("Querying installed apps. Please confirm on device.", false);
    let mut mainnet = false;
    let mut testnet = false;
    match list_installed_apps(transport) {
        Ok(apps) => {
            log::debug!("List installed apps:");
            msg_callback("List installed apps...", false);
            for app in apps {
                log::debug!("  [{}]", &app.name);
                if app.name == "Bitcoin" {
                    mainnet = true
                }
                if app.name == "Bitcoin Test" {
                    testnet = true
                }
            }
        }
        Err(e) => {
            log::debug!("Error listing installed applications: {}.", e);
            msg_callback(
                &format!("Error listing installed applications: {}.", e),
                true,
            );
            return Err(());
        }
    }
    if mainnet {
        log::debug!("Mainnet App installed");
    }
    if testnet {
        log::debug!("Testnet App installed");
    }
    Ok((mainnet, testnet))
}

fn install_app<M>(transport: &TransportNativeHID, msg_callback: M, testnet: bool)
where
    M: Fn(&str, bool),
{
    log::debug!("ledger::install_app(testnet={})", testnet);

    msg_callback("Get device info from API...", false);
    if let Ok(device_info) = device_info(transport) {
        let bitcoin_app = match bitcoin_app(&device_info, testnet) {
            Ok(Some(a)) => a,
            Ok(None) => {
                msg_callback("Could not get info about Bitcoin app.", true);
                return;
            }
            Err(e) => {
                msg_callback(
                    &format!("Error querying info about Bitcoin app: {}.", e),
                    true,
                );
                return;
            }
        };
        msg_callback(
            "Installing, please allow Ledger manager on device...",
            false,
        );
        // Now install the app by connecting through their websocket thing to their HSM. Make sure to
        // properly escape the parameters in the request's parameter.
        let install_ws_url = UrlSerializer::new(format!("{}/install?", BASE_SOCKET_URL))
            .append_pair("targetId", &device_info.target_id.to_string())
            .append_pair("perso", &bitcoin_app.perso)
            .append_pair("deleteKey", &bitcoin_app.delete_key)
            .append_pair("firmware", &bitcoin_app.firmware)
            .append_pair("firmwareKey", &bitcoin_app.firmware_key)
            .append_pair("hash", &bitcoin_app.hash)
            .finish();
        msg_callback("Install app...", false);
        if let Err(e) = query_via_websocket(transport, &install_ws_url) {
            msg_callback(
                &format!(
                    "Got an error when installing Bitcoin app from Ledger's remote HSM: {}.",
                    e
                ),
                false,
            );
            return;
        }
        msg_callback("Successfully installed the app.", false);
    } else {
        msg_callback("Fail to fetch device info!", true);
    }
}

fn ledger_api() -> Result<HidApi, String> {
    HidApi::new().map_err(|e| format!("Error initializing HDI api: {}.", e))
}

fn device_info(ledger_api: &TransportNativeHID) -> Result<DeviceInfo, String> {
    log::info!("ledger::device_info()");
    DeviceInfo::new(ledger_api)
        .map_err(|e| format!("Error fetching device info: {}. Is the Ledger unlocked?", e))
}

struct VersionInfo {
    pub device_model: Option<Model>,
    pub device_version: Option<String>,
    pub mainnet_version: Option<Version>,
    pub testnet_version: Option<Version>,
}

fn get_app_version(info: &DeviceInfo, testnet: bool) -> Result<(Model, Version), String> {
    log::debug!("get_app_version()");
    match bitcoin_app(info, testnet) {
        Ok(r) => {
            log::debug!("decoding app data");
            // example for nano s
            // BitcoinAppV2 { version_name: "Bitcoin Test", perso: "perso_11", delete_key: "nanos/2.1.0/bitcoin_testnet/app_2.2.1_del_key", firmware: "nanos/2.1.0/bitcoin_testnet/app_2.2.1", firmware_key: "nanos/2.1.0/bitcoin_testnet/app_2.2.1_key", hash: "7f07efc20d96faaf8c93bd179133c88d1350113169da914f88e52beb35fcdd1e" }
            // example for nano s+
            // BitcoinAppV2 { version_name: "Bitcoin Test", perso: "perso_11", delete_key: "nanos+/1.1.0/bitcoin_testnet/app_2.2.0-beta_del_key", firmware: "nanos+/1.1.0/bitcoin_testnet/app_2.2.0-beta", firmware_key: "nanos+/1.1.0/bitcoin_testnet/app_2.2.0-beta_key", hash: "3c6d6ebebb085da948c0211434b90bc4504a04a133b8d0621aa0ee91fd3a0b4f" }
            if let Some(app) = r {
                let chunks: Vec<&str> = app.firmware.split('/').collect();
                let model = chunks.first().map(|m| m.to_string());
                let version = chunks.last().map(|m| m.to_string());
                if let (Some(model), Some(version)) = (model, version) {
                    let model = if model == "nanos" {
                        Model::NanoS
                    } else if model == "nanos+" {
                        Model::NanoSP
                        // i guess `nanox` for the nano x but i don't have device to test
                    } else if model == "nanox" {
                        Model::NanoX
                    } else {
                        Model::Unknown
                    };

                    let version = if version.contains("app_") {
                        version.replace("app_", "")
                    } else {
                        version
                    };

                    let version = Version::Installed(version);
                    if testnet {
                        log::debug!("Testnet Model{}, Version{}", model.clone(), version.clone());
                    } else {
                        log::debug!("Mainnet Model{}, Version{}", model.clone(), version.clone());
                    }
                    Ok((model, version))
                } else {
                    Err(format!("Failed to parse  model/version in {:?}", chunks))
                }
            } else {
                log::debug!("Fail to get version info");
                Err("Fail to get version info".to_string())
            }
        }
        Err(e) => {
            log::debug!("Fail to get version info: {}", e);
            Err(format!("Fail to get version info: {}", e))
        }
    }
}

#[allow(clippy::result_unit_err)]
fn get_version_info<V, M>(
    transport: TransportNativeHID,
    actual_device_version: &Option<String>,
    version_callback: V,
    msg_callback: M,
) -> Result<VersionInfo, ()>
where
    V: Fn(Option<String>, Option<String>),
    M: Fn(&str, bool),
{
    log::info!("ledger::get_version_info()");
    let mut device_version: Option<String> = None;
    let info = match device_info(&transport) {
        Ok(info) => {
            log::info!("Device connected");
            log::debug!("Device version: {}", &info.version);
            msg_callback(
                &format!("Device connected, version: {}", &info.version),
                false,
            );
            if actual_device_version.is_none() {
                version_callback(Some("Ledger".to_string()), Some(info.version.clone()));
            }
            device_version = Some(info.version.clone());
            Some(info)
        }
        Err(e) => {
            log::debug!("Failed connect device: {}", &e);
            msg_callback(&e, true);
            None
        }
    };

    if let Some(info) = info {
        // if it's our first connection, we check the if apps are installed & version
        msg_callback("Querying installed apps. Please confirm on device.", false);
        if actual_device_version.is_none() && device_version.is_some() {
            if let Ok((main_installed, test_installed)) =
                check_apps_installed(&transport, &msg_callback)
            {
                // get the mainnet app version name
                let (main_model, main_version) = if main_installed {
                    msg_callback("Call ledger API....", false);
                    match get_app_version(&info, true) {
                        Ok((model, version)) => (model, version),
                        Err(e) => {
                            msg_callback(&e, true);
                            (Model::Unknown, Version::None)
                        }
                    }
                } else {
                    log::debug!("Mainnet app not installed!");
                    // self.display_message("Mainnet app not installed!", false);
                    (Model::Unknown, Version::NotInstalled)
                };

                // get the testnet app version name
                let (test_model, test_version) = if test_installed {
                    msg_callback("Call ledger API....", false);
                    match get_app_version(&info, true) {
                        Ok((model, version)) => (model, version),
                        Err(e) => {
                            msg_callback(&e, false);
                            (Model::Unknown, Version::None)
                        }
                    }
                } else {
                    log::debug!("Testnet app not installed!");
                    (Model::Unknown, Version::NotInstalled)
                };

                let model = match (&main_model, &test_model) {
                    (Model::Unknown, _) => test_model,
                    _ => main_model,
                };
                // clear message after app version check (after app install)
                msg_callback("", false);
                return Ok(VersionInfo {
                    device_model: Some(model),
                    device_version,
                    mainnet_version: Some(main_version),
                    testnet_version: Some(test_version),
                });
            } else {
                msg_callback("Cannot check installed apps", false);
            }
        }
        Ok(VersionInfo {
            device_model: None,
            device_version,
            mainnet_version: None,
            testnet_version: None,
        })
    } else {
        Err(())
    }
}

#[derive(Debug, Clone)]
pub enum Version {
    Installed(String),
    NotInstalled,
    None,
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::Installed(version) => {
                write!(f, "{}", version)
            }
            Version::NotInstalled => {
                write!(f, "Not installed!")
            }
            Version::None => {
                write!(f, " - ")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Model {
    NanoS,
    NanoSP,
    NanoX,
    Unknown,
}

impl Display for Model {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Model::NanoS => {
                write!(f, "Nano S")
            }
            Model::NanoSP => {
                write!(f, "Nano S+")
            }
            Model::NanoX => {
                write!(f, "Nano X")
            }
            _ => {
                write!(f, "")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum LedgerMessage {
    #[allow(unused)]
    UpdateMain,
    InstallMain,
    #[allow(unused)]
    UpdateTest,
    InstallTest,
    TryConnect,

    Connected(Option<String>, Option<String>),
    MainAppVersion(Version),
    #[allow(unused)]
    MainAppNextVersion(Version),
    TestAppVersion(Version),
    #[allow(unused)]
    TestAppNextVersion(Version),
    DisplayMessage(String, bool),
}

pub struct LedgerService {
    sender: Sender<LedgerMessage>,
    receiver: Receiver<LedgerMessage>,
    loopback: Sender<LedgerMessage>,
    device_version: Option<String>,
    mainnet_version: Version,
    testnet_version: Version,
}

impl LedgerService {
    pub fn start(mut self) {
        tokio::spawn(async move {
            self.run().await;
        });
    }
    /// Send a LedgerMessage to the GUI via async-channel
    fn send_to_gui(&self, msg: LedgerMessage) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            if sender.send(msg).await.is_err() {
                log::debug!("LedgerService.send_to_gui() -> Fail to send Message")
            };
        });
    }

    /// Handle a LedgerMessage received from the GUI via async-channel
    fn handle_message(&mut self, msg: LedgerMessage) {
        match &msg {
            LedgerMessage::TryConnect => {
                if self.device_version.is_none() {
                    self.poll_later();
                    self.poll();
                }
            }
            LedgerMessage::UpdateMain => self.update_main(),
            LedgerMessage::InstallMain => self.install_main(),
            LedgerMessage::UpdateTest => self.update_test(),
            LedgerMessage::InstallTest => self.install_test(),
            _ => {
                log::debug!("LedgerService.handle_message({:?}) -> unhandled!", msg)
            }
        }
    }

    /// Delayed self sent message in order to call poll() later
    fn poll_later(&self) {
        let loopback = self.loopback.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if loopback.send(LedgerMessage::TryConnect).await.is_err() {
                log::debug!("Fail to send Message")
            };
        });
    }

    /// Try to connect to the ledger device and get firmware/bitcoin-apps versions
    fn poll(&mut self) {
        if self.device_version.is_none() {
            let sender = self.sender.clone();
            log::info!("Try to poll device...");
            if let Some(transport) = self.connect() {
                if let Ok(info) = get_version_info(
                    transport,
                    &self.device_version,
                    |model, version| {
                        self.send_to_gui(LedgerMessage::Connected(model, version));
                    },
                    |msg, alarm| Self::display_message(&sender, msg, alarm),
                ) {
                    match (info.device_model, info.device_version) {
                        (None, Some(version)) => {
                            self.device_version = Some(version);
                        }
                        (Some(model), Some(version)) => {
                            self.device_version = Some(version.clone());
                            self.send_to_gui(LedgerMessage::Connected(
                                Some(model.to_string()),
                                Some(version),
                            ));
                        }
                        _ => {}
                    }
                    if let (Some(main), Some(test)) = (info.mainnet_version, info.testnet_version) {
                        self.mainnet_version = main;
                        self.testnet_version = test;
                        self.update_apps_version();
                    }
                    // clear message if not app detected
                    Self::display_message(&sender, "", false)
                }
            } else {
                // Inform GUI that ledger disconnected
                self.send_to_gui(LedgerMessage::Connected(None, None));
                log::debug!("No transport");
            }
        }
    }

    fn connect(&self) -> Option<TransportNativeHID> {
        if let Some(api) = &ledger_api().ok() {
            TransportNativeHID::new(api).ok()
        } else {
            None
        }
    }

    fn update_apps_version(&self) {
        match &self.mainnet_version {
            Version::None => {}
            _ => {
                self.send_to_gui(LedgerMessage::MainAppVersion(self.mainnet_version.clone()));
            }
        }
        match &self.testnet_version {
            Version::None => {}
            _ => {
                self.send_to_gui(LedgerMessage::TestAppVersion(self.testnet_version.clone()));
            }
        }
    }

    fn install(&mut self, testnet: bool) {
        let sender = self.sender.clone();
        Self::display_message(&sender, "Try to download last firmware...", false);

        self.send_to_gui(LedgerMessage::MainAppVersion(Version::None));
        self.send_to_gui(LedgerMessage::TestAppVersion(Version::None));

        self.install_app(testnet);

        self.device_version = None;
        self.poll();
    }

    fn install_app(&mut self, testnet: bool) {
        let sender = self.sender.clone();
        if let Some(transport) = self.connect() {
            install_app(
                &transport,
                |msg, alarm| Self::display_message(&sender, msg, alarm),
                testnet,
            )
        }
    }

    fn install_main(&mut self) {
        self.install(false);
    }

    fn update_main(&mut self) {
        self.install(false);
    }

    fn install_test(&mut self) {
        self.install(true);
    }

    fn update_test(&mut self) {
        self.install(true);
    }

    fn display_message(sender: &Sender<LedgerMessage>, msg: &str, alarm: bool) {
        let sender = sender.clone();
        let msg = LedgerMessage::DisplayMessage(msg.to_string(), alarm);
        tokio::spawn(async move {
            if sender.send(msg).await.is_err() {
                log::debug!("LedgerService.send_to_gui() -> Fail to send Message")
            };
        });
    }
}

impl ServiceFn<LedgerMessage, Sender<LedgerMessage>> for LedgerService {
    fn new(
        sender: Sender<LedgerMessage>,
        receiver: Receiver<LedgerMessage>,
        loopback: Sender<LedgerMessage>,
    ) -> Self {
        LedgerService {
            sender,
            receiver,
            loopback,
            device_version: None,
            mainnet_version: Version::None,
            testnet_version: Version::None,
        }
    }

    async fn run(&mut self) {
        self.poll();
        self.poll_later();
        loop {
            if let Ok(msg) = self.receiver.try_recv() {
                self.handle_message(msg);
            }
            // cpu load is not visible w/ 10ns but we can increase it w/o performance penalty
            tokio::time::sleep(Duration::from_nanos(10)).await;
        }
    }
}
