use std::collections::HashMap;
use std::path::{Path, PathBuf};
use log::error;
use crate::auth::auth_manager::fetch_license_manager;
use crate::utils::file_utils::{file_to_string, string_to_file};
use url::Url;

pub const VERSIONS_FILE_PATH: &str = "versions";

pub struct VersionTable {
    pub read_function: fn(path: &Path) -> Result<String, String>,
    pub write_function: fn(path: &Path, content: &str) -> Result<(), String>,
}

impl VersionTable {
    pub fn new() -> VersionTable {
        Self { read_function: file_to_string, write_function: string_to_file }
    }
    fn read_versions(&self) -> HashMap<String, String> {
        let file_path = VersionTable::get_version_table_path();
        match (self.read_function)(file_path.as_path()) {
            Ok(file_content) => {
                match serde_json::from_str(&file_content){
                    Ok(json) => json,
                    Err(err)=>{
                        let message = format!("Error occured while reading version table {}, overriding the file {}", err, file_path.to_string_lossy());
                        log::error!("{}", message);
                        HashMap::new()
                        
                    }
                }
            }
            Err(error) => {
                let message = format!("Error occured while reading version table {}, overriding the file {}", error, file_path.to_string_lossy());
                log::error!("{}", message);
                HashMap::new()
            }
        }
    }
    pub fn get_version(&self) -> String {
        match fetch_license_manager() {
            Ok(license_manager) => {
                let versions_hash = self.read_versions();
                let server = license_manager.get_server().unwrap();
                let server = Self::strip_server_url(&server);
                match versions_hash.get(&server) {
                    Some(version) => version.clone(),
                    None => "not_supported".to_string()
                }
            }
            Err(error) => {
                error!("Failed getting the license: {error}");
                "not_supported".to_string()
            }
        }

    }
    fn get_version_table_path() -> PathBuf{
        //snap/phantom-agent
        #[cfg(unix)]
            let dir = PathBuf::from(
            "../");

        #[cfg(windows)]
            let dir = PathBuf::from("./");

        dir.join(VERSIONS_FILE_PATH)
    }
    pub fn update_version_file(&self, server: &str, version: &str) {
        let file_path = VersionTable::get_version_table_path();
        let mut versions_hash = self.read_versions();
        versions_hash.insert(Self::strip_server_url(server), version.to_string());
        match (self.write_function)(file_path.as_path(), &serde_json::to_string(&versions_hash).unwrap()) {
            Ok(_) => log::info!("Version file successfully updated"),
            Err(error) => log::error!("Failed updating version file with error {error}"),
        }
    }
    fn strip_server_url(server: &str) -> String{
        if !server.starts_with("https://") && !server.starts_with("http://") {
            return server.to_string();
        }
        match Url::parse(server){
            Ok(url) =>{
                match url.host_str(){
                    Some(host) => host.to_string(),
                    None => {
                        log::error!("Failed getting host from {}", server);
                        server.to_string()
                    }
                }
            },
            Err(error) => {
                log::error!("Failed parsing {} with error {}", server, error); 
                server.to_string()
            }
        }
    }


}

impl Default for VersionTable {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_strip_server(){
    
    assert_eq!(VersionTable::strip_server_url("https://qa.phantomauto.dev/"), "qa.phantomauto.dev".to_string());
    assert_eq!(VersionTable::strip_server_url("http://qa.phantomauto.dev/"), "qa.phantomauto.dev".to_string());
    assert_eq!(VersionTable::strip_server_url("qa.phantomauto.dev"), "qa.phantomauto.dev".to_string());
    assert_eq!(VersionTable::strip_server_url("qa.phantomauto.dev/"), "qa.phantomauto.dev/".to_string());
}