use std::{net::SocketAddr, sync::OnceLock};

use serde::Deserialize;

use crate::error::*;

include!(concat!(env!("OUT_DIR"), "/config_generated.rs"));

static CONFIG: OnceLock<Config> = OnceLock::new();
static CONFIG_DEFAULT: OnceLock<Config> = OnceLock::new();

pub fn config_init() -> &'static Config {
    assert!(CONFIG.get().is_none());

    // Allow overriding config path via command line argument
    let file_override = std::env::args().nth(1);
    let file_path = file_override.as_deref().unwrap_or(CONFIG_PATH);
    if let Some(loaded_config) = Config::load(file_path) {
        if CONFIG.set(loaded_config).is_err() {
            panic_log("Couldn't initialize config");
        }
        log(Severity::Info, "Loaded config");
    }
    config_get()
}

pub fn config_get() -> &'static Config {
    // really, the only time the config should be accessed
    // before it's ready is while it's loading, by log()
    let fallback = CONFIG_DEFAULT.get_or_init(Config::default);
    match CONFIG.get() {
        Some(c) => c,
        None => fallback,
    }
}

pub trait SettingDefault<T> {
    fn setting_default(self) -> T;
}
impl<T: Clone> SettingDefault<T> for T {
    fn setting_default(self) -> T {
        self
    }
}
impl SettingDefault<String> for &str {
    fn setting_default(self) -> String {
        self.to_string()
    }
}
impl SettingDefault<SocketAddr> for &str {
    fn setting_default(self) -> SocketAddr {
        self.parse().expect("Invalid default SocketAddr")
    }
}

macro_rules! define_setting {
    ($name:ident, $ty:ty, $dv:expr) => {
        #[derive(Deserialize, Default)]
        #[serde(transparent)]
        pub struct $name(Option<$ty>);
        impl $name {
            pub fn get(&self) -> $ty {
                match self.0 {
                    Some(ref v) => v.clone(),
                    None => SettingDefault::<$ty>::setting_default($dv),
                }
            }

            pub fn is_set(&self) -> bool {
                self.0.is_some()
            }

            pub fn is_set_to_default(&self) -> bool {
                self.0 == Some(SettingDefault::<$ty>::setting_default($dv))
            }
        }
    };
}
use define_setting;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    // Validates that the config schema file can be parsed.
    fn test_config_schema() {
        Config::load(CONFIG_SCHEMA_PATH).expect("Config schema is invalid!");
    }
}
