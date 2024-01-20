use super::*;

define_setting!(LogPathSetting, String, "login.log");
define_setting!(ListenAddrSetting, String, "127.0.0.1:23000");
define_setting!(AutoCreateAccountsSetting, bool, true);
define_setting!(MotdPathSetting, String, "motd.txt");

#[derive(Deserialize, Default)]
pub struct LoginConfig {
    pub log_path: LogPathSetting,
    pub listen_addr: ListenAddrSetting,
    pub auto_create_accounts: AutoCreateAccountsSetting,
    pub motd_path: MotdPathSetting,
}
