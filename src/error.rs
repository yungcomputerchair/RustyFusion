use std::{
    cmp::min,
    fmt::Display,
    fs::File,
    io::{BufWriter, Write},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use crate::{config::config_get, net::ffserver::FFServer, state::ServerState};

pub type FFResult<T> = std::result::Result<T, FFError>;
pub fn catch_fail<T>(
    result: FFResult<T>,
    mut on_fail: impl FnMut() -> FFResult<()>,
) -> FFResult<T> {
    if let Err(e) = &result {
        let fail_result = on_fail();
        if let Err(ee) = fail_result {
            return Err(ee.chain(e.clone()));
        }
    }
    result
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub enum Severity {
    Debug = 3,
    Info = 2,
    Warning = 1,
    Fatal = 0,
}
impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Severity::Debug => "DEBUG",
            Severity::Info => "INFO",
            Severity::Warning => "WARN",
            Severity::Fatal => "FATAL",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub struct FFError {
    severity: Severity,
    msg: String,
    should_dc: bool,
}
impl FFError {
    fn new(severity: Severity, msg: String, should_dc: bool) -> Self {
        Self {
            severity,
            msg,
            should_dc,
        }
    }

    pub fn build(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, false)
    }

    pub fn build_dc(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, true)
    }

    pub fn from_io_err(error: std::io::Error) -> Self {
        Self {
            severity: match error.kind() {
                std::io::ErrorKind::UnexpectedEof => Severity::Debug,
                _ => Severity::Warning,
            },
            msg: format!("I/O error ({:?})", error.kind()),
            should_dc: true,
        }
    }

    pub fn from_enum_err<T: std::fmt::Debug>(val: T) -> Self {
        Self {
            severity: Severity::Warning,
            msg: format!("Enum error ({:?})", val),
            should_dc: true,
        }
    }

    pub fn chain(self, other: FFError) -> Self {
        Self {
            severity: min(self.severity, other.severity),
            msg: format!("{}\nfrom [{}] {}", self.msg, other.severity, other.msg),
            should_dc: self.should_dc || other.should_dc,
        }
    }

    pub fn get_severity(&self) -> Severity {
        self.severity
    }

    pub fn get_msg(&self) -> &str {
        &self.msg
    }

    pub fn should_dc(&self) -> bool {
        self.should_dc
    }
}

static LOGGER: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

pub fn logger_init(log_path: String) {
    assert!(LOGGER.get().is_none());
    if log_path.is_empty() {
        return;
    }

    let log_create = File::create(log_path.clone());
    match log_create {
        Ok(log_file) => {
            let logger = BufWriter::new(log_file);
            LOGGER.set(Mutex::new(logger)).unwrap();
        }
        Err(e) => {
            log(
                Severity::Warning,
                &format!("Couldn't create log file {}: {}", log_path, e),
            );
        }
    }
}

pub fn logger_flush() -> std::io::Result<()> {
    if let Some(logger) = LOGGER.get() {
        let mut logger = logger.lock().unwrap();
        logger.flush()
    } else {
        Ok(())
    }
}

pub fn logger_flush_scheduled(
    _: SystemTime,
    _: &mut FFServer,
    _: &mut ServerState,
) -> FFResult<()> {
    logger_flush().map_err(FFError::from_io_err)
}

pub fn log(severity: Severity, msg: &str) {
    let val = severity as usize;
    let threshold = config_get().general.logging_level.get();

    if val > threshold {
        return;
    }

    let s = format!("[{}] {}", severity, msg);
    println!("{}", s);
    if let Some(logger) = LOGGER.get() {
        if writeln!(logger.lock().unwrap(), "{}", s).is_err() {
            println!("Couldn't write to log file!");
        }
    }
}
