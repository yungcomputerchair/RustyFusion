use std::{
    cmp::min,
    fmt::Display,
    fs::File,
    io::{BufWriter, Write},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use crate::{config::config_get, net::FFServer, state::ServerState};

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
impl Severity {
    pub fn get_label(&self, colored: bool) -> String {
        if colored {
            format!(
                "\x1b[{}m[{}]\x1b[0m",
                match self {
                    Severity::Debug => "36",
                    Severity::Info => "32",
                    Severity::Warning => "33",
                    Severity::Fatal => "31",
                },
                self
            )
        } else {
            format!("[{}]", self)
        }
    }
}

#[derive(Debug, Clone)]
pub struct FFError {
    severity: Severity,
    msg: String,
    should_dc: bool,
    parent: Option<Box<FFError>>,
}
impl FFError {
    fn new(severity: Severity, msg: String, should_dc: bool) -> Self {
        Self {
            severity,
            msg,
            should_dc,
            parent: None,
        }
    }

    pub fn build(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, false)
    }

    pub fn build_dc(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, true)
    }

    pub fn from_bcrypt_err(error: bcrypt::BcryptError) -> Self {
        Self {
            severity: Severity::Warning,
            msg: format!("BCrypt error ({:?})", error),
            should_dc: false,
            parent: None,
        }
    }

    pub fn from_io_err(error: std::io::Error) -> Self {
        Self {
            severity: match error.kind() {
                std::io::ErrorKind::UnexpectedEof => Severity::Debug,
                std::io::ErrorKind::BrokenPipe => Severity::Debug,
                _ => Severity::Warning,
            },
            msg: format!("I/O error ({:?})", error.kind()),
            should_dc: true,
            parent: None,
        }
    }

    pub fn from_enum_err<T: std::fmt::Debug>(val: T) -> Self {
        Self {
            severity: Severity::Warning,
            msg: format!("Enum error ({:?})", val),
            should_dc: true,
            parent: None,
        }
    }

    pub fn chain(self, other: FFError) -> Self {
        Self {
            parent: Some(Box::new(other)),
            ..self
        }
    }

    pub fn get_severity(&self) -> Severity {
        let mut sev = self.severity;
        if let Some(parent) = self.parent.as_ref() {
            // Recursively get the lowest value severity,
            // which is the most severe
            sev = min(sev, parent.get_severity());
        }
        sev
    }

    pub fn get_msg(&self) -> &str {
        &self.msg
    }

    pub fn should_dc(&self) -> bool {
        // Any DC error in the chain should cause a DC.
        // Recursive short-circuiting.
        match self.should_dc {
            true => true,
            false => match self.parent.as_ref() {
                Some(parent) => parent.should_dc(),
                None => false,
            },
        }
    }

    pub fn get_formatted(&self, colored: bool) -> String {
        let mut msg = format!("{} {}", self.severity.get_label(colored), self.msg);
        if let Some(parent) = self.parent.as_ref() {
            msg.push_str(&format!("\nfrom {}", parent.get_formatted(colored)));
        }
        msg
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
    let err = FFError::build(severity, msg.to_string());
    log_error(&err);
}

pub fn log_error(err: &FFError) {
    let severity = err.get_severity();

    let config = &config_get().general;
    let threshold_console = config.logging_level_console.get();
    let threshold_file = config.logging_level_file.get();

    if severity as usize <= threshold_console {
        // Log to console, colored output
        let msg = err.get_formatted(true);
        if severity == Severity::Fatal {
            // Print to stderr instead
            eprintln!("{}", msg);
        } else {
            println!("{}", msg);
        }
    }

    if severity as usize <= threshold_file {
        // Log to file
        let msg = err.get_formatted(false);
        if let Some(logger) = LOGGER.get() {
            let mut logger = logger.lock().unwrap();
            if writeln!(logger, "{}", msg).is_err() {
                println!("Couldn't write to log file!");
            }
        }
    }
}

pub fn panic_log(msg: &str) -> ! {
    let err = FFError::build(Severity::Fatal, msg.to_string());
    log_error(&err);
    panic!("A fatal error occurred, see log for details");
}

pub fn log_if_failed<T>(result: FFResult<T>) {
    if let Err(e) = result {
        log_error(&e);
    }
}

pub fn panic_if_failed<T>(result: FFResult<T>) -> T {
    if let Err(e) = &result {
        log_error(e);
        panic!("A fatal error occurred, see log for details");
    }
    result.unwrap()
}

pub mod codes {
    use super::*;
    use num_enum::TryFromPrimitive;

    #[repr(i32)]
    #[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
    #[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
    pub enum PlayerSearchReqErr {
        NotFound = 0,
        SearchInProgress = 1,
    }

    #[repr(i32)]
    #[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
    #[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
    pub enum TaskEndErr {
        Unknown = 0,
        TimeLimitExceeded = 1,
        EscortFailed = 11,
        InstanceLeft = 12,
        InventoryFull = 13,
    }
}
