use crate::{
    ledger_service::{LedgerListener, LedgerMessage, Version},
    theme::{self, Theme},
};
use async_channel::{Receiver, Sender};
use iced::{
    alignment, executor,
    widget::{Button, Column, Container, Row, Rule, Space, Text},
    Alignment, Application, Element, Font, Length, Renderer,
};
use iced_runtime::{futures::Subscription, Command};

const ICONEX_ICONS_BYTES: &[u8] = include_bytes!("iconex-icons.ttf");

#[derive(Debug)]
pub struct Flags {
    pub ledger_sender: Sender<LedgerMessage>,
    pub ledger_receiver: Receiver<LedgerMessage>,
}

#[derive(Debug, Clone)]
pub enum Message {
    LedgerServiceMsg(LedgerMessage),

    UpdateMain,
    InstallMain,
    UpdateTest,
    InstallTest,
    #[allow(unused)]
    Connect,
    GenuineCheck,

    ResetAlarm,
    Result,
}

impl From<Result<(), iced::font::Error>> for Message {
    fn from(_: Result<(), iced::font::Error>) -> Self {
        Self::Result
    }
}

#[allow(unused)]
pub struct LedgerInstaller {
    ledger_sender: Sender<LedgerMessage>,
    ledger_receiver: Receiver<LedgerMessage>,
    ledger_model: Option<String>,
    ledger_version: Option<String>,
    main_app_version: Version,
    main_latest_version: Version,
    test_app_version: Version,
    test_latest_version: Version,
    user_message: Option<String>,
    device_is_genuine: Option<bool>,
    device_busy: bool,
    alarm: bool,
}

impl LedgerInstaller {
    #[allow(unused)]
    pub fn send_ledger_msg(&self, msg: LedgerMessage) {
        let sender = self.ledger_sender.clone();
        tokio::spawn(async move { sender.send(msg).await });
    }
}

impl Application for LedgerInstaller {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = Flags;

    fn new(args: Self::Flags) -> (Self, Command<Self::Message>) {
        let escrow = LedgerInstaller {
            ledger_sender: args.ledger_sender,
            ledger_receiver: args.ledger_receiver,
            ledger_model: None,
            ledger_version: None,
            main_app_version: Version::None,
            main_latest_version: Version::None,
            test_app_version: Version::None,
            test_latest_version: Version::None,
            user_message: Some("Please connect a device and unlock it...".to_string()),
            device_is_genuine: None,
            device_busy: false,
            alarm: false,
        };

        let cmd = iced::font::load(ICONEX_ICONS_BYTES).map(Message::from);
        (escrow, cmd)
    }

    fn title(&self) -> String {
        "Bacca - your Ledger Bitcoin companion".to_string()
    }

    fn update(&mut self, event: Message) -> Command<Message> {
        log::debug!("Gui receive: {:?}", event.clone());
        match event {
            Message::LedgerServiceMsg(ledger) => match ledger {
                LedgerMessage::Connected(model, version) => {
                    self.device_busy = false;
                    if model.is_none() {
                        self.main_app_version = Version::None;
                        self.main_latest_version = Version::None;
                        self.test_app_version = Version::None;
                        self.test_latest_version = Version::None;
                    }
                    self.ledger_model = model;
                    self.ledger_version = version;
                }
                LedgerMessage::MainAppVersion(version) => {
                    self.device_busy = false;
                    self.main_app_version = version;
                }
                LedgerMessage::TestAppVersion(version) => {
                    self.device_busy = false;
                    self.test_app_version = version;
                }
                LedgerMessage::DisplayMessage(s, alarm) => {
                    log::info!(
                        "LedgerInstaller::update(DisplayMessage({}), {:?})",
                        s,
                        alarm
                    );
                    if alarm {
                        self.device_busy = false
                    }
                    self.user_message = Some(s);
                    self.alarm = alarm;
                }
                LedgerMessage::DeviceIsGenuine(genuine) => {
                    self.device_is_genuine = genuine;
                    self.device_busy = false;
                }
                LedgerMessage::LatestApps(bitcoin, test) => {
                    self.main_latest_version = bitcoin;
                    self.test_latest_version = test;
                }
                _ => {
                    log::debug!(
                        "LedgerInstaller.update() => Unhandled message from ledger: {:?}!",
                        ledger
                    )
                }
            },
            Message::ResetAlarm => {
                self.alarm = false;
                self.user_message = None;
            }
            Message::UpdateMain => {
                self.send_ledger_msg(LedgerMessage::UpdateMain);
                self.device_busy = true;
            }
            Message::InstallMain => {
                self.main_app_version = Version::None;
                self.test_app_version = Version::None;
                self.device_busy = true;
                self.send_ledger_msg(LedgerMessage::InstallMain)
            }
            Message::UpdateTest => {
                self.send_ledger_msg(LedgerMessage::UpdateTest);
                self.device_busy = true;
            }
            Message::InstallTest => {
                self.main_app_version = Version::None;
                self.test_app_version = Version::None;
                self.device_busy = true;
                self.send_ledger_msg(LedgerMessage::InstallTest)
            }
            Message::GenuineCheck => {
                self.device_busy = true;
                self.send_ledger_msg(LedgerMessage::GenuineCheck)
            }
            Message::Result => {}
            _ => {
                log::debug!("LedgerInstaller.update() => Unhandled message {:?}", event)
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Message, Theme> {
        let display_app = self.ledger_model.is_some() && !self.alarm;

        let device = device_container(
            self.ledger_model.clone(),
            self.ledger_version.clone(),
            self.device_is_genuine,
            self.device_busy,
        );

        let apps = apps_container(
            self.main_app_version.clone(),
            self.main_latest_version.clone(),
            self.test_app_version.clone(),
            self.test_latest_version.clone(),
            self.device_busy,
        );

        let app = if display_app {
            Some(
                Column::new()
                    .push(
                        Row::new()
                            .push(Space::with_width(Length::Fill))
                            .push(Text::new("Device").size(20))
                            .push(Space::with_width(Length::Fill)),
                    )
                    .push(Space::with_height(5))
                    .push(device)
                    .push(Space::with_height(5))
                    .push(
                        Row::new()
                            .push(Space::with_width(Length::Fill))
                            .push(Text::new("Apps").size(20))
                            .push(Space::with_width(Length::Fill)),
                    )
                    .push(Space::with_height(5))
                    .push(apps),
            )
        } else {
            None
        };

        let reset_alarm: Option<Row<Message, Theme, Renderer>> =
            if self.alarm && self.ledger_model.is_some() {
                Some(
                    Row::new()
                        .push(Space::with_width(Length::Fill))
                        .push({
                            let mut reset = Button::new(" OK ");
                            reset = reset.on_press(Message::ResetAlarm);
                            reset
                        })
                        .push(Space::with_width(Length::Fill)),
                )
            } else {
                None
            };

        let hint_message =
            if self.alarm && (self.ledger_model.is_some() || self.main_latest_version.is_none()) {
                self.user_message.clone().map(|msg| {
                    Row::new()
                        .push(Space::with_width(Length::Fill))
                        .push(Text::new(msg.clone()))
                        .push(Space::with_width(Length::Fill))
                })
            } else if self.alarm && self.ledger_model.is_none() {
                self.user_message.clone().map(|_| {
                    Row::new()
                        .push(Space::with_width(Length::Fill))
                        .push(Text::new("Please connect a device and unlock it..."))
                        .push(Space::with_width(Length::Fill))
                })
            } else {
                None
            };

        let user_message = if !self.alarm {
            self.user_message.clone().map(|msg| {
                Row::new()
                    .push(Space::with_width(10))
                    .push(Text::new(msg.clone()))
            })
        } else {
            None
        };

        Container::new(
            Column::new()
                .push(Space::with_height(Length::Fill))
                .push_maybe(hint_message)
                .push_maybe(app)
                .push_maybe(reset_alarm)
                .push(Space::with_height(10))
                .push_maybe(user_message)
                .push(Space::with_height(5))
                .push(Space::with_height(Length::Fill)),
        )
        .padding(10)
        .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::from_recipe(LedgerListener {
            receiver: self.ledger_receiver.clone(),
        })
    }
}

fn device_container<'a>(
    model: Option<String>,
    version: Option<String>,
    is_genuine: Option<bool>,
    device_busy: bool,
) -> Container<'a, Message, Theme, Renderer> {
    let model = model.unwrap_or(" - ".to_string());
    let version = version.unwrap_or(" - ".to_string());

    // We do not allow user to click the button if service still processing a task w/ device
    let genuine_msg = if !device_busy {
        Some(Message::GenuineCheck)
    } else {
        None
    };

    // allow user to check if device is genuine only once per launch
    let button = if is_genuine.is_none() {
        Some(Button::new("Check").on_press_maybe(genuine_msg))
    } else {
        None
    };

    let genuine = is_genuine.map(|g| {
        if g {
            Text::new(" Yes ")
        } else {
            // FIXME: should we display in a more obvious way?
            Text::new("No!")
        }
    });

    let first_column_offset = 80;
    let first_column_width = 150;

    Container::new(
        Column::new()
            .push(
                Row::new()
                    .push(Space::with_width(first_column_offset))
                    .push(Text::new("Model:").width(first_column_width))
                    .push(Space::with_width(Length::Fill))
                    .push(Text::new(model))
                    .push(Space::with_width(Length::Fill)),
            )
            .push(Space::with_height(5))
            .push(
                Row::new()
                    .push(Space::with_width(first_column_offset))
                    .push(Text::new("Firmware:").width(first_column_width))
                    .push(Space::with_width(Length::Fill))
                    .push(Text::new(version))
                    .push(Space::with_width(Length::Fill)),
            )
            .push(Space::with_height(5))
            .push(
                Row::new()
                    .push(Space::with_width(first_column_offset))
                    .push(Text::new("Genuine:").width(first_column_width))
                    .push(Space::with_width(Length::Fill))
                    .push_maybe(button)
                    .push_maybe(genuine)
                    .push(Space::with_width(Length::Fill)),
            ),
    )
    .style(theme::Container::Frame)
    .padding(10)
}

fn apps_container<'a>(
    bitcoin_version: Version,
    bitcoin_latest: Version,
    test_version: Version,
    test_latest: Version,
    device_busy: bool,
) -> Container<'a, Message, Theme, Renderer> {
    let network_size = 25;
    let version_color = theme::color::GREY_3;
    let vertical_rule_position = 230;

    // It looks weird that we load iconex-icons.ttf by its name: Untitled1
    const ICONEX_ICONS: Font = Font::with_name("Untitled1");

    fn raw_btn(txt: &str, msg: Option<Message>) -> Button<Message, Theme> {
        Button::new(
            Row::new()
                .push(
                    Text::new('\u{605B}'.to_string())
                        .font(ICONEX_ICONS)
                        .width(Length::Fixed(40.0))
                        .size(25)
                        .horizontal_alignment(alignment::Horizontal::Center),
                )
                .push(Text::new(txt).size(25)),
        )
        .on_press_maybe(msg)
    }

    fn btn(
        version: &Version,
        latest: &Version,
        install_msg: Option<Message>,
        update_msg: Option<Message>,
    ) -> Container<'static, Message, Theme> {
        match (version, latest) {
            (Version::NotInstalled, _) => Container::new(raw_btn(" Install ", install_msg)),
            (Version::Installed(_), Version::Latest(_)) => {
                // FIXME: Here we only check if installed version differ from `latest` in Ledger catalog(stable), so if
                //     //  user have an `alpha` version installed we still offer him to `update` to the `stable` version
                if version != latest {
                    Container::new(raw_btn(" Update ", update_msg))
                } else {
                    Container::new(Text::new("Latest").size(25))
                }
            }
            _ => Container::new(Text::new(" - ").size(25)),
        }
    }

    fn version(version: Version) -> String {
        match version {
            Version::Installed(v) => format!("Version: {}", v),
            Version::NotInstalled => "Not installed".to_string(),
            _ => " - ".to_string(),
        }
    }

    // We do not allow user to click buttons if service still processing a task w/ device
    let install_bitcoin_msg = if !device_busy {
        Some(Message::InstallMain)
    } else {
        None
    };
    let update_bitcoin_msg = if !device_busy {
        Some(Message::UpdateMain)
    } else {
        None
    };
    let install_test_msg = if !device_busy {
        Some(Message::InstallTest)
    } else {
        None
    };
    let update_test_msg = if !device_busy {
        Some(Message::UpdateTest)
    } else {
        None
    };

    let bitcoin_button = btn(
        &bitcoin_version,
        &bitcoin_latest,
        install_bitcoin_msg,
        update_bitcoin_msg,
    );

    let test_button = btn(
        &test_version,
        &test_latest,
        install_test_msg,
        update_test_msg,
    );

    let bitcoin_version = version(bitcoin_version);

    let test_version = version(test_version);

    Container::new(
        Column::new()
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .push(Space::with_height(Length::Fill))
                            .push(Text::new("Bitcoin").size(network_size))
                            .push(
                                Text::new(bitcoin_version).style(theme::Text::Color(version_color)),
                            )
                            .push(Space::with_height(Length::Fill))
                            .width(vertical_rule_position)
                            .align_items(Alignment::Center),
                    )
                    .push(
                        Column::new()
                            .push(Space::with_height(5))
                            .push(Rule::vertical(1).style(theme::Rule::Light))
                            .push(Space::with_height(10)),
                    )
                    .push(Space::with_width(Length::Fill))
                    .push(
                        Column::new()
                            .push(Space::with_height(Length::Fill))
                            .push(bitcoin_button)
                            .push(Space::with_height(Length::Fill)),
                    )
                    .push(Space::with_width(Length::Fill)),
            )
            .push(
                Row::new()
                    .push(Space::with_width(30))
                    .push(Rule::horizontal(2))
                    .push(Space::with_width(30)),
            )
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .push(Space::with_height(Length::Fill))
                            .push(Text::new("Bitcoin Test").size(network_size))
                            .push(Text::new(test_version).style(theme::Text::Color(version_color)))
                            .push(Space::with_height(Length::Fill))
                            .width(vertical_rule_position)
                            .align_items(Alignment::Center),
                    )
                    .push(
                        Column::new()
                            .push(Space::with_height(10))
                            .push(Rule::vertical(1).style(theme::Rule::Light))
                            .push(Space::with_height(5)),
                    )
                    .push(Space::with_width(Length::Fill))
                    .push(
                        Column::new()
                            .push(Space::with_height(Length::Fill))
                            .push(test_button)
                            .push(Space::with_height(Length::Fill)),
                    )
                    .push(Space::with_width(Length::Fill)),
            ),
    )
    .style(theme::Container::Frame)
    .padding(10)
    .height(200)
}
