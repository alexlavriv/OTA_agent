pub use mockall::*;
use std::fmt;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, PartialEq)]
pub enum AuthError {
    NetworkError(String),
    LicenseError(String),
    DecodingError(String),
    NotFoundError(String)
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AuthError::NetworkError(error) => write!(f, "NetworkError: {error}"),
            AuthError::LicenseError(error) => write!(f, "LicenseError: {error}"),
            AuthError::DecodingError(error) => write!(f, "DecodingError: {error}"),
            AuthError::NotFoundError(error) => write!(f, "NotFoundError: {error}"),
        }
    }
}

#[automock]
pub trait LicenseManagerTrait {
    fn get_name(&self) -> Result<String, String>;
    fn get_server(&self) -> Result<String, String>;
    fn get_token(&self) -> Result<String, String>;
    fn get_path(&self) -> Result<PathBuf, String>;
    fn get_url(&self) -> Result<Url, String>;
    fn read_license(&mut self) -> Result<(), AuthError>;
}
