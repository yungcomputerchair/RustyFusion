#![allow(dead_code)]

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use crate::config::{config_get, GeneralConfig};
use crate::entity::Player;
use crate::error::*;
use crate::state::Account;

#[cfg(feature = "postgres")]
mod postgresql;

type Int = i32;
type BigInt = i64;
type Bytes = Vec<u8>;

#[cfg(feature = "postgres")]
type DbBackend = postgresql::PostgresDatabase;

static DB: OnceLock<Database<DbBackend>> = OnceLock::new();
static DB_ERROR_SEVERITY: OnceLock<Severity> = OnceLock::new();

pub fn db_error_severity() -> Severity {
    DB_ERROR_SEVERITY.get().copied().unwrap()
}

#[derive(Debug)]
pub struct Database<D> {
    inner: D,
    disconnected: AtomicBool,
}
impl<D> Database<D> {
    fn new(inner: D) -> Self {
        Self {
            inner,
            disconnected: AtomicBool::new(false),
        }
    }
}

/// Defines the `DbImpl` trait and generates a delegating implementation for `Database<D>`.
macro_rules! define_db_api {
    ($(
        $method:ident(&self $(, $arg:ident : $arg_ty:ty)*) -> $ret:ty;
    )*) => {
        #[async_trait]
        pub trait DbImpl: Send + Sync + Debug {
            $(
                async fn $method(&self $(, $arg: $arg_ty)*) -> FFResult<$ret>;
            )*
        }

        #[async_trait]
        impl<D: DbImpl> DbImpl for Database<D> {
            $(
                async fn $method(&self $(, $arg: $arg_ty)*) -> FFResult<$ret> {
                    const MAX_TRIES: usize = 5;
                    const RETRY_DELAY_BASE_MS: u64 = 250;

                    let was_disconnected = self.disconnected.load(Ordering::Acquire);
                    let tries = if was_disconnected { 1 } else { MAX_TRIES };

                    let mut last_err = None;
                    for attempt in 1..=tries {
                        match self.inner.$method($($arg),*).await {
                            Ok(result) => {
                                self.disconnected.store(false, Ordering::Release);
                                return Ok(result);
                            }
                            Err(e) => {
                                log(Severity::Warning, &format!("Database operation failed: {} (attempt {}/{})", e.get_msg(), attempt, tries));
                                if attempt == tries {
                                    last_err = Some(e);
                                    break;
                                }
                            }
                        }

                        // exponential backoff
                        let delay_ms = RETRY_DELAY_BASE_MS * 2u64.pow((attempt - 1) as u32);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }

                    if !was_disconnected {
                        self.disconnected.store(true, Ordering::Release);
                        return Err(FFError::build(
                            db_error_severity(),
                            format!("Database connection lost ({} failed attempts)", tries),
                        )
                        .with_parent(last_err.unwrap()));
                    }

                    return Err(last_err.unwrap());
                }
            )*
        }
    };
}

define_db_api! {
    find_account_from_username(&self, username: &str) -> Option<Account>;
    find_account_from_player(&self, pc_uid: BigInt) -> Option<Account>;
    create_account(&self, username: &str, password_hashed: &str) -> Account;
    change_account_level(&self, acc_id: BigInt, new_level: Int) -> ();
    ban_account(&self, acc_id: BigInt, banned_until: SystemTime, ban_reason: &str) -> ();
    unban_account(&self, acc_id: BigInt) -> ();
    init_player(&self, acc_id: BigInt, player: &Player) -> ();
    update_player_appearance(&self, player: &Player) -> ();
    update_selected_player(&self, acc_id: BigInt, slot_num: Int) -> ();
    save_player(&self, player: &Player) -> ();
    save_players(&self, players: &[&Player]) -> ();
    load_player(&self, acc_id: BigInt, pc_uid: BigInt) -> Option<Player>;
    load_players(&self, acc_id: BigInt) -> Vec<Player>;
    delete_player(&self, pc_uid: BigInt) -> ();
}

fn format_db_conn_error(parent_error: FFError) -> FFError {
    let expected_error_message = "Error occurred while creating a new object: error connecting to server";

    if let Some(parent_parent) = parent_error.get_parent() {
        if parent_parent.get_msg() == expected_error_message {
            // If it's the common case of not being able to connect to the DB,
            // just print a concise, readable error message.
            return FFError::build(
                Severity::Warning,
                "Failed to connect to database".to_string(),
            );
        }
    }

    // If it's some other DB connection error, print the pseudo-stack trace.
    FFError::build(
        Severity::Warning,
        "Unexpected error while connecting to database".to_string(),
    ).with_parent(parent_error)
}

async fn db_connect(config: &GeneralConfig) -> FFResult<DbBackend> {
    let _db_impl: Option<FFResult<DbBackend>> = None;

    #[cfg(feature = "postgres")]
    let _db_impl = Some(postgresql::PostgresDatabase::connect(config).await);

    match _db_impl {
        Some(Ok(db)) => Ok(db),
        Some(Err(parent_error)) => Err(format_db_conn_error(parent_error)),
        None => Err(FFError::build(
            Severity::Fatal,
            "No database implementation enabled; please enable one through a feature".to_string(),
        )),
    }
}

pub async fn db_init(error_severity: Severity) -> FFResult<&'static Database<DbBackend>> {
    if DB.get().is_some() {
        return Ok(db_get());
    }

    let _ = DB_ERROR_SEVERITY.set(error_severity);

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

    let _ = DB.set(Database::new(db_impl));
    Ok(db_get())
}

pub fn db_get() -> &'static Database<DbBackend> {
    DB.get().expect("Database not initialized")
}
