use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use postgres::{tls, Client};

use crate::{
    config::config_get,
    error::{log, Severity},
};

pub struct Database {
    client: Option<Client>,
}
impl Database {
    pub fn get(&mut self) -> &mut Client {
        self.client.as_mut().expect("Database not initialized")
    }
}

static DATABASE: Mutex<Database> = Mutex::new(Database { client: None });

pub fn db_init() -> MutexGuard<'static, Database> {
    const DB_NAME: &str = "rustyfusion";

    let config = &config_get().general;
    let mut db_config = Client::configure();
    db_config
        .host(&config.db_host.get())
        .port(config.db_port.get())
        .user(&config.db_username.get())
        .password(config.db_password.get())
        .dbname(DB_NAME)
        .connect_timeout(Duration::from_secs(5));
    let db_client = db_config.connect(tls::NoTls);
    if let Err(e) = db_client {
        log(
            Severity::Fatal,
            &format!("Couldn't connect to database: {}", e),
        );
        panic!();
    }
    let db = Database {
        client: Some(db_client.unwrap()),
    };
    *DATABASE.lock().unwrap() = db;
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

pub fn db_get() -> MutexGuard<'static, Database> {
    DATABASE.lock().unwrap()
}
