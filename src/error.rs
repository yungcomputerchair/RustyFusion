use std::fmt::Display;

pub type FFResult<T> = std::result::Result<T, FFError>;

#[derive(Clone, Copy)]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Fatal,
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

pub struct FFError {
    severity: Severity,
    msg: String,
}
impl FFError {
    pub fn new(severity: Severity, msg: String) -> Self {
        Self { severity, msg }
    }

    pub fn from_io_err(error: std::io::Error) -> Self {
        Self {
            severity: Severity::Fatal,
            msg: format!("I/O error ({:?})", error),
        }
    }

    pub fn get_severity(&self) -> Severity {
        self.severity
    }

    pub fn get_msg(&self) -> &str {
        &self.msg
    }
}

pub fn log(severity: Severity, msg: &str) {
    let s = format!("[{}] {}", severity, msg);
    println!("{}", s);
}
