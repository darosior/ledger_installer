use crate::listener;
use crate::{gui::Message, gui::Message::LedgerServiceMsg, service::ServiceFn};

use ledger_manager::utils::{
    check_latest_apps, get_version_info, install_app, ledger_api, ledger_api_raw, Step, Version,
};
use ledger_manager::{genuine_check, ledger_transport_hidapi::TransportNativeHID};
use std::time::Duration;

listener!(LedgerListener, LedgerMessage, Message, LedgerServiceMsg);

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
                log::info!("No transport");
            }
        }
    }

    fn connect(&self) -> Option<TransportNativeHID> {
        match ledger_api_raw() {
            Ok(api) => match TransportNativeHID::new(&api) {
                Ok(api) => Some(api),
                Err(e) => {
                    log::info!("{:?}", e);
                    None
                }
            },
            Err(e) => {
                log::info!("{:?}", e);
                None
            }
        }
        // if let Some(api) = &ledger_api().ok() {
        //     TransportNativeHID::new(api).ok()
        // } else {
        //     None
        // }
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
                |msg| {
                    if msg.is_message() {
                        let alarm = msg.is_error();
                        Self::display_message(&sender, &msg.message(), alarm)
                    }
                },
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
