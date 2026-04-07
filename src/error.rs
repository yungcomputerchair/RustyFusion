use std::{
    cmp::min,
    fmt::Display,
    fs::File,
    io::{BufWriter, ErrorKind, Write},
    sync::OnceLock,
    time::SystemTime,
};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
    config::config_get,
    util::{self, RingBuffer},
};

pub type FFResult<T> = std::result::Result<T, FFError>;
pub trait CatchFail<T> {
    fn catch_fail(self, on_fail: impl FnOnce()) -> Self;
}
impl<T> CatchFail<T> for FFResult<T> {
    fn catch_fail(self, on_fail: impl FnOnce()) -> Self {
        if let Err(e) = &self {
            log_error(e.clone());
            on_fail();
        }
        self
    }
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
    timestamp: SystemTime,
    parent: Option<Box<FFError>>,
}
impl std::error::Error for FFError {}
impl Display for FFError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_formatted(false, false))
    }
}
impl From<std::io::Error> for FFError {
    fn from(error: std::io::Error) -> Self {
        let severity = match error.kind() {
            ErrorKind::UnexpectedEof => Severity::Debug,
            ErrorKind::BrokenPipe => Severity::Debug,
            ErrorKind::ConnectionReset => Severity::Debug,
            ErrorKind::ConnectionAborted => Severity::Debug,
            ErrorKind::WouldBlock => Severity::Debug,
            _ => Severity::Warning,
        };
        let should_dc =
            error.kind() != ErrorKind::WouldBlock && error.kind() != ErrorKind::TimedOut;
        Self::new(
            severity,
            format!("I/O error ({:?})", error.kind()),
            should_dc,
        )
    }
}
impl From<bcrypt::BcryptError> for FFError {
    fn from(error: bcrypt::BcryptError) -> Self {
        Self::new(
            Severity::Warning,
            format!("BCrypt error ({:?})", error),
            false,
        )
    }
}
impl FFError {
    fn new(severity: Severity, msg: String, should_dc: bool) -> Self {
        Self {
            severity,
            msg,
            should_dc,
            timestamp: SystemTime::now(),
            parent: None,
        }
    }

    pub fn build(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, false)
    }

    pub fn build_dc(severity: Severity, msg: String) -> Self {
        Self::new(severity, msg, true)
    }

    pub fn from_enum_err<T: std::fmt::Debug>(val: T) -> Self {
        Self::new(Severity::Warning, format!("Enum error ({:?})", val), true)
    }

    pub fn with_parent(self, other: FFError) -> Self {
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

    pub fn get_timestamp(&self) -> SystemTime {
        self.timestamp
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

    pub fn get_formatted(&self, colored: bool, with_time: bool) -> String {
        let mut msg = if with_time {
            let time_str = util::get_timestamp_str(self.timestamp);
            format!(
                "[{}] {} {}",
                time_str,
                self.severity.get_label(colored),
                self.msg
            )
        } else {
            format!("{} {}", self.severity.get_label(colored), self.msg)
        };
        if let Some(parent) = self.parent.as_ref() {
            msg.push_str(&format!(
                "\n\tfrom: {}",
                parent.get_formatted(colored, with_time)
            ));
        }
        msg
    }
}

static LOG_TX: OnceLock<UnboundedSender<FFError>> = OnceLock::new();
const LOG_BUFFER_SIZE: usize = 1000;

pub fn log_init() -> UnboundedReceiver<FFError> {
    assert!(LOG_TX.get().is_none());
    let (tx, rx) = mpsc::unbounded_channel();
    LOG_TX.set(tx).unwrap();
    rx
}

pub fn log(severity: Severity, msg: &str) {
    let err = FFError::build(severity, msg.to_string());
    log_error(err);
}

pub fn log_error(err: FFError) {
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.send(err);
    } else {
        // Before log_init, fall back to stdout
        let msg = err.get_formatted(true, true);
        println!("{}", msg);
    }
}

pub struct Logger {
    rx: UnboundedReceiver<FFError>,
    buffer: RingBuffer<FFError>,
    file_writer: Option<BufWriter<File>>,
}
impl Logger {
    pub fn new(rx: UnboundedReceiver<FFError>, log_path: &str) -> Self {
        let file_writer = if log_path.is_empty() {
            None
        } else {
            match File::create(log_path) {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    log(
                        Severity::Warning,
                        &format!("Couldn't create log file {}: {}", log_path, e),
                    );
                    None
                }
            }
        };
        Self {
            rx,
            buffer: RingBuffer::new(LOG_BUFFER_SIZE),
            file_writer,
        }
    }

    pub fn drain(&mut self) {
        let config = &config_get().general;
        let threshold_console = config.logging_level_console.get();
        let threshold_file = config.logging_level_file.get();
        while let Ok(err) = self.rx.try_recv() {
            let severity = err.get_severity();

            // queue for file writing
            if severity as usize <= threshold_file {
                if let Some(writer) = &mut self.file_writer {
                    let msg = err.get_formatted(false, true);
                    let _ = writeln!(writer, "{}", msg);
                }
            }

            // store in console buffer
            if severity as usize <= threshold_console {
                self.buffer.push(err);
            }
        }
    }

    pub fn flush(&mut self) {
        if let Some(writer) = &mut self.file_writer {
            let _ = writer.flush();
        }
    }

    pub fn buffer(&self) -> &RingBuffer<FFError> {
        &self.buffer
    }
}
impl Drop for Logger {
    fn drop(&mut self) {
        self.drain();
        self.flush();
    }
}

pub fn panic_log(msg: &str) -> ! {
    let err = FFError::build(Severity::Fatal, msg.to_string());
    log_error(err);
    panic!("A fatal error occurred, see log for details");
}

pub fn log_if_failed<T>(result: FFResult<T>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            log_error(e);
            None
        }
    }
}

pub fn panic_if_failed<T>(result: FFResult<T>) -> T {
    if let Err(e) = result {
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
    pub enum LoginError {
        DbConnectionError = 0,     // "DB connection error"
        UsernameNotFound = 1, // "Sorry, the ID you have entered does not exist. Please try again."
        IncorrectPassword = 2, // "Sorry, the ID and Password you have entered do not match. Please try again."
        AlreadyLoggedIn = 3,   // "ID already in use. Disconnect existing connection?"
        LoginError = 4,        // "Login error"
        LoginError2 = 5,       // "Login error"
        ClientVersionOutdated = 6, // "Client version outdated"
        UnauthorizedForBeta = 7, // "You are not an authorized beta tester"
        AuthServicesError = 8, // "Authentication connection error"
        EulaNotAccepted = 9,   // "Updated EUALA acceptance required"
    }

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
        NotComplete = 0,
        TimeLimitExceeded = 1,
        EscortFailed = 11,
        InstanceLeft = 12,
        InventoryFull = 13,
    }

    #[repr(i32)]
    #[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
    #[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
    pub enum BuddyWarpErr {
        CantWarpToLocation = 3,
        RechargeNotComplete = 6,
    }
}
