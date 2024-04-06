use crate::auth::license_manager_trait::{AuthError, LicenseManagerTrait};
use crate::auth::license_manager::LicenseManager;
use crate::utils::color::Coloralex;
use crate::utils::file_utils::file_to_string;
use crate::utils::file_utils::string_to_file;

use std::path::PathBuf;
use std::str::FromStr;
use std::string::ToString;
use url::Url;

use serde::Serialize;
use serde_json::Value;

pub const AUTH_FILE: &str = "auth";

#[derive(Serialize)]
pub struct AuthObject {
    pub url: String,
    pub token: String,
    pub version: String,
}

pub struct AuthManager {
    path: PathBuf,
    token: String,
    url: Url,
    version: String,
}

impl LicenseManagerTrait for AuthManager {
    fn get_name(&self) -> Result<String, String> {
       Ok(self.version.to_string())
    }
    fn get_server(&self) -> Result<String, String> {
        Ok(self.url.to_string())
    }
    fn get_token(&self) -> Result<String, String> { Ok(self.token.clone()) }
    fn get_path(&self) -> Result<PathBuf, String> { Ok(self.path.clone()) }
    fn get_url(&self) -> Result<Url, String> { Ok(self.url.clone()) }
    fn read_license(&mut self) -> Result<(), AuthError> {
        let auth = Self::get_auth_from_file(self.path.clone()).map_err(AuthError::LicenseError)?;
        self.update_from_value(auth).map_err(AuthError::LicenseError)
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::from_path(Self::auth_path())
    }
}

impl AuthManager {
    pub fn new(path: PathBuf, token: String, url: Url, version: String) -> Self {
        AuthManager { path, token, url, version }
    }

    #[cfg(unix)]
    pub fn auth_path() -> PathBuf {
        PathBuf::from(format!("./{}", AUTH_FILE))
    }

    #[cfg(windows)]
    #[cfg(test)]
    pub fn auth_path() -> PathBuf {
        PathBuf::from(format!("./{}", AUTH_FILE))
    }

    #[cfg(windows)]
    #[cfg(not(test))]
    pub fn auth_path() -> PathBuf {
        use std::{env, fs};
        use crate::utils::file_utils;
        let localappdata = env::var("ProgramData").unwrap();
        let localdir = PathBuf::from(localappdata).join("Phantom_Agent");
        if !localdir.exists() {
            if fs::create_dir(&localdir).is_err() {
                log::error!("Failed to create Auth Path dir!");
            }
            if let Err(error) = LicenseManager::set_permissions(&localdir){
                log::error!("Setting permissions to {} failed: {}", &localdir.to_string_lossy(), error);
            }
        }
        let auth_path = localdir.join(AUTH_FILE);

        let old_path = PathBuf::from("./auth");
        if old_path.exists() && !auth_path.exists() {
            log::info!("Old version of auth file found, copying for backwards compatibility");
            if let Err(error) = file_utils::mv_file(&old_path, &auth_path) {
                log::error!("Failed to copy {} to {} : {}", &old_path.to_string_lossy(), &auth_path.to_string_lossy(), error);
            }
            if let Err(error) = LicenseManager::set_permissions(&auth_path){
                log::error!("Setting permissions to {} failed: {}", &localdir.to_string_lossy(), error);
            }
        }
        auth_path
    }

    pub fn from_path(path: PathBuf) -> Self {
        AuthManager {
            path,
            token: Default::default(),
            url: Url::from_str("https://invalid.com").unwrap(),
            version: Default::default()
        }
    }

    pub fn update_from_value(&mut self, auth: Value) -> Result<(), String> {
        if !Self::valid_auth(&auth) {
            return Err(format!("Cannot parse auth: [{}]", auth));
        }
        log::info!("Url: {}\nVersion: {}\nToken: {}", auth["url"], auth["version"], auth["token"]);
        self.url = LicenseManager::make_proper_url(auth["url"].as_str().unwrap());
        self.token = auth["token"].as_str().unwrap().to_string();
        self.version = auth["version"].as_str().unwrap().to_string();
        Ok(())
    }

    pub fn save_into_file(self) -> Result<(), String> {
        match serde_json::to_value(AuthObject {
            url: self.url.to_string(),
            token: self.token,
            version: self.version,
        }) {
            Ok(auth) => { Self::save_auth_values(&auth) }
            Err(_) => { Err("Cannot create AuthObject".to_string()) }
        }
    }

    fn get_auth_from_file(auth_file: PathBuf) -> Result<Value, String> {
        if auth_file.exists() {
            match file_to_string(&auth_file) {
                Ok(text) => {
                    match serde_json::from_str(&text) {
                        Ok(value) => {
                            log::info!("Authorization fetched from file");
                            Ok(value)
                        }
                        Err(e) => { Err(format!("Cannot parse authorization: {}", e)) }
                    }
                }
                Err(_) => { Err("Could not read from authorization file".to_string()) }
            }
        }
        else {
            Err("Authorization file does not exist".to_string())
        }
    }

    pub fn get_auth_values() -> Result<Value, String> {
        Self::get_auth_from_file(Self::auth_path())
    }

    fn save_auth_to_file(auth: &Value, auth_file: PathBuf) -> Result<(), String> {
        string_to_file(&auth_file, &auth.to_string())?;
        #[cfg(windows)]
        if let Err(error) = LicenseManager::set_permissions(&auth_file) {
            log::error!("Setting permissions to {} failed: {}", &auth_file.to_string_lossy(), error);
        }
        Ok(())
    }

    pub fn save_auth_values(auth: &Value) -> Result<(), String> {
        Self::save_auth_to_file(auth, Self::auth_path())
    }

    pub fn valid_auth(auth: &Value) -> bool {
        auth["url"].is_string() && auth["token"].is_string() && auth["version"].is_string()
    }
}

fn create_license_manager() -> Box<dyn LicenseManagerTrait> {
    let auth_path = AuthManager::auth_path();
    if auth_path.exists() {
        let message = format!(">>> Fetched Auth Manager from {} <<<", auth_path.to_string_lossy());
        log::info!("{}", message.cyan(true));
        Box::new(AuthManager::from_path(auth_path))
    }
    else {
        log::info!("{}", ">>> Fetched License Manager (no auth file) <<<".cyan(true));
        Box::new(LicenseManager::new())
    }
}

pub fn fetch_license_manager() -> Result<Box<dyn LicenseManagerTrait>, AuthError> {
    let mut manager: Box<dyn LicenseManagerTrait> = create_license_manager();
    manager.read_license()?;
    Ok(manager)
}

#[cfg(test)]
mod tests {

}
