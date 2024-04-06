use crate::utils::color::Coloralex;
use std::fmt;

#[derive(PartialEq, Debug, Clone)]
pub enum OTAErrorSeverity {
    NonFatalError,
    FatalError,
}

#[derive(Clone)]
pub struct OTAError {
    pub severity: OTAErrorSeverity,
    pub message: String,
}

impl OTAError {
    pub fn nonfatal(message: String) -> OTAError {
        OTAError {
            severity: OTAErrorSeverity::NonFatalError,
            message,
        }
    }

    pub fn fatal(message: String) -> OTAError {
        OTAError {
            severity: OTAErrorSeverity::FatalError,
            message,
        }
    }
    pub fn message(&self) -> String{
        self.message.clone()
    }
}

impl fmt::Display for OTAError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.severity {
            OTAErrorSeverity::NonFatalError => write!(f, "{}", self.message.red(true)),
            OTAErrorSeverity::FatalError => {
                write!(f, "{} {}", "FATAL:".red(true), self.message.red(true))
            }
        }
    }
}

impl fmt::Debug for OTAError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("OTAError")
            .field("severity", &self.severity)
            .field("message", &self.message)
            .finish()
    }
}

impl From<String> for OTAError {
    fn from(message: String) -> OTAError {
        OTAError {
            severity: OTAErrorSeverity::NonFatalError,
            message,
        }
    }
}
