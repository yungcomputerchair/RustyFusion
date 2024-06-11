#![allow(dead_code)]

use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use crate::config::*;
use crate::entity::Player;
use crate::error::*;
use crate::state::{Account, FFReceiver, FFSender};

#[cfg(feature = "postgres")]
mod postgresql;

#[cfg(feature = "mongo")]
mod mongo;

type Int = i32;
type BigInt = i64;
type Text = String;
type Bytes = Vec<u8>;

pub struct DbResult {
    result: FFResult<Box<dyn Any>>,
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
type DbOperation = dyn FnOnce(&mut dyn Database);

struct DbManager {
    db_impl: Box<dyn Database>,
    op_queue: VecDeque<Box<DbOperation>>,
    shutdown: bool,
}
unsafe impl Send for DbManager {}
impl DbManager {
    fn flush(&mut self) {
        self.op_queue.drain(..).for_each(|op| {
            op(&mut *self.db_impl);
        });
    }
}

pub trait Database: Send + std::fmt::Debug {
    fn find_account_from_username(&mut self, username: &Text) -> FFResult<Option<Account>>;
    fn find_account_from_player(&mut self, pc_uid: BigInt) -> FFResult<Account>;
    fn create_account(&mut self, username: &Text, password_hashed: &Text) -> FFResult<Account>;
    fn change_account_level(&mut self, acc_id: BigInt, new_level: Int) -> FFResult<()>;
    fn ban_account(
        &mut self,
        acc_id: BigInt,
        banned_until: SystemTime,
        ban_reason: Text,
    ) -> FFResult<()>;
    fn unban_account(&mut self, acc_id: BigInt) -> FFResult<()>;
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
static DB_MANAGER: OnceLock<Mutex<DbManager>> = OnceLock::new();

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

pub fn db_init() -> JoinHandle<()> {
    match DB_MANAGER.get() {
        Some(_) => panic_log("Database already initialized"),
        None => {
            log(Severity::Info, "Connecting to database...");
            let config = &config_get().general;
            let db_impl = panic_if_failed(db_connect(config));
            let _ = DB_MANAGER.set(Mutex::new(DbManager {
                db_impl,
                op_queue: VecDeque::new(),
                shutdown: false,
            }));
            log(
                Severity::Info,
                &format!(
                    "Connected to database ({}@{}:{})",
                    config.db_username.get(),
                    config.db_host.get(),
                    config.db_port.get()
                ),
            );

            std::thread::spawn(|| loop {
                std::thread::sleep(Duration::from_millis(100));
                let mut db_manager = DB_MANAGER.get().unwrap().lock().unwrap();
                db_manager.flush();
                if db_manager.shutdown {
                    break;
                }
            })
        }
    }
}

pub fn db_shutdown() {
    let mut db_manager = DB_MANAGER.get().unwrap().lock().unwrap();
    db_manager.shutdown = true;
}

// TODO migrate most DB operations to async
pub fn db_run_sync<T, F>(f: F) -> FFResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut dyn Database) -> FFResult<T> + Send + 'static,
{
    const TIMEOUT: Duration = Duration::from_secs(5);
    let rx = db_run_async(f);
    rx.recv(Some(TIMEOUT)).and_then(|res| res.get())
}

pub fn db_run_async<T, F>(f: F) -> FFReceiver<DbResult>
where
    T: Send + Any,
    F: FnOnce(&mut dyn Database) -> FFResult<T> + Send + 'static,
{
    match DB_MANAGER.get() {
        Some(db_mgr_lock) => {
            let mut db_mgr = db_mgr_lock.lock().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let start_time = SystemTime::now();
            let f = move |db: &mut dyn Database| {
                let result = f(db).map(|v| Box::new(v) as Box<dyn Any>);
                let db_result = DbResult {
                    result,
                    completed: SystemTime::now(),
                };
                let _ = FFSender::new(tx).send(db_result);
            };
            db_mgr.op_queue.push_back(Box::new(f));
            FFReceiver::new(start_time, rx)
        }
        None => panic_log("Database not initialized"),
    }
}
