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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Validate that every setting is defined with default value in config.toml.default
    fn test_defaults() {
        let config = Config::load("config.toml.default").unwrap();
        let shard = config.shard;
        assert!(shard.log_path.is_set_to_default());
        assert!(shard.shard_id.is_set_to_default());
        assert!(shard.listen_addr.is_set_to_default());
        assert!(shard.external_addr.is_set_to_default());
        assert!(shard.login_server_addr.is_set_to_default());
        assert!(shard.login_server_conn_interval.is_set_to_default());
        assert!(shard.num_channels.is_set_to_default());
        assert!(shard.max_channel_pop.is_set_to_default());
        assert!(shard.visibility_range.is_set_to_default());
        assert!(shard.autosave_interval.is_set_to_default());
        assert!(shard.num_sliders.is_set_to_default());
        assert!(shard.vehicle_duration.is_set_to_default());
    }
}
