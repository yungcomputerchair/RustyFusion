#![allow(dead_code)]

use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use crate::config::*;
use crate::entity::Player;
use crate::error::*;
use crate::state::Account;

#[cfg(feature = "postgres")]
mod postgresql;

type Int = i32;
type BigInt = i64;
type Text = String;
type Bytes = Vec<u8>;

pub struct DbResult {
    result: FFResult<Box<dyn Any + Send>>,
    pub completed: SystemTime,
}
impl DbResult {
    pub fn get<T: 'static>(self) -> FFResult<T> {
        self.result.map(|v| {
            *v.downcast::<T>()
                .unwrap_or_else(|_| panic_log("Bad DbResult cast"))
        })
    }
}
type DbOperation = Box<
    dyn for<'a> FnOnce(&'a mut dyn Database) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send,
>;

static DB_TX: OnceLock<std::sync::Mutex<Option<mpsc::UnboundedSender<DbOperation>>>> =
    OnceLock::new();

#[async_trait]
pub trait Database: Send + std::fmt::Debug {
    async fn find_account_from_username(&mut self, username: &Text) -> FFResult<Option<Account>>;
    async fn find_account_from_player(&mut self, pc_uid: BigInt) -> FFResult<Account>;
    async fn create_account(
        &mut self,
        username: &Text,
        password_hashed: &Text,
    ) -> FFResult<Account>;
    async fn change_account_level(&mut self, acc_id: BigInt, new_level: Int) -> FFResult<()>;
    async fn ban_account(
        &mut self,
        acc_id: BigInt,
        banned_until: SystemTime,
        ban_reason: Text,
    ) -> FFResult<()>;
    async fn unban_account(&mut self, acc_id: BigInt) -> FFResult<()>;
    async fn init_player(&mut self, acc_id: BigInt, player: &Player) -> FFResult<()>;
    async fn update_player_appearance(&mut self, player: &Player) -> FFResult<()>;
    async fn update_selected_player(&mut self, acc_id: BigInt, slot_num: Int) -> FFResult<()>;
    async fn save_player(&mut self, player: &Player) -> FFResult<()>;
    async fn save_players(&mut self, players: &[&Player]) -> FFResult<()>;
    async fn load_player(&mut self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Player>;
    async fn load_players(&mut self, acc_id: BigInt) -> FFResult<Vec<Player>>;
    async fn delete_player(&mut self, pc_uid: BigInt) -> FFResult<()>;
}

const DB_NAME: &str = "rustyfusion";

async fn db_connect(config: &GeneralConfig) -> FFResult<Box<dyn Database>> {
    let _db_impl: Option<FFResult<Box<dyn Database>>> = None;

    #[cfg(feature = "postgres")]
    let _db_impl = Some(postgresql::PostgresDatabase::connect(config).await);

    match _db_impl {
        Some(Ok(db)) => Ok(db),
        Some(Err(e)) => Err(FFError::build(
            Severity::Fatal,
            format!("Failed to connect to database: {}", e.get_msg()),
        )),
        None => Err(FFError::build(
            Severity::Fatal,
            "No database implementation enabled; please enable one through a feature".to_string(),
        )),
    }
}

pub async fn db_init() {
    if DB_TX.get().is_some() {
        panic_log("Database already initialized");
    }

    log(Severity::Info, "Connecting to database...");
    let config = &config_get().general;
    let db_impl = panic_if_failed(db_connect(config).await);
    log(
        Severity::Info,
        &format!(
            "Connected to database ({}@{}:{})",
            config.db_username.get(),
            config.db_host.get(),
            config.db_port.get()
        ),
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<DbOperation>();
    let _ = DB_TX.set(std::sync::Mutex::new(Some(tx)));

    tokio::spawn(async move {
        let mut db = db_impl;
        while let Some(op) = rx.recv().await {
            op(&mut *db).await;
        }
    });
}

pub fn db_shutdown() {
    // Drop the sender to close the channel, which ends the spawned task
    if let Some(lock) = DB_TX.get() {
        lock.lock().unwrap().take();
    }
}

pub fn _db_run_sync<T, F>(f: F) -> FFResult<T>
where
    T: Send + 'static,
    F: for<'a> FnOnce(
            &'a mut dyn Database,
        ) -> Pin<Box<dyn Future<Output = FFResult<T>> + Send + 'a>>
        + Send
        + 'static,
{
    const TIMEOUT: Duration = Duration::from_secs(5);
    let rx = _db_run_async(f);
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            tokio::time::timeout(TIMEOUT, rx)
                .await
                .map_err(|_| {
                    FFError::build(
                        Severity::Warning,
                        "DB operation failed: timed out".to_string(),
                    )
                })?
                .map_err(|_| {
                    FFError::build(
                        Severity::Warning,
                        "DB operation failed: sender dropped".to_string(),
                    )
                })
                .and_then(|res| res.get())
        })
    })
}

pub fn _db_run_async<T, F>(f: F) -> oneshot::Receiver<DbResult>
where
    T: Send + Any,
    F: for<'a> FnOnce(
            &'a mut dyn Database,
        ) -> Pin<Box<dyn Future<Output = FFResult<T>> + Send + 'a>>
        + Send
        + 'static,
{
    let lock = DB_TX
        .get()
        .unwrap_or_else(|| panic_log("Database not initialized"));
    let guard = lock.lock().unwrap();
    let tx = guard
        .as_ref()
        .unwrap_or_else(|| panic_log("Database has been shut down"));
    let (result_tx, result_rx) = oneshot::channel();
    let op: DbOperation = Box::new(move |db: &mut dyn Database| {
        Box::pin(async move {
            let result = f(db).await.map(|v| Box::new(v) as Box<dyn Any + Send>);
            let db_result = DbResult {
                result,
                completed: SystemTime::now(),
            };
            let _ = result_tx.send(db_result);
        })
    });
    let _ = tx.send(op);
    result_rx
}
