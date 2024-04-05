use super::*;

use crate::defines::CN_ACCOUNT_LEVEL__USER;

define_setting!(LogPathSetting, String, "login.log");
define_setting!(ListenAddrSetting, String, "127.0.0.1:23000");
define_setting!(AutoCreateAccountsSetting, bool, true);
define_setting!(DefaultAccountLevelSetting, u32, CN_ACCOUNT_LEVEL__USER);
define_setting!(MotdPathSetting, String, "motd.txt");

#[derive(Deserialize, Default)]
pub struct LoginConfig {
    pub log_path: LogPathSetting,
    pub listen_addr: ListenAddrSetting,
    pub auto_create_accounts: AutoCreateAccountsSetting,
    pub default_account_level: DefaultAccountLevelSetting,
    pub motd_path: MotdPathSetting,
}
