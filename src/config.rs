use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::*;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Deserialize, Clone)]
pub struct Config {
    pub login: LoginConfig,
    pub shard: ShardConfig,
}
impl Config {
    fn new() -> Self {
        toml::from_str(&std::fs::read_to_string("config.toml").unwrap_or_else(|e| {
            log(Severity::Fatal, &format!("Can't open config.toml: {}", e));
            panic!();
        }))
        .unwrap_or_else(|e| {
            log(Severity::Fatal, &format!("Malformed config.toml: {}", e));
            panic!();
        })
    }
}

pub fn config_init() {
    assert!(CONFIG.get().is_none());
    log(Severity::Info, "Loading config...");
    if CONFIG.set(Config::new()).is_err() {
        panic!("Couldn't load config");
    }
    log(Severity::Info, "Successfully loaded config");
}

pub fn config_get() -> Config {
    CONFIG.get().expect("Config not initialized").clone()
}

#[derive(Deserialize, Clone)]
pub struct LoginConfig {
    pub listen_addr: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct ShardConfig {
    pub listen_addr: Option<String>,
    pub login_server_addr: Option<String>,
    pub external_addr: Option<String>,
}
