#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::config::*;
use crate::error::*;
use crate::player::Player;

#[cfg(feature = "postgres")]
mod postgresql;

#[cfg(feature = "mongo")]
mod mongo;

type Int = i32;
type BigInt = i64;
type Text = String;
type Bytes = Vec<u8>;

pub trait Database: Send + std::fmt::Debug {
    fn find_account(&mut self, username: &Text) -> FFResult<Option<BigInt>>;
    fn create_account(&mut self, username: &Text, password_hashed: &Text) -> FFResult<BigInt>;
    fn init_player(&mut self, acc_id: BigInt, player: &Player) -> FFResult<()>;
    fn update_player_appearance(&mut self, player: &Player) -> FFResult<()>;
    fn update_selected_player(&mut self, acc_id: BigInt, slot_num: Int) -> FFResult<()>;
    fn save_player(&mut self, player: &Player) -> FFResult<()>;
    fn save_players(&mut self, players: &[&Player]) -> FFResult<()>;
    fn load_player(&mut self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Player>;
    fn load_players(&mut self, acc_id: BigInt) -> FFResult<Vec<Player>>;
    fn delete_player(&mut self, pc_uid: BigInt) -> FFResult<()>;
}

const DB_NAME: &str = "rustyfusion";
static DATABASE: OnceLock<Mutex<Box<dyn Database>>> = OnceLock::new();

pub fn db_init() -> MutexGuard<'static, Box<dyn Database>> {
    match DATABASE.get() {
        Some(_) => panic_log("Database already initialized"),
        None => {
            log(Severity::Info, "Connecting to database...");

            let config = &config_get().general;
            let _db_impl: Option<FFResult<Box<dyn Database>>> = None;

            #[cfg(feature = "postgres")]
            let _db_impl = Some(postgresql::PostgresDatabase::connect(config));

            #[cfg(feature = "mongo")]
            let _db_impl = Some(mongo::MongoDatabase::connect(config));

            let db = match _db_impl {
                Some(Ok(db)) => db,
                Some(Err(e)) => {
                    panic_log(&format!("Failed to connect to database: {}", e.get_msg()))
                }
                None => panic_log(
                    "No database implementation enabled; please enable one through a feature",
                ),
            };
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
