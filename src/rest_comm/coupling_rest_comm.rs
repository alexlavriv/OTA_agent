use crate::rest_comm::coupling_submit_trait::*;
use crate::rest_request::{RestServer, SendType};
use std::path::{Path, PathBuf};
use log::{warn, info};
use serde_json::Value;
#[cfg(unix)]
use log::error;
use url::Url;
use crate::auth::auth_manager::{AuthManager, fetch_license_manager};
use crate::auth::license_manager_trait::LicenseManagerTrait;

pub type RESTRequestFunction =
fn(SendType, &Url, Option<&serde_json::Value>, Option<String>) -> Result<(String, u16), (String, u16)>;

pub struct CouplingRestComm {
    pub url: Url,
    pub token: String,
    pub name: String,
    pub send: RESTRequestFunction,
    pub path: PathBuf,
    pub named: bool,
}

impl CouplingRestComm {
    fn redirect_send(&self, send_type: SendType, url: &str, value: Option<&serde_json::Value>, auth: Option<String>) -> Result<(String, u16), (String, u16)> {
        let req_url = self.url.join("api/v3/").unwrap().join(url).unwrap();
        let result = (self.send)(send_type, &req_url, value, auth.clone());
        if let Err((_e, 404)) = result {
            warn!("Redirecting call to v1");
            let req_url = self.url.join("api/v1/").unwrap().join(url).unwrap();
            (self.send)(send_type, &req_url, value, auth)
        } else {
            result
        }
    }
    pub fn new( license_manager: &dyn LicenseManagerTrait, send: RESTRequestFunction) -> Self {
        let named = match license_manager.get_path() {
            Ok(path) => { path == AuthManager::auth_path() }
            Err(_) => { false }
        };
        CouplingRestComm {
            url: license_manager.get_url().unwrap(),
            token: license_manager.get_token().unwrap_or_default(),
            name: license_manager.get_name().unwrap_or_default(),
            path: license_manager.get_path().unwrap_or_default(),
            send,
            named,
        }
    }
}

pub fn fetch_coupling_rest_comm() -> Result<CouplingRestComm, String> {
    match fetch_license_manager() {
        Ok(license_manager) => {
            let named = match license_manager.get_path() {
                Ok(path) => { path == AuthManager::auth_path() }
                Err(_) => { false }
            };
            Ok(CouplingRestComm {
                url: license_manager.get_url().unwrap(),
                token: license_manager.get_token().unwrap_or_default(),
                name: license_manager.get_name().unwrap_or_default(),
                path: license_manager.get_path().unwrap_or_default(),
                send: RestServer::send_json,
                named,
            })
        }
        Err(e) => { Err(format!("Failed to fetch Coupling Rest Comm: {}", e)) }
    }
}

impl CouplingRestSubmitter for CouplingRestComm {
    fn post_checksums(&self, checksums: Value) -> Result<String, String> {
        info!("Posting checksums to server: {}", self.url.to_string());
        info!("Checksums: {}", &checksums);

        let (send_type, url, body) = if self.named {
                if !self.name.is_empty() {
                    (SendType::POST, format!("versions/{}/manifest", self.name), Some(&checksums))
                } else {
                    return Err("The version name is empty, ignoring this call".to_string());
                }
            }
            else {
                (SendType::POST, "versions/manifest?includeVersion=true".to_string(), Some(&checksums))
            };
        let (content, _) = self.redirect_send(
            send_type,
            &url,
            body,
            Some(format!("Bearer {}", self.token)),
        ).map_err(|(message, code)|
            if code == 404 {
                format!("Failed getting manifest details with message {message}")
            } else { message })?;
        Ok(content)
    }

    #[cfg(windows)]
    fn put_ota_status(&self, _message: Option<String>, _eta: Option<u64>, _ota_progress: NodeOtaProgressStatus){}
    #[cfg(unix)]
    fn put_ota_status(&self, message: Option<String>, eta: Option<u64>, ota_progress: NodeOtaProgressStatus) {
        let ota_status = NodeOtaStatus {
            eta: eta.unwrap_or_default(),
            status: ota_progress,
            message: message.unwrap_or_default(),
        };

        let status_json = serde_json::json!(ota_status);
        info!("Putting the following status to dashboard: {status_json}",);
        let response = self.redirect_send(
            SendType::PUT,
            "nodes/self/ota",
            Some(&status_json),
            Some(format!("Bearer {}", self.token)));

        if let Err((_, 404)) = response
        {
            // To support backwards compatability with 1.27, when getting 404,
            // we will submit to the previous route with previous object
            let status_json = serde_json::json!({"otaStatus": ota_status.status.as_str()});
            if let Err((content, code)) = self.redirect_send(
                SendType::PUT,
                "nodes/self/ota-status",
                Some(&status_json),
                Some(format!("Bearer {}", self.token))) {
                error!("Error {content} occurred while updating OTA status witch code: {code}");
            }
        } else if response.is_err() {
            error!("Unknown Error during OTA status update");
        }
    }

    fn send_file_to_jira(&self, file: &Path, ticket: &str) -> Result<String, String> {
        info!("Posting file {} to jira {}", file.to_string_lossy(), ticket);
        let file_json = serde_json::json!({"file": file.to_string_lossy()});
        let (content, _) = self.redirect_send(
            SendType::FILE,
            &format!("support/jira-tickets/{ticket}/attach"),
            Some(&file_json),
            Some(format!("Bearer {}", self.token)),
        ).map_err(|(message, _code)| message)?;
        Ok(content)
    }

    fn get_url_and_token(&self) -> (Url, String) {
        (self.url.clone(), self.token.clone())
    }

    fn check_versions(&self) -> Result<String, String> {
        match self.redirect_send(
            SendType::GET,
            "versions",
            None,
            Some(format!("Bearer {}", self.token)),
        ) {
            Ok((text, _)) => { Ok(text) }
            Err((text, _)) => { Err(text) }
        }
    }

    #[cfg(windows)]
    fn get_ota_status(&self) -> NodeOtaProgressStatus { NodeOtaProgressStatus::Updated }
    #[cfg(unix)]
    fn get_ota_status(&self) -> NodeOtaProgressStatus {
        info!("Getting status from dashboard");
        let reply = match self.redirect_send(
            SendType::GET,
            "nodes/self/ota",
            None,
            Some(format!("Bearer {}", self.token))
        ) {
            Ok((reply, _code)) => { reply }
            Err((reply, code)) => {
                warn!("Failed to get reply ({} [{}]), assuming Triggered", reply, code);
                return NodeOtaProgressStatus::Triggered;
            }
        };

        let reply_json: serde_json::Value = serde_json::from_str(&reply).unwrap_or_default();
        match reply_json.get("data") {
            None => {
                warn!("Failed to get data ({}), assuming Triggered", reply);
                NodeOtaProgressStatus::Triggered
            }
            Some(data) => {
                match data.get("status") {
                    None => {
                        warn!("Failed to get status ({}), assuming Triggered", reply);
                        NodeOtaProgressStatus::Triggered
                    }
                    Some(status) => {
                        NodeOtaProgressStatus::from_string(status.as_str().unwrap_or_default())
                    }
                }
            }
        }
    }

    fn get_node_info(&self) -> Result<String, String> {
        info!("Get node information");
        match self.redirect_send(
            SendType::GET,
            "nodes/64e4b108d05d6c6ab735060c",
            None,
            Some(format!("Bearer {}", self.token)),
        ) {
            Ok((text, _)) => { Ok(text) }
            Err((text, _)) => { Err(text) }
        }
    }
}


#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use url::Url;
    use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
    use crate::rest_comm::coupling_submit_trait::{CouplingRestSubmitter, NodeOtaProgressStatus};
    use crate::rest_request::{RestServer};

    #[test]
    #[ignore]
    fn test_put_ota_status() {
        let token = "eyJhbGciOiJQUzI1NiIsImtpZCI6ImhiMXR5cFNxTzJNNXc3NmJjS0FiaSJ9.eyJpc3MiOiJpbC5waGFudG9tYXV0by5kZXYvIiwiYXVkIjpbImlsLnBoYW50b21hdXRvLmRldi9jb3VwbGluZyIsImlsLnBoYW50b21hdXRvLmRldi9wcm94eSJdLCJzdWIiOiI2NDIzMDI5ZDBiYzYzZTA3YmU4YWU3MzQiLCJleHAiOjE2ODAxNzgyNzYsImlhdCI6MTY4MDA5MTg3Nn0.nPPldSwTdIa9z55wKzvwKqQHUdh5Sbgnss3-gX_4wHFdWNn94b8uoyymlXfbWf8aPaO6kzXr-0abdFxDtpgzMV4aPmnfqZWRxzRooX27rlqL4dBpf1L1e_v_Xcl9ncN76wuY4ZjPj4k-NykAs_xoOOKoHF_DdK5wl0RZYN8nU7plaZLwTini43Foe6AtjUHMyZXvLR6OB877CVmGr_QHgfqXKqBGjYLA_aSK_91TLbEPQfD3zbiUk7hwVVfgZKHLWnIm6qL1fEMLAwmT5DIa1XNF-GFzRsVjC5xRVRFcWCiKtE9X9PNxHFu5QYLTvgm8po46XldUv-sNtn1nL2keKw".to_string();
        let url = Url::parse("https://eng.phantomauto.dev").unwrap();
        //let status =
        let coupling = CouplingRestComm { url, token, name: "".to_string(), path: PathBuf::from(""), send: RestServer::send_json, named: false };

        coupling.put_ota_status(None, None, NodeOtaProgressStatus::Downloading);

        let json = serde_json::from_str(r#"{"checksums":{},"arch":"WIN"}"#).unwrap();

        match coupling.post_checksums(json) {
            Ok(content) => { println!("OK {content}") }
            Err(err) => { println!("error: {err}") }
        }
    }

    #[ignore]
    #[test]
    fn test_post_checksums() {
        let token = "eyJhbGciOiJQUzI1NiIsImtpZCI6IlpyZFBWZU10OU53TDZZVDlVcU9SZSJ9.eyJodHRwczovL3BoYW50b21hdXRvLmNvbS9vcmdhbml6YXRpb25JZCI6IjYwMTFjZGVkY2M5NjU2MDAxODlmNGQ0NSIsImlzcyI6ImVuZy5waGFudG9tYXV0by5kZXYvIiwiYXVkIjpbImVuZy5waGFudG9tYXV0by5kZXYvY291cGxpbmciLCJlbmcucGhhbnRvbWF1dG8uZGV2L3Byb3h5Il0sInN1YiI6IjY0MmFjYTBlMzM3YzJkNjNkNzM0YTdmYyIsImV4cCI6MTY4NzUwNjMxNiwiaWF0IjoxNjg3NDE5OTE2fQ.anQI48oZxrz9FFXxIRSEsfAbatU1sim_JhEdnZQQf1HO2NtnmDbw6iPMU-eUe39XnkY8Wl3AzwIvOTsyZdadpfAXFxy92bis1WRk77Q7kCHiRa2pRrKUtcaBX1naBLACyVIjy2rtEA1yUVn435CeaT-tBpqvk2WZkgxuNeWAZd3ZXJBlWt9S0xoGBpw3flGSKGItXDzJP_j-kAPN0LFcJ1Pn3qTpJixklNqhMlnz9Z_LByuF3EJVruoIISU8wACJqWxJDKlGSDxjNTd4beM59C2HRQQJvyJUK5PxwHQV35Lc6nydKzibWxs2jnLu55wAzsMtEP0dBUtlKwR15K956g".to_string();
        let url = Url::parse("https://eng.phantomauto.dev").unwrap();

        let coupling = CouplingRestComm { url, token, name: "".to_string(), path: PathBuf::from(""), send: RestServer::send_json, named: false };

        let json = serde_json::from_str(r#"{"checksums":{},"arch":"WIN"}"#).unwrap();

        match coupling.post_checksums(json) {
            Ok(content) => { println!("OK {content}") }
            Err(err) => { println!("error: {err}") }
        }
    }

    #[ignore]
    #[test]
    fn test_parse_ota() {
        let reply = r#"{"data":{"status":"updated","version":"local","lastUpdated":"2023-10-05T11:29:43.550Z","eta":0,"updatedBy":"alex@phantomauto.com"},"count":1,"Node":{"_id":"63ea55c77b6d4dc6e05e7c48","label":"PHANTOM-IL-TX2-ALEX"}}"#.to_string();
        let reply_json: serde_json::Value = serde_json::from_str(&reply).unwrap_or_default();
        match reply_json.get("data") {
            None => {
                println!("Failed to get data ({}), assuming Triggered", reply);
            }
            Some(data) => {
                match data.get("status") {
                    None => {
                        println!("Failed to get status ({}), assuming Triggered", reply);
                    }
                    Some(status) => {
                        println!("Status is {}", status.as_str().unwrap_or_default());
                    }
                }
            }
        }
    }
}
