use super::*;

define_setting!(LoggingLevelSetting, usize, 2_usize);
define_setting!(LogWriteIntervalSetting, u64, 60_u64);
define_setting!(LiveCheckTimeSetting, u64, 60_u64);
define_setting!(ServerKeySetting, String, "myserverkey".to_string());
define_setting!(DbUsernameSetting, String, "admin".to_string());
define_setting!(DbPasswordSetting, String, "mypassword".to_string());
define_setting!(DbHostSetting, String, "localhost".to_string());
define_setting!(DbPortSetting, u16, 5432_u16);
define_setting!(TableDataPathSetting, String, "tabledata/".to_string());

#[derive(Deserialize, Default)]
pub struct GeneralConfig {
    pub logging_level_console: LoggingLevelSetting,
    pub logging_level_file: LoggingLevelSetting,
    pub log_write_interval: LogWriteIntervalSetting,
    pub live_check_time: LiveCheckTimeSetting,
    pub server_key: ServerKeySetting,
    pub db_username: DbUsernameSetting,
    pub db_password: DbPasswordSetting,
    pub db_host: DbHostSetting,
    pub db_port: DbPortSetting,
    pub table_data_path: TableDataPathSetting,
}
