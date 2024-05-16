use crate::listener;
use crate::{gui::Message, gui::Message::LedgerServiceMsg, service::ServiceFn};
use std::error::Error;

use form_urlencoded::Serializer as UrlSerializer;
use ledger_manager::{
    bitcoin_latest_app, genuine_check, get_latest_apps,
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
) -> Result<(Model, Version, Version), Box<dyn Error>>
where
    M: Fn(&str, bool),
{
    log::info!("ledger::check_apps_installed()");
    msg_callback("Querying installed apps. Please confirm on device.", false);
    let mut mainnet = Version::NotInstalled;
    let mut testnet = Version::NotInstalled;
    let mut model = Model::Unknown;
    match list_installed_apps(transport) {
        Ok(apps) => {
            log::debug!("List installed apps:ok");
            msg_callback("List installed apps...", false);
            for app in apps.into_iter().flatten() {
                log::debug!("  [{}]", &app.version_name);
                if app.version_name == "Bitcoin" {
                    mainnet = Version::Installed(app.version);
                    model = Model::from_app_firmware(&app.firmware);
                    log::debug!("Mainnet App installed");
                } else if app.version_name == "Bitcoin Test" {
                    testnet = Version::Installed(app.version);
                    model = Model::from_app_firmware(&app.firmware);
                    log::debug!("Testnet App installed");
                }
            }
        }
        Err(e) => {
            log::debug!("Error listing installed applications: {}.", e);
            msg_callback(
                &format!("Error listing installed applications: {}.", e),
                true,
            );
            return Err(e);
        }
    }
    Ok((model, mainnet, testnet))
}

fn check_latest_apps<M>(
    transport: &TransportNativeHID,
    msg_callback: M,
) -> Result<(Version, Version), Box<dyn Error>>
where
    M: Fn(&str, bool),
{
    log::info!("ledger::check_latest_apps()");
    msg_callback("Querying latest apps on Ledger API...", false);

    let device_info = DeviceInfo::new(transport)?;
    let (bitcoin, test) = get_latest_apps(&device_info)?;

    let bitcoin = if let Some(app) = bitcoin {
        Version::Latest(app.version)
    } else {
        Version::None
    };

    let test = if let Some(app) = test {
        Version::Latest(app.version)
    } else {
        Version::None
    };

    Ok((bitcoin, test))
}

fn install_app<M>(transport: &TransportNativeHID, msg_callback: M, testnet: bool)
where
    M: Fn(&str, bool),
{
    log::debug!("ledger::install_app(testnet={})", testnet);

    msg_callback("Get device info from API...", false);
    if let Ok(device_info) = device_info(transport) {
        let bitcoin_app = match bitcoin_latest_app(&device_info, testnet) {
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

    if info.is_some() {
        // if it's our first connection, we check the if apps are installed & version
        msg_callback("Querying installed apps. Please confirm on device.", false);
        if actual_device_version.is_none() && device_version.is_some() {
            match check_apps_installed(&transport, &msg_callback) {
                Ok((model, mainnet, testnet)) => {
                    msg_callback("", false);
                    return Ok(VersionInfo {
                        device_model: Some(model),
                        device_version,
                        mainnet_version: Some(mainnet),
                        testnet_version: Some(testnet),
                    });
                }
                Err(e) => {
                    let msg = format!("Cannot check installed apps: {}", &*e.to_string());
                    msg_callback(&msg, true);
                }
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
    Latest(String),
    NotInstalled,
    None,
}

impl Version {
    pub fn is_none(&self) -> bool {
        matches!(self, Version::None)
    }

    #[allow(unused)]
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::Installed(version) => {
                write!(f, "{}", version)
            }
            Version::Latest(version) => {
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

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
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

impl Model {
    /// Determine device model based on BitcoinAppInfo.firmware value
    fn from_app_firmware(value: &str) -> Self {
        let chunks: Vec<&str> = value.split('/').collect();
        let model = chunks.first().map(|m| m.to_string());
        if let Some(model) = model {
            if model == "nanos" {
                Model::NanoS
            } else if model == "nanos+" {
                Model::NanoSP
                // i guess `nanox` for the nano x but i don't have device to test
            } else if model == "nanox" {
                Model::NanoX
            } else {
                Model::Unknown
            }
        } else {
            Model::Unknown
        }
    }
}

#[derive(Debug, Clone)]
pub enum LedgerMessage {
    UpdateMain,
    InstallMain,
    UpdateTest,
    InstallTest,
    TryConnect,
    GenuineCheck,

    Connected(Option<String>, Option<String>),
    MainAppVersion(Version),
    TestAppVersion(Version),
    DisplayMessage(String, bool),
    DeviceIsGenuine(Option<bool>),
    LatestApps(Version, Version),
}

pub struct LedgerService {
    sender: Sender<LedgerMessage>,
    receiver: Receiver<LedgerMessage>,
    loopback: Sender<LedgerMessage>,
    device_version: Option<String>,
    mainnet_version: Version,
    testnet_version: Version,
    last_mainnet: Version,
    last_testnet: Version,
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
        log::info!("LedgerService::send_to_gui({:?})", &msg);
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
            LedgerMessage::GenuineCheck => self.genuine_check(),
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
                // check for latest apps on ledger catalog
                if self.last_mainnet.is_none() || self.last_testnet.is_none() {
                    log::info!("Query Ledger catalog...");
                    if let Ok((bitcoin, test)) = check_latest_apps(&transport, |msg, alarm| {
                        Self::display_message(&sender, msg, alarm)
                    }) {
                        self.last_mainnet = bitcoin.clone();
                        self.last_testnet = test.clone();
                        self.send_to_gui(LedgerMessage::LatestApps(bitcoin, test))
                    } else {
                        Self::display_message(
                            &sender,
                            "Fail to get latest apps from Ledger API!",
                            true,
                        )
                    }
                }

                log::info!("Get device info...");
                // get versions of device & apps
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
        self.send_to_gui(LedgerMessage::LatestApps(
            self.last_mainnet.clone(),
            self.last_testnet.clone(),
        ))
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

    fn genuine_check(&mut self) {
        log::info!("LedgerService::genuine_check()");
        if let Some(transport) = self.connect() {
            self.send_to_gui(LedgerMessage::DisplayMessage(
                "Check if device genuine...".to_string(),
                false,
            ));
            log::info!("Check if device genuine...");
            if let Err(e) = genuine_check(&transport) {
                self.send_to_gui(LedgerMessage::DisplayMessage(e.to_string(), true));
                self.send_to_gui(LedgerMessage::DeviceIsGenuine(None));
            } else {
                self.send_to_gui(LedgerMessage::DisplayMessage("".to_string(), false));
                self.send_to_gui(LedgerMessage::DeviceIsGenuine(Some(true)));
            }
        } else {
            log::info!("Cannot connect to device!");
            self.send_to_gui(LedgerMessage::DisplayMessage(
                "Cannot connect to device!".to_string(),
                true,
            ));
            self.send_to_gui(LedgerMessage::DeviceIsGenuine(None));
        }
        log::info!("LedgerService::genuine_check() ended!");
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
            last_mainnet: Version::None,
            last_testnet: Version::None,
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
