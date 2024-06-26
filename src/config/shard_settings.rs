use super::*;

define_setting!(LogPathSetting, String, "shard.log");
define_setting!(ShardIDSetting, i32, 1_i32);
define_setting!(ListenAddrSetting, String, "127.0.0.1:23001");
define_setting!(ExternalAddrSetting, String, "127.0.0.1:23001");
define_setting!(LoginServerAddrSetting, String, "127.0.0.1:23000");
define_setting!(LoginServerConnIntervalSetting, u64, 10_u64);
define_setting!(NumChannelsSetting, usize, 1_usize);
define_setting!(MaxChannelPopSetting, usize, 100_usize);
define_setting!(VisibilityRangeSetting, usize, 1_usize);
define_setting!(AutosaveIntervalSetting, u64, 5_u64);
define_setting!(NumSlidersSetting, usize, 20_usize);
define_setting!(VehicleDurationSetting, u64, 10_080_u64);

#[derive(Deserialize, Default)]
pub struct ShardConfig {
    pub log_path: LogPathSetting,
    pub shard_id: ShardIDSetting,
    pub listen_addr: ListenAddrSetting,
    pub external_addr: ExternalAddrSetting,
    pub login_server_addr: LoginServerAddrSetting,
    pub login_server_conn_interval: LoginServerConnIntervalSetting,
    pub num_channels: NumChannelsSetting,
    pub max_channel_pop: MaxChannelPopSetting,
    pub visibility_range: VisibilityRangeSetting,
    pub autosave_interval: AutosaveIntervalSetting,
    pub num_sliders: NumSlidersSetting,
    pub vehicle_duration: VehicleDurationSetting,
}
