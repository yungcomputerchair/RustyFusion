#![allow(dead_code)]

use std::sync::mpsc::Receiver;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

use crate::config::*;
use crate::entity::Player;
use crate::error::*;
use crate::state::Account;

#[cfg(feature = "postgres")]
mod postgresql;

#[cfg(feature = "mongo")]
mod mongo;

type Int = i32;
type BigInt = i64;
type Text = String;
type Bytes = Vec<u8>;

pub trait Database: Send + std::fmt::Debug {
    fn find_account(&mut self, username: &Text) -> FFResult<Option<Account>>;
    fn create_account(&mut self, username: &Text, password_hashed: &Text) -> FFResult<Account>;
    fn init_player(&mut self, acc_id: BigInt, player: &Player) -> FFResult<()>;
    fn update_player_appearance(&mut self, player: &Player) -> FFResult<()>;
    fn update_selected_player(&mut self, acc_id: BigInt, slot_num: Int) -> FFResult<()>;
    fn save_player(&mut self, player: &Player, state_time: Option<SystemTime>) -> FFResult<()>;
    fn save_players(&mut self, players: &[&Player], state_time: Option<SystemTime>)
        -> FFResult<()>;
    fn load_player(&mut self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Player>;
    fn load_players(&mut self, acc_id: BigInt) -> FFResult<Vec<Player>>;
    fn delete_player(&mut self, pc_uid: BigInt) -> FFResult<()>;
}

const DB_NAME: &str = "rustyfusion";
static DATABASE: OnceLock<Mutex<Box<dyn Database>>> = OnceLock::new();

fn db_connect(config: &GeneralConfig) -> FFResult<Box<dyn Database>> {
    let _db_impl: Option<FFResult<Box<dyn Database>>> = None;

    #[cfg(feature = "postgres")]
    let _db_impl = Some(postgresql::PostgresDatabase::connect(config));

    #[cfg(feature = "mongo")]
    let _db_impl = Some(mongo::MongoDatabase::connect(config));

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

pub fn db_init() -> MutexGuard<'static, Box<dyn Database>> {
    match DATABASE.get() {
        Some(_) => panic_log("Database already initialized"),
        None => {
            log(Severity::Info, "Connecting to database...");
            let config = &config_get().general;
            let db = panic_if_failed(db_connect(config));
            DATABASE.set(Mutex::new(db)).unwrap();
            log(
                Severity::Info,
                &format!(
                    "Connected to database ({}@{}:{})",
                    config.db_username.get(),
                    config.db_host.get(),
                    config.db_port.get()
                ),
            );
            db_get()
        }
    }
}

pub fn db_get() -> MutexGuard<'static, Box<dyn Database>> {
    match DATABASE.get() {
        Some(db) => db.lock().unwrap(),
        None => panic_log("Database not initialized"),
    }
}

pub fn db_run_parallel<T, F>(f: F) -> FFResult<Receiver<FFResult<T>>>
where
    T: Send + 'static,
    F: FnOnce(&mut dyn Database) -> FFResult<T> + Send + 'static,
{
    let mut db = db_connect(&config_get().general)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(f(db.as_mut())));
    Ok(rx)
}
