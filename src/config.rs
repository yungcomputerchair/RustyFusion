use std::{collections::HashMap, fmt::Write as _, net::SocketAddr, sync::OnceLock};

use serde::Deserialize;

use crate::error::*;

include!(concat!(env!("OUT_DIR"), "/config_generated.rs"));

static CONFIG: OnceLock<Config> = OnceLock::new();
static CONFIG_DEFAULT: OnceLock<Config> = OnceLock::new();

fn auto_quote_toml_value(value: &str) -> String {
    // Already a quoted TOML string
    if value.starts_with('"') && value.ends_with('"')
        || value.starts_with('\'') && value.ends_with('\'')
    {
        return value.to_string();
    }

    // Bool or TOML keyword
    if matches!(value, "true" | "false" | "inf" | "nan") {
        return value.to_string();
    }

    // Integer or float (including negatives and underscores like 10_080)
    if value.parse::<i64>().is_ok() || value.parse::<u64>().is_ok() || value.parse::<f64>().is_ok()
    {
        return value.to_string();
    }

    // Hex/octal/binary integer literals
    if value.starts_with("0x") || value.starts_with("0o") || value.starts_with("0b") {
        return value.to_string();
    }

    // Treat everything else as a string
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn get_overrides(args: Vec<String>) -> FFResult<Option<Config>> {
    // Accept positional args of the form `section.key=value`.
    // String values are auto-quoted; booleans and numbers are passed as-is.
    // Group by section into a partial TOML string.
    let mut sections: HashMap<String, Vec<String>> = HashMap::new();
    for arg in &args {
        let arg = arg.trim_start_matches('-');
        if let Some((path, value)) = arg.split_once('=') {
            if let Some((section, key)) = path.split_once('.') {
                sections
                    .entry(section.to_string())
                    .or_default()
                    .push(format!("{} = {}", key, auto_quote_toml_value(value)));
            } else {
                log(
                    Severity::Warning,
                    &format!(
                        "Ignoring malformed config override (expected section.key=value): {}",
                        arg
                    ),
                );
            }
        }
    }

    if sections.is_empty() {
        return Ok(None);
    }

    let mut toml_str = String::new();
    for (section, kvs) in &sections {
        writeln!(toml_str, "[{}]", section).unwrap();
        for kv in kvs {
            writeln!(toml_str, "{}", kv).unwrap();
        }
    }

    let config = Config::from_str(&toml_str)?;
    Ok(Some(config))
}

pub fn config_init() -> FFResult<&'static Config> {
    assert!(CONFIG.get().is_none());

    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // First, check the args for `config=...`; this can override the config file path.
    let mut config_file_path = CONFIG_PATH.to_string();
    for i in 0..args.len() {
        let arg = args[i].trim_start_matches('-');
        if let Some(path) = arg.strip_prefix("config=") {
            config_file_path = path.to_string();
            // Remove this arg so it doesn't interfere with override parsing later.
            args.remove(i);
            break;
        }
    }

    let mut loaded_config = Config::load(&config_file_path)?;

    // Now parse any remaining args as config overrides.
    match get_overrides(args) {
        Ok(Some(overrides)) => {
            loaded_config.merge(overrides);
            log(
                Severity::Info,
                "Applied config overrides from command-line arguments",
            );
        }
        Ok(None) => {}
        Err(e) => {
            log(
                Severity::Warning,
                &format!("Failed to parse config overrides: {}", e),
            );
        }
    }

    CONFIG.set(loaded_config).unwrap_or_default();
    Ok(config_get())
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
