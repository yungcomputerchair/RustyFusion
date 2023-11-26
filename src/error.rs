use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub enum Severity {
    Info,
    Warning,
    Fatal,
}
impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Severity::Info => "INFO",
            Severity::Warning => "WARN",
            Severity::Fatal => "FATAL",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug)]
pub struct FFError {
    severity: Severity,
    msg: String,
}
impl FFError {
    pub fn build(severity: Severity, msg: String) -> Box<dyn Error> {
        Box::new(Self { severity, msg })
    }
}
impl Error for FFError {}
impl Display for FFError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.severity, self.msg)
    }
}
