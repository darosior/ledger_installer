use ledger_transport_hidapi::{
    hidapi::{HidApi, HidError},
    TransportNativeHID,
};
use std::{
    error::Error,
    fmt::{Display, Formatter},
    time::Duration,
};

use crate::{
    bitcoin_latest_app, close_app, get_latest_apps, list_installed_apps, open_bitcoin_app,
    query_via_websocket_raw, DeviceInfo, StatusCode, BASE_SOCKET_URL, GET_VERSION_COMMAND,
};

pub fn check_apps_installed<M>(
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

pub fn check_latest_apps<M>(
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

pub trait Step {
    fn is_error(&self) -> bool;
    fn is_message(&self) -> bool;
    fn message(self) -> String;
}

#[derive(Debug, Clone)]
pub enum InstallStep<T> {
    NotStarted,
    Started,
    CloseApp,
    AllowInstall,
    Chunk,
    Completed,
    Info(String),
    Error(String),
    InstalledVersion(T),
}

impl<T> Step for InstallStep<T> {
    fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    fn is_message(&self) -> bool {
        !matches!(self, Self::Chunk) && !matches!(self, Self::NotStarted)
    }

    fn message(self) -> String {
        match self {
            InstallStep::Started => "Get device info from API...".into(),
            InstallStep::CloseApp => "Close the app in order to upgrade it...".into(),
            InstallStep::AllowInstall => {
                "Installing, please allow ledger manager on device...".into()
            }
            InstallStep::Completed => "Successfully installed the app.".into(),
            InstallStep::Info(msg) => msg,
            InstallStep::Error(msg) => msg,
            _ => "".into(),
        }
    }
}

pub fn install_app<M, P, T>(id: String, api: P, msg_callback: M, testnet: bool, reopen: bool)
where
    M: Fn(InstallStep<T>),
    P: Fn(&str) -> Option<TransportNativeHID>,
{
    msg_callback(InstallStep::Started);

    // if the app is open, we close it
    let mut close_cmd_send = false;

    // we create a temporary transport, see https://github.com/wizardsardine/async-hwi/issues/95
    let open = is_app_open(&{
        if let Some(api) = api(&id) {
            api
        } else {
            msg_callback(InstallStep::Error("Cannot open transport!".into()));
            return;
        }
    });

    match open {
        Some(true) => {
            // wait until the app is close
            loop {
                if !close_cmd_send {
                    // we create a temporary transport, see https://github.com/wizardsardine/async-hwi/issues/95
                    let close = close_app(&{
                        if let Some(api) = api(&id) {
                            api
                        } else {
                            msg_callback(InstallStep::Error("Cannot open transport!".into()));
                            return;
                        }
                    });
                    if let Err(e) = close {
                        msg_callback(InstallStep::Error(format!(
                            "Could not send close app command: {}",
                            e
                        )))
                    }
                    close_cmd_send = true;
                }
                // we create a temporary transport, see https://github.com/wizardsardine/async-hwi/issues/95
                let open = is_app_open(&{
                    if let Some(api) = api(&id) {
                        api
                    } else {
                        msg_callback(InstallStep::Info("Cannot open transport!".into()));
                        continue;
                    }
                });

                match open {
                    Some(false) => break,
                    None => {
                        msg_callback(InstallStep::Info("Closing app...".into()));
                        std::thread::sleep(Duration::from_millis(5000));
                        continue;
                    }
                    _ => {
                        msg_callback(InstallStep::CloseApp);
                    }
                }
            }
        }
        None => msg_callback(InstallStep::Error(
            "Could not check if the app is open.".into(),
        )),
        _ => {}
    }

    // now the app is closed, we can keep transport open
    let transport = if let Some(api) = api(&id) {
        api
    } else {
        msg_callback(InstallStep::Error("Cannot open transport!".into()));
        return;
    };

    if let Ok(device_info) = device_info(&transport) {
        let bitcoin_app = match bitcoin_latest_app(&device_info, testnet) {
            Ok(Some(a)) => a,
            Ok(None) => {
                msg_callback(InstallStep::Error(
                    "Could not get info about Bitcoin app.".into(),
                ));
                return;
            }
            Err(e) => {
                msg_callback(InstallStep::Error(format!(
                    "Error querying info about Bitcoin app: {}.",
                    e
                )));
                return;
            }
        };
        msg_callback(InstallStep::AllowInstall);
        // Now install the app by connecting through their websocket thing to their HSM. Make sure to
        // properly escape the parameters in the request's parameter.
        let install_ws_url =
            form_urlencoded::Serializer::new(format!("{}/install?", BASE_SOCKET_URL))
                .append_pair("targetId", &device_info.target_id.to_string())
                .append_pair("perso", &bitcoin_app.perso)
                .append_pair("deleteKey", &bitcoin_app.delete_key)
                .append_pair("firmware", &bitcoin_app.firmware)
                .append_pair("firmwareKey", &bitcoin_app.firmware_key)
                .append_pair("hash", &bitcoin_app.hash)
                .finish();
        if let Err(e) = query_via_websocket_raw(&transport, &install_ws_url, &msg_callback) {
            msg_callback(InstallStep::Error(format!(
                "Got an error when installing Bitcoin app from Ledger's remote HSM: {}.",
                e
            )));
            return;
        }
        msg_callback(InstallStep::Info("App successfully installed!".into()));

        if reopen {
            // drop transport to avoid the device node name  to change
            drop(transport);
            msg_callback(InstallStep::Info("Accept open app on device!".into()));
            let open_cmd = open_bitcoin_app(
                &{
                    if let Some(api) = api(&id) {
                        api
                    } else {
                        msg_callback(InstallStep::Error("Cannot open transport!".into()));
                        return;
                    }
                },
                testnet,
            );
            if let Err(e) = open_cmd {
                msg_callback(InstallStep::Error(format!(
                    "Fail to send OpenApp command to device: {}",
                    e
                )));
                return;
            }

            // wait for the app to open
            loop {
                std::thread::sleep(Duration::from_millis(5000));
                let open = is_app_open(&{
                    if let Some(api) = api(&id) {
                        api
                    } else {
                        msg_callback(InstallStep::Error("Cannot open transport!".into()));
                        continue;
                    }
                });

                if let Some(true) = open {
                    break;
                }
            }

            // get the installed version
            // TODO:
        }
        msg_callback(InstallStep::Completed);
    } else {
        msg_callback(InstallStep::Error("Fail to fetch device info!".into()));
    }
}

pub fn ledger_api() -> Result<HidApi, String> {
    HidApi::new().map_err(|e| format!("Error initializing HDI api: {}.", e))
}

pub fn ledger_api_raw() -> Result<HidApi, HidError> {
    HidApi::new()
}

pub fn device_info(ledger_api: &TransportNativeHID) -> Result<DeviceInfo, String> {
    DeviceInfo::new(ledger_api)
        .map_err(|e| format!("Error fetching device info: {}. Is the Ledger unlocked?", e))
}

// if the app is open we get StatusCode::ClaNotSupported
// see https://github.com/darosior/ledger_installer/issues/14
pub fn is_app_open(ledger_api: &TransportNativeHID) -> Option<bool> {
    let ver_answer = ledger_api.exchange(&GET_VERSION_COMMAND).ok()?;
    let ret = ver_answer.retcode();
    Some(ret == StatusCode::ClaNotSupported as u16)
}

pub struct VersionInfo {
    pub device_model: Option<Model>,
    pub device_version: Option<String>,
    pub mainnet_version: Option<Version>,
    pub testnet_version: Option<Version>,
}

#[allow(clippy::result_unit_err)]
pub fn get_version_info<V, M>(
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
