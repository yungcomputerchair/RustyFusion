// This file is a template read by build.rs.

#[derive(Default)]
pub struct Config {
    //CONFIG_FIELDS//
}

impl Config {
    fn load(path: &str) -> FFResult<Self> {
        let file_read = std::fs::read_to_string(path);
        if let Err(e) = file_read {
            if let std::io::ErrorKind::NotFound = e.kind() {
                log(
                    Severity::Warning,
                    &format!("Config file {} missing, using default config", path),
                );

                return Ok(Self::default());
            } else {
                return Err(FFError::build(
                    Severity::Fatal,
                    "Failed to read config file".to_string(),
                )
                .with_parent(e.into()));
            }
        }

        let file_contents = file_read.unwrap();
        Config::from_str(&file_contents)
    }

    fn from_str(toml_str: &str) -> FFResult<Self> {
        #[derive(Deserialize)]
        struct ConfigLayout {
            //LAYOUT_FIELDS//
        }

        let parsed = toml::from_str::<ConfigLayout>(toml_str).map_err(|e| {
            FFError::build(Severity::Fatal, format!("Failed to parse config: {}", e))
        })?;

        Ok(Config {
            //UNWRAP_FIELDS//
        })
    }

    fn merge(&mut self, other: Config) {
        //MERGE_FIELDS//
    }
}
