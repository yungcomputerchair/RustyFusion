#![allow(dead_code)]

use std::fmt::Debug;
use std::sync::OnceLock;
use std::time::SystemTime;

use async_trait::async_trait;

use crate::config::{config_get, GeneralConfig};
use crate::entity::Player;
use crate::error::*;
use crate::state::Account;

#[cfg(feature = "postgres")]
mod postgresql;

type Int = i32;
type BigInt = i64;
type Text = String;
type Bytes = Vec<u8>;

#[cfg(feature = "postgres")]
type DbBackend = postgresql::PostgresDatabase;

static DB: OnceLock<DbBackend> = OnceLock::new();

#[async_trait]
pub trait Database: Send + Sync + Debug {
    async fn find_account_from_username(&self, username: &Text) -> FFResult<Option<Account>>;
    async fn find_account_from_player(&self, pc_uid: BigInt) -> FFResult<Option<Account>>;
    async fn create_account(&self, username: &Text, password_hashed: &Text) -> FFResult<Account>;
    async fn change_account_level(&self, acc_id: BigInt, new_level: Int) -> FFResult<()>;
    async fn ban_account(
        &self,
        acc_id: BigInt,
        banned_until: SystemTime,
        ban_reason: Text,
    ) -> FFResult<()>;
    async fn unban_account(&self, acc_id: BigInt) -> FFResult<()>;
    async fn init_player(&self, acc_id: BigInt, player: &Player) -> FFResult<()>;
    async fn update_player_appearance(&self, player: &Player) -> FFResult<()>;
    async fn update_selected_player(&self, acc_id: BigInt, slot_num: Int) -> FFResult<()>;
    async fn save_player(&self, player: &Player) -> FFResult<()>;
    async fn save_players(&self, players: &[&Player]) -> FFResult<()>;
    async fn load_player(&self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Option<Player>>;
    async fn load_players(&self, acc_id: BigInt) -> FFResult<Vec<Player>>;
    async fn delete_player(&self, pc_uid: BigInt) -> FFResult<()>;
}

const DB_NAME: &str = "rustyfusion";

async fn db_connect(config: &GeneralConfig) -> FFResult<DbBackend> {
    let _db_impl: Option<FFResult<DbBackend>> = None;

    #[cfg(feature = "postgres")]
    let _db_impl = Some(postgresql::PostgresDatabase::connect(config).await);

    match _db_impl {
        Some(Ok(db)) => Ok(db),
        Some(Err(e)) => Err(FFError::build(
            Severity::Fatal,
            "Failed to connect to database".to_string(),
        )
        .with_parent(e)),
        None => Err(FFError::build(
            Severity::Fatal,
            "No database implementation enabled; please enable one through a feature".to_string(),
        )),
    }
}

pub async fn db_init() -> FFResult<&'static DbBackend> {
    if DB.get().is_some() {
        return Err(FFError::build(
            Severity::Warning,
            "Database already initialized".to_string(),
        ));
    }

    log(Severity::Info, "Connecting to database...");
    let config = &config_get().general;
    let db_impl = db_connect(config).await?;

    log(
        Severity::Info,
        &format!(
            "Connected to database ({}@{}:{})",
            config.db_username.get(),
            config.db_host.get(),
            config.db_port.get()
        ),
    );

    let _ = DB.set(db_impl);
    Ok(db_get())
}

pub fn db_get() -> &'static DbBackend {
    DB.get().expect("Database not initialized")
}
