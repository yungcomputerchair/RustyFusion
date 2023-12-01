use super::*;

define_setting!(LoggingLevelSetting, usize, 2_usize);
define_setting!(LogWriteIntervalSetting, u64, 60_u64);

#[derive(Deserialize, Default)]
pub struct GeneralConfig {
    pub logging_level: LoggingLevelSetting,
    pub log_write_interval: LogWriteIntervalSetting,
}
