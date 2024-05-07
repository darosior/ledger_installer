use crate::{
    ledger_service::{LedgerListener, LedgerMessage, Version},
    theme::Theme,
};
use async_channel::{Receiver, Sender};
use iced::{
    alignment::Horizontal,
    executor,
    widget::{container, Button, Column, Row, Space, Text},
    Application, Element, Length, Renderer,
};
use iced_runtime::{futures::Subscription, Command};

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
}

#[allow(unused)]
pub struct LedgerInstaller {
    ledger_sender: Sender<LedgerMessage>,
    ledger_receiver: Receiver<LedgerMessage>,
    ledger_model: Option<String>,
    ledger_version: Option<String>,
    main_app_version: Version,
    main_next_version: Version,
    test_app_version: Version,
    test_next_version: Version,
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
            main_next_version: Version::None,
            test_app_version: Version::None,
            test_next_version: Version::None,
            user_message: None,
            device_is_genuine: None,
            device_busy: false,
            alarm: false,
        };

        (escrow, Command::none())
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
                        self.main_next_version = Version::None;
                        self.test_app_version = Version::None;
                        self.test_next_version = Version::None;
                    }
                    self.ledger_model = model;
                    self.ledger_version = version;
                }
                LedgerMessage::MainAppVersion(version) => {
                    self.device_busy = false;
                    self.main_app_version = version;
                }
                LedgerMessage::MainAppNextVersion(version) => {
                    self.device_busy = false;
                    self.main_next_version = version;
                }
                LedgerMessage::TestAppVersion(version) => {
                    self.device_busy = false;
                    self.test_app_version = version;
                }
                LedgerMessage::TestAppNextVersion(version) => {
                    self.device_busy = false;
                    self.test_next_version = version;
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
                // self.send_ledger_msg(LedgerMessage::UpdateMain);
                // self.device_busy = true;
            }
            Message::InstallMain => {
                self.main_app_version = Version::None;
                self.test_app_version = Version::None;
                self.device_busy = true;
                self.send_ledger_msg(LedgerMessage::InstallMain)
            }
            Message::UpdateTest => {
                // self.send_ledger_msg(LedgerMessage::UpdateTest)
                // self.device_busy = true;
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
            _ => {
                log::debug!("LedgerInstaller.update() => Unhandled message {:?}", event)
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Message, Theme> {
        let first_line = match (
            &self.ledger_model,
            &self.ledger_version,
            self.alarm && self.user_message.is_some(),
        ) {
            (_, _, true) => Text::new(self.user_message.as_ref().unwrap()),
            (Some(model), None, _) => Text::new(format!("Model: {}  Version: unknown ", model)),
            (Some(model), Some(version), _) => {
                Text::new(format!("Model: {}        Version: {}", model, version))
            }
            _ => Text::new("Please connect a device and unlock it..."),
        }
        .horizontal_alignment(Horizontal::Center);

        let display_app =
            self.ledger_model.is_some() && !(self.alarm && self.user_message.is_some());

        let main_app = if display_app {
            Some(app_row(
                "Bitcoin app",
                &self.main_app_version,
                self.device_busy,
                Message::UpdateMain,
                Message::InstallMain,
            ))
        } else {
            None
        };

        let test_app = if display_app {
            Some(app_row(
                "Testnet app",
                &self.test_app_version,
                self.device_busy,
                Message::UpdateTest,
                Message::InstallTest,
            ))
        } else {
            None
        };
        let btn_genuine_msg = if !self.device_busy
            && self.main_app_version != Version::None
            && self.device_is_genuine.is_none()
        {
            Some(Message::GenuineCheck)
        } else {
            None
        };

        let genuine_text = match self.device_is_genuine {
            None => "  Check if device is genuine  ",
            Some(false) => "  Device is NOT genuine!  ",
            Some(true) => "  Device is genuine!  ",
        };

        let genuine_check_btn = if display_app {
            Some(
                Row::new()
                    .push(Space::with_width(Length::Fill))
                    .push(Button::new(genuine_text).on_press_maybe(btn_genuine_msg))
                    .push(Space::with_width(Length::Fill)),
            )
        } else {
            None
        };

        let reset_alarm: Option<Row<Message, Theme, Renderer>> = if self.alarm {
            Some(
                Row::new()
                    .push(Space::with_width(Length::Fill))
                    .push({
                        let mut reset = Button::new("OK");
                        reset = reset.on_press(Message::ResetAlarm);
                        reset
                    })
                    .push(Space::with_width(Length::Fill)),
            )
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

        Column::new()
            .push(Space::with_height(Length::Fill))
            .push(
                Row::new()
                    .push(Space::with_width(Length::Fill))
                    .push(first_line)
                    .push(Space::with_width(Length::Fill)),
            )
            .push(Space::with_height(10))
            .push_maybe(main_app)
            .push(Space::with_height(10))
            .push_maybe(test_app)
            .push_maybe(reset_alarm)
            .push(Space::with_height(Length::Fill))
            .push_maybe(genuine_check_btn)
            .push(Space::with_height(10))
            .push_maybe(user_message)
            .push(Space::with_height(5))
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

fn app_row<'a>(
    app_name: &'a str,
    version: &Version,
    genuine_test_running: bool,
    _update_msg: Message,
    install_msg: Message,
) -> Row<'a, Message, Theme, Renderer> {
    let button_text = match version {
        Version::Installed(_) => "Try update".to_string(),
        Version::NotInstalled => "Install".to_string(),
        Version::None => "".to_string(),
    };
    let mut button = Button::new(
        Text::new(button_text)
            .size(11)
            .width(100)
            .horizontal_alignment(Horizontal::Center),
    );

    match (version, genuine_test_running) {
        (Version::Installed(_), false) => {
            // button = button.on_press(update_msg);
        }
        (Version::NotInstalled, false) => {
            button = button.on_press(install_msg);
        }
        _ => {}
    }

    Row::new()
        .push(Space::with_width(Length::Fill))
        .push(
            container(
                Row::new()
                    .push(Text::new(app_name))
                    .push(Space::with_width(Length::Fill))
                    .push(Text::new(version.to_string())),
            )
            .width(220),
        )
        .push(Space::with_width(15))
        .push(button)
        .push(Space::with_width(Length::Fill))
}
