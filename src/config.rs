use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::*;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Deserialize, Clone, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub login: LoginConfig,
    pub shard: ShardConfig,
}
impl Config {
    fn new() -> Self {
        let file_read = std::fs::read_to_string("config.toml");
        if let Err(e) = file_read {
            if let std::io::ErrorKind::NotFound = e.kind() {
                log(Severity::Warning, "No config.toml, using default config");
                return Self::default();
            } else {
                log(Severity::Fatal, &format!("Can't open config.toml: {}", e));
                panic!();
            }
        }

        let file_contents = file_read.unwrap();
        toml::from_str(&file_contents).unwrap_or_else(|e| {
            log(Severity::Fatal, &format!("Malformed config.toml: {}", e));
            panic!();
        })
    }
}

pub fn config_init() -> Config {
    assert!(CONFIG.get().is_none());
    if CONFIG.set(Config::new()).is_err() {
        panic!("Couldn't load config");
    }
    log(Severity::Info, "Loaded config");
    config_get()
}

pub fn config_get() -> Config {
    // really, the only time the config should be accessed
    // before it's ready is while it's loading, by log()
    match CONFIG.get() {
        Some(c) => c.clone(),
        None => Config::default(),
    }
}

#[derive(Deserialize, Clone, Default)]
pub struct GeneralConfig {
    pub logging_level: Option<usize>,
}

#[derive(Deserialize, Clone, Default)]
pub struct LoginConfig {
    pub log_path: Option<String>,
    pub listen_addr: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
pub struct ShardConfig {
    pub log_path: Option<String>,
    pub listen_addr: Option<String>,
    pub external_addr: Option<String>,
    pub login_server_addr: Option<String>,
    pub login_server_conn_interval: Option<u64>,
    pub visibility_range: Option<usize>,
}
