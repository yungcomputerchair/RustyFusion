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
    should_dc_client: bool,
}
impl FFError {
    fn new(severity: Severity, msg: String, should_dc_client: bool) -> Self {
        Self {
            severity,
            msg,
            should_dc_client,
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
            should_dc_client: true,
        }
    }

    pub fn get_severity(&self) -> Severity {
        self.severity
    }

    pub fn get_msg(&self) -> &str {
        &self.msg
    }

    pub fn should_dc_client(&self) -> bool {
        self.should_dc_client
    }
}

pub fn log(severity: Severity, msg: &str) {
    let s = format!("[{}] {}", severity, msg);
    println!("{}", s);
}
