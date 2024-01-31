use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::*;

mod general_settings;
mod login_settings;
mod shard_settings;

pub use self::{
    general_settings::GeneralConfig, login_settings::LoginConfig, shard_settings::ShardConfig,
};

static CONFIG: OnceLock<Config> = OnceLock::new();
static CONFIG_DEFAULT: OnceLock<Config> = OnceLock::new();

#[derive(Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub login: LoginConfig,
    pub shard: ShardConfig,
}
impl Config {
    fn load() -> Option<Self> {
        #[derive(Deserialize)]
        struct ConfigLayout {
            general: Option<GeneralConfig>,
            login: Option<LoginConfig>,
            shard: Option<ShardConfig>,
        }
        let file_read = std::fs::read_to_string("config.toml");
        if let Err(e) = file_read {
            if let std::io::ErrorKind::NotFound = e.kind() {
                log(Severity::Warning, "No config.toml, using default config");
                return None;
            } else {
                panic_log(&format!("Can't open config.toml: {}", e));
            }
        }

        let file_contents = file_read.unwrap();
        let parsed: ConfigLayout = toml::from_str(&file_contents).unwrap_or_else(|e| {
            panic_log(&format!("Malformed config.toml: {}", e));
        });

        Some(Config {
            general: parsed.general.unwrap_or_default(),
            login: parsed.login.unwrap_or_default(),
            shard: parsed.shard.unwrap_or_default(),
        })
    }
}

pub fn config_init() -> &'static Config {
    assert!(CONFIG.get().is_none());
    if let Some(loaded_config) = Config::load() {
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

macro_rules! define_setting {
    ($name:ident, $ty:ty, $dv:expr) => {
        #[derive(Deserialize, Default)]
        #[serde(transparent)]
        pub struct $name(Option<$ty>);
        impl $name {
            pub fn get(&self) -> $ty {
                match self.0 {
                    Some(ref v) => v.clone(),
                    None => $dv.into(),
                }
            }
        }
    };
}
use define_setting;
