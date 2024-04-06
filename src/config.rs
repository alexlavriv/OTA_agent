use config::Config as ExternalConfig;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::fmt::{Display, Formatter, Result};
use url::Url;

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ArchType {
    AMD64,
    ARM64,
    WIN,
}

impl Display for ArchType {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            ArchType::AMD64 => write!(f, "AMD64"),
            ArchType::ARM64 => write!(f, "ARM64"),
            ArchType::WIN => write!(f, "WIN"),
        }
    }
}

#[cfg(unix)]
use std::env;

#[cfg(windows)]
pub fn get_arch() -> ArchType { ArchType::WIN }

#[cfg(unix)]
pub fn get_arch() -> ArchType {
    let arch_str = env::consts::ARCH;
    match arch_str {
        "x86_64" => ArchType::AMD64,
        "aarch64" => ArchType::ARM64,
        _ => {
            let error_msg = format!("Unsupported ARCH: {}", arch_str);
            log::error!("{error_msg}");
            panic!("{error_msg}")
        }
    }
}

///"logging":{
//       "default_level":2,
//       "loggers":[
//          {
//             "component":"core_comm",
//             "level":2
//          }
//       ],
//       "retention":14
//    }

#[derive(Clone, PartialEq, Eq, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warning = 3,
    Error = 4,
    Critical = 5,
    None = 6,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct ComponentLogger {
    pub component: String,
    pub level: LogLevel,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct LoggingConfig {
    pub default_level: LogLevel,
    pub loggers: Vec<ComponentLogger>,
    //Days
    pub retention: u32,
}

impl Display for LoggingConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let self_serialized = serde_json::to_string_pretty(&self).unwrap();
        write!(f, "\n{self_serialized}")
    }
}

/* -  "core_uri": "http://localhost:8700",
-  "ota_interval": 3600,
-  "ota_rest_port": 30000,
-  "ota_poll_frequency": 5
*/

pub struct Config {
    pub core_uri: Url,
    // Intervals are in seconds
    pub ota_interval: u64,
    pub ota_rest_port: u16,
    pub ota_poll_frequency: u32,
    pub enable_ota: bool,
    pub logging: LoggingConfig,
}

impl Config {
    pub fn new() -> Config {
        let ota_interval = 3600;
        let ota_rest_port = 30000;
        let ota_poll_frequency = 5;
        let core_uri = String::from("http://localhost:8700");
        let core_uri = Url::parse(&core_uri).unwrap();
        let enable_ota = true;

        let logging = LoggingConfig::default();

        Config {
            core_uri,
            ota_interval,
            ota_rest_port,
            ota_poll_frequency,
            enable_ota,
            logging,
        }
    }

    // settings is external object
    pub(crate) fn settings_to_config(settings: &ExternalConfig) -> LoggingConfig {
        Config::get_value_or_default(
            settings,
            "logging",
            LoggingConfig::default(),
        )
    }

    fn get_value_or_default<'a, T: Display + Deserialize<'a>>(
        settings: &ExternalConfig,
        key: &str,
        default_value: T,
    ) -> T {
        if let Ok(value) = settings.get(key) {
            log::info!("Config: Got {} value from settings file: {}", key, value);
            value
        } else {
            log::info!("Config: Kept default {} value: {}", key, default_value);
            default_value
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> LoggingConfig {
        LoggingConfig {
            default_level: LogLevel::Info,
            loggers: vec![],
            retention: 14
        }
    }
}

impl Default for Config { fn default() -> Self { Self::new() } }


