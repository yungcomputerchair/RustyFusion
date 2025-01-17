use super::*;

use crate::defines::CN_ACCOUNT_LEVEL__USER;

define_setting!(LogPathSetting, String, "login.log");
define_setting!(ListenAddrSetting, String, "127.0.0.1:23000");
define_setting!(AutoCreateAccountsSetting, bool, true);
define_setting!(DefaultAccountLevelSetting, u32, CN_ACCOUNT_LEVEL__USER);
define_setting!(AutoApproveCustomNamesSetting, bool, true);
define_setting!(MotdPathSetting, String, "motd.txt");
define_setting!(MonitorEnabledSetting, bool, false);
define_setting!(MonitorAddrSetting, String, "127.0.0.1:8003");
define_setting!(MonitorIntervalSetting, u64, 5_u64);

#[derive(Deserialize, Default)]
pub struct LoginConfig {
    pub log_path: LogPathSetting,
    pub listen_addr: ListenAddrSetting,
    pub auto_create_accounts: AutoCreateAccountsSetting,
    pub default_account_level: DefaultAccountLevelSetting,
    pub auto_approve_custom_names: AutoApproveCustomNamesSetting,
    pub motd_path: MotdPathSetting,
    pub monitor_enabled: MonitorEnabledSetting,
    pub monitor_addr: MonitorAddrSetting,
    pub monitor_interval: MonitorIntervalSetting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Validate that every setting is defined with default value in config.toml.default
    fn test_defaults() {
        let config = Config::load("config.toml.default").unwrap();
        let login = config.login;
        assert!(login.log_path.is_set_to_default());
        assert!(login.listen_addr.is_set_to_default());
        assert!(login.auto_create_accounts.is_set_to_default());
        assert!(login.default_account_level.is_set_to_default());
        assert!(login.auto_approve_custom_names.is_set_to_default());
        assert!(login.motd_path.is_set_to_default());
        assert!(login.monitor_enabled.is_set_to_default());
        assert!(login.monitor_addr.is_set_to_default());
        assert!(login.monitor_interval.is_set_to_default());
    }
}
