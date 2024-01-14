use super::*;

define_setting!(LogPathSetting, String, "shard.log");
define_setting!(ListenAddrSetting, String, "127.0.0.1:23001");
define_setting!(ExternalAddrSetting, String, "127.0.0.1:23001");
define_setting!(LoginServerAddrSetting, String, "127.0.0.1:23000");
define_setting!(LoginServerConnIntervalSetting, u64, 10_u64);
define_setting!(VisibilityRangeSetting, usize, 1_usize);
define_setting!(AutosaveIntervalSetting, u64, 5_u64);
define_setting!(VehicleDurationSetting, u64, 10_080_u64);

#[derive(Deserialize, Default)]
pub struct ShardConfig {
    pub log_path: LogPathSetting,
    pub listen_addr: ListenAddrSetting,
    pub external_addr: ExternalAddrSetting,
    pub login_server_addr: LoginServerAddrSetting,
    pub login_server_conn_interval: LoginServerConnIntervalSetting,
    pub visibility_range: VisibilityRangeSetting,
    pub autosave_interval: AutosaveIntervalSetting,
    pub vehicle_duration: VehicleDurationSetting,
}
