use crate::auth::file_auth::FileAuth;
use crate::auth::license_manager_trait::AuthError::{LicenseError, NetworkError, NotFoundError};
use crate::auth::license_manager_trait::{AuthError, LicenseManagerTrait};
use crate::create_dir_if_not_exists;
use crate::rest_comm::core_rest_comm::{GetRequestFunction, PostRequestFunction};
use crate::rest_request::RestServer;

use crate::utils::color::Coloralex;
use crate::utils::file_utils;
use log::{error, warn};
use serde::Serialize;
#[cfg(windows)]
use std::env;
#[cfg(windows)]
use crate::utils::bash_exec::BashExec;
use std::path::{Path, PathBuf};
use std::string::ToString;
use url::Url;

pub struct LicenseManager {
    path: PathBuf,
    url: Option<Url>,
    auth: Option<FileAuth>,
    token: String,
    read_json: fn(file_name: &Path) -> Result<serde_json::Value, String>,
    get_token: fn(manager: &LicenseManager, get: GetRequestFunction,
                  post: PostRequestFunction) -> Result<String, AuthError>
}

impl LicenseManagerTrait for LicenseManager {
    fn get_name(&self) -> Result<String, String> {
        if let Some(auth) = &self.auth {
            Ok(auth.name.clone())
        } else {
            Err(String::from("Name unavailable"))
        }
    }

    fn get_server(&self) -> Result<String, String> {
        if let Some(auth) = &self.auth {
            Ok(auth.server.clone())
        } else {
            Err(String::from("Server unavailable"))
        }
    }

    fn get_token(&self) -> Result<String, String> { Ok(self.token.clone()) }

    fn get_path(&self) -> Result<PathBuf, String> {
        Ok(self.path.clone())
    }

    fn get_url(&self) -> Result<Url, String> {
        if let Some(url) = self.url.clone() {
            Ok(url)
        } else {
            Err(String::from("The URL is None"))
        }
    }

    fn read_license(&mut self) -> Result<(), AuthError> {
        let json = (self.read_json)(&self.path);
        if let Err(error_message) = json {
            error!(
                "The following error occurred {} while reading license from {}",
                error_message,
                self.path.display()
            );
            return Err(LicenseError(error_message));
        };
        let json = json.map_err(LicenseError)?;

        if let (Some(b64_key), Some(license_id), Some(server), Some(node_name)) = (
            json["KeyB64"].as_str(),
            json["LicenseId"].as_str(),
            json["Server"].as_str(),
            json["FirstNodeLabel"].as_str(),
        ) {
            let url = LicenseManager::make_proper_url(server);
            log::info!(
                "License info:\n\tNode name: {}\n\tServer name: {}",
                node_name,
                server
            );
            let auth = FileAuth {
                name: String::from(node_name),
                server: String::from(server),
                b64_key: String::from(b64_key),
                license_id: String::from(license_id),
            };
            self.url = Some(url);
            self.auth = Some(auth);
            self.token = (self.get_token)(self, RestServer::get, RestServer::post)?;
            Ok(())
        } else {
            Err(LicenseError("Failed extracting values from license JSON".to_string()))
        }
    }
}

impl LicenseManager {
    pub fn make_proper_url(url: &str) -> Url {
        let https = if url.starts_with("https") || url.starts_with("http") {
            String::new()
        } else {
            "https://".to_string()
        };
        let url = format!("{https}{url}");
        Url::parse(&url).unwrap()
    }

    #[cfg(windows)] #[cfg(not(test))]
    fn resolve_license_path() -> PathBuf {
       let phantom_client_path = LicenseManager::resolve_phantom_client_path();
        phantom_client_path.join("licenses\\license")
    }
    #[cfg(windows)]
    fn resolve_phantom_client_path() -> PathBuf {
        let localappdata = env::var("ProgramData").unwrap();
        PathBuf::from(localappdata).join("Phantom_Client")
    }
    #[cfg(unix)] #[cfg(not(test))]
    fn resolve_license_path() -> PathBuf {
        PathBuf::from("/root/.config/phantom/Phantom_Streamer/licenses/license")
    }
    #[cfg(test)]
    fn resolve_license_path() -> PathBuf { PathBuf::from("./license") }

    fn get_src_path() -> PathBuf {
        #[cfg(not(windows))]
        return PathBuf::from("/root/snap/phau-core/common/license");
        #[cfg(windows)]
        return PathBuf::from("C:\\Program Files\\phantom_agent\\bin\\license");
    }
    pub fn move_license() {
        let src_path = LicenseManager::get_src_path();
        let dst_path = LicenseManager::get_license_path();
        match dst_path.parent() {
            None => warn!("License folder is root"),
            Some(license_parent_folder) => create_dir_if_not_exists(license_parent_folder),
        }
        // Move license if exists in src and does not exist in dst
        if src_path.exists() && !dst_path.exists() {
            log::info!(
                "Moving license from\n{}\nto\n{}",
                src_path.to_string_lossy(),
                dst_path.to_string_lossy()
            );
            if let Err(error) = file_utils::mv_file(&src_path, &dst_path) {
                log::error!("Failed moving license: {error}");
            }
        }
        // Set permissions recursively to allow non administrative apps work with this folder
        #[cfg(windows)]
        if let Err(error) = LicenseManager::set_permissions(&LicenseManager::resolve_phantom_client_path()){
            log::error!("Setting permissions to {} failed: {}", &LicenseManager::resolve_phantom_client_path().to_string_lossy(), error);
        }
    }
    #[cfg(windows)]
    pub(crate) fn set_permissions(path: &Path) -> Result<(), String>{
        let command = if path.is_dir() {
            format!("icacls {} /q /c /t /grant Users:F", path.to_string_lossy())
        } else {
            format!("icacls {} /q /c /grant Users:F", path.to_string_lossy())
        };
        let result = BashExec::exec(&command)?;
        log::info!("Setting permissions succeed: {}", result);
        Ok(())
    }
    fn get_license_path() -> PathBuf {
        let license_path = LicenseManager::resolve_license_path();
        let message = format!("License path: {}", license_path.to_string_lossy()).green(true);
        log::info!("{message}");
        license_path
    }
    pub fn new() -> Self {
        LicenseManager {
            path: LicenseManager::get_license_path(),
            url: None,
            auth: None,
            token: Default::default(),
            read_json: file_utils::file_to_json,
            get_token: LicenseManager::get_token,
        }
    }
    pub fn from_path(path: PathBuf) -> Self {
        LicenseManager {
            path,
            url: None,
            auth: None,
            token: Default::default(),
            read_json: file_utils::file_to_json,
            get_token: LicenseManager::get_token,
        }
    }

    pub fn get_token(manager: &LicenseManager, get: GetRequestFunction, post: PostRequestFunction) -> Result<String, AuthError> {
        let challenge = manager.request_challenge(get)?;
        let challenge = manager.auth.as_ref().unwrap().get_challenge_response(&challenge)?;
        manager.validate_challenge(challenge, post)
    }
    fn redirect_on_not_found_get(&self, route: &str, get: GetRequestFunction) -> Result<String, AuthError> {
        log::info!("Redirecting call to v1");
        let req_url = self.url.as_ref().unwrap().join("/api/v1/").unwrap();
        let req_url = req_url.join(route).unwrap();
        LicenseManager::parse_challenge_response(get(&req_url, None))

    }
    fn request_challenge(&self, get: GetRequestFunction) -> Result<String, AuthError> {
        let req_url = self.url.as_ref().unwrap().join("/api/v3/").unwrap();
        let route = &format!("requestChallenge?licenseId={}", self.auth.as_ref().unwrap().license_id);
        let req_url = req_url.join(route).unwrap();
        let result = LicenseManager::parse_challenge_response(get(&req_url, None));
        // if v3 fails, try v1
        if let Err(NotFoundError(_message)) = result {
            self.redirect_on_not_found_get(route, get)
        } else {
            result
        }
    }
    fn parse_challenge_response(
        response: Result<(String, u16), (String, u16)>,
    ) -> Result<String, AuthError> {
        let response = match response {
            Ok((message, 200)) => message,
            Ok((message, _)) => return Err(LicenseError(message)),
            Err((message, 404)) => return Err(NotFoundError(message)),
            Err((message, _)) => return Err(NetworkError(message)),
        };
        let json = match file_utils::string_to_json(&response) {
            Ok(result) => result,
            Err(error) => return Err(LicenseError(error)),
        };

        if let Some(challenge) = json["challenge"].as_str() {
            Ok(String::from(challenge))
        } else {
            Err(LicenseError(format!(
                "Failed getting challenge from the response {response}"
            )))
        }
    }
    fn validate_challenge_reroute(&self, route_version: &str, challenge_response: String, post: PostRequestFunction) ->  Result<String, AuthError> {
        let req_url = self
            .url
            .as_ref()
            .unwrap().join(route_version).unwrap()
            .join("validateChallenge")
            .unwrap();
        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct ValidateRequest {
            hmac: String,
            licenseId: String,
        }
        let request = ValidateRequest {
            hmac: challenge_response,
            licenseId: String::from(&self.auth.as_ref().unwrap().license_id),
        };
        match post(&req_url, &serde_json::json!(request), None) {
            Ok((response, 200)) => LicenseManager::parse_validate_challenge(response),
            Ok((response, _)) => Err(LicenseError(response)),
            Err((error, 404)) => Err(NotFoundError(error)),
            Err((error, _)) => Err(NetworkError(error)),
        }
    }

    // returns token
    fn validate_challenge(&self, challenge_response: String, post: PostRequestFunction) -> Result<String, AuthError> {
        let result = self.validate_challenge_reroute("/api/v3/", challenge_response.clone() ,post);
        if let Err(NotFoundError(_error)) = result {
            self.validate_challenge_reroute("/api/v1/", challenge_response ,post)
        } else { result }
    }

    fn parse_validate_challenge(response: String) -> Result<String, AuthError> {
        let json = match file_utils::string_to_json(&response) {
            Ok(json) => json,
            Err(error) => return Err(AuthError::DecodingError(error)),
        };
        log::info!("challenge response {}", response);
        if let Some(token) = json["token"].as_str() {
            Ok(String::from(token))
        } else {
            Err(AuthError::DecodingError(format!(
                "Failed getting token from the response {response}",
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_proper_url() {
        let actual = LicenseManager::make_proper_url("tst.phantomauto.com");
        assert_eq!(actual.to_string(), "https://tst.phantomauto.com/".to_string());
        let actual = LicenseManager::make_proper_url("https://tst.phantomauto.com");
        assert_eq!(actual.to_string(), "https://tst.phantomauto.com/");
        let actual = LicenseManager::make_proper_url("http://tst.phantomauto.com");
        assert_eq!(actual.to_string(), "http://tst.phantomauto.com/")
    }

    #[test]
    fn simple_test() {
        let mock_get = |_: &Url, _: Option<String>| {
            Ok((
                String::from(r#"{"challenge":"yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M="}"#),
                200,
            ))
        };
        let mut manager = LicenseManager {
            path: Default::default(),
            url: None,
            auth: None,
            token: Default::default(),
            read_json: |_| {
                let json = r#"{"LicenseId":"623089a7a9cf3f001a1e7be7","Server":"tst.phantomauto.com","FirstNodeLabel":"PHANTOM-WSL2-ALEX","KeyB64":"cMWS8MqXd6TVWR0cKxc5SH6MAXM="}"#;
                Ok(serde_json::from_str(json).unwrap())
            },
            get_token: |_,_,_| { Ok(Default::default()) }
        };
        manager.read_license().expect("Failed to read license!");

        let challenge = manager.request_challenge(mock_get).unwrap();
        // assert_eq!("yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M=", challenge);
        let challenge = manager
            .auth
            .as_ref()
            .unwrap()
            .get_challenge_response(&challenge)
            .unwrap();

        let mock_post = |_: &Url, _: &serde_json::Value, _: Option<String>| {
            Ok((
                String::from(
                    r#"{
                    "verified":true,
                    "license":{
                       "id":"60472f20aa32d50018690dd9",
                       "label":"PHANTOM-IL-DESKTOP-ALEX"
                    },
                    "node":{
                       "id":"60472f20aa32d50018690dd9",
                       "label":"PHANTOM-IL-DESKTOP-ALEX"
                    },
                    "token":"eyJhbGciOiJQUzI1NiIsImtpZCI6InpZMEM5WVlLcmNiMS0tdEloNERYXyJ9.eyJpc3MiOiJ0c3QucGhhbnRvbWF1dG8uY29tLyIsImF1ZCI6InRzdC5waGFudG9tYXV0by5jb20vY291cGxpbmciLCJzdWIiOiI2MDQ3MmYyMGFhMzJkNTAwMTg2OTBkZDkiLCJleHAiOjE2Mzk2Njc2MTAsImlhdCI6MTYzOTU4MTIxMH0.W2w1OaUZtZSbV8zG6Y7ubCCWTJXzWc5759X1iLlUWV6SxakpLnVLeWzFfhAm3DkGJzY-SFOZhZ4hr2YO-tCd4X_oDz2HTNrDECh1AsMJ3rKXBAOzbgaYaUDdz88FIKmJAdHTuU2IZG-IessGbtfSZdy6a7ckBdnExCii3SwcftWnWqTQiBTT5VNXSyYbmXLmsQxV8nB1Toil84sZrthtQKFppPYfk1wZ6o03cFyMmF2TUbhIMQ-SsL-bz_9fhpW5j_DGdkuVPzJStslfpk0FxAoizVOLOms0YovL6TJaB2bHFla8vmIdYs9PLbaFrIrrGf-0NfbHVW_c5j9_2GIXYA"
                 }"#,
                ),
                200,
            ))
        };

        let token = manager.validate_challenge(challenge, mock_post).unwrap();
        println!("{}", token)
    }

    #[test]
    fn simple_test_server_failure() {
        let mock_get = |_: &Url, _: Option<String>| Err((format!("Responded with - {}", 599, ), 599));
        let mut manager = LicenseManager {
            path: Default::default(),
            url: None,
            auth: None,
            token: Default::default(),
            read_json: |_| {
                let json = r#"{"LicenseId":"623089a7a9cf3f001a1e7be7","Server":"tst.phantomauto.com","FirstNodeLabel":"PHANTOM-WSL2-ALEX","KeyB64":"cMWS8MqXd6TVWR0cKxc5SH6MAXM="}"#;
                Ok(serde_json::from_str(json).unwrap())
            },
            get_token: |_,_,_| { Ok(Default::default()) }
        };
        manager.read_license().expect("Failed to read license!");

        let mock_post = |_: &Url, _: &serde_json::Value, _: Option<String>| {
            Ok((
                String::from(
                    r#"{
                    "verified":true,
                    "license":{
                       "id":"60472f20aa32d50018690dd9",
                       "label":"PHANTOM-IL-DESKTOP-ALEX"
                    },
                    "node":{
                       "id":"60472f20aa32d50018690dd9",
                       "label":"PHANTOM-IL-DESKTOP-ALEX"
                    },
                    "token":"eyJhbGciOiJQUzI1NiIsImtpZCI6InpZMEM5WVlLcmNiMS0tdEloNERYXyJ9.eyJpc3MiOiJ0c3QucGhhbnRvbWF1dG8uY29tLyIsImF1ZCI6InRzdC5waGFudG9tYXV0by5jb20vY291cGxpbmciLCJzdWIiOiI2MDQ3MmYyMGFhMzJkNTAwMTg2OTBkZDkiLCJleHAiOjE2Mzk2Njc2MTAsImlhdCI6MTYzOTU4MTIxMH0.W2w1OaUZtZSbV8zG6Y7ubCCWTJXzWc5759X1iLlUWV6SxakpLnVLeWzFfhAm3DkGJzY-SFOZhZ4hr2YO-tCd4X_oDz2HTNrDECh1AsMJ3rKXBAOzbgaYaUDdz88FIKmJAdHTuU2IZG-IessGbtfSZdy6a7ckBdnExCii3SwcftWnWqTQiBTT5VNXSyYbmXLmsQxV8nB1Toil84sZrthtQKFppPYfk1wZ6o03cFyMmF2TUbhIMQ-SsL-bz_9fhpW5j_DGdkuVPzJStslfpk0FxAoizVOLOms0YovL6TJaB2bHFla8vmIdYs9PLbaFrIrrGf-0NfbHVW_c5j9_2GIXYA"
                 }"#,
                ),
                200,
            ))
        };

        let error = LicenseManager::get_token(&manager, mock_get, mock_post).unwrap_err();
        assert_eq!(error.to_string(), "NetworkError: Responded with - 599");
    }
}

#[cfg(test)]
mod real_tests {
    use super::*;
    use crate::rest_request::RestServer;

    // this test is disabled for now
    #[test]
    fn load_license() {
        let mut manager = LicenseManager {
            path: Default::default(),
            url: None,
            auth: None,
            token: Default::default(),
            read_json: |_| {
                let json = r#"{"LicenseId":"623089a7a9cf3f001a1e7be7","Server":"tst.phantomauto.com","FirstNodeLabel":"PHANTOM-WSL2-ALEX","KeyB64":"cMWS8MqXd6TVWR0cKxc5SH6MAXM="}"#;
                Ok(serde_json::from_str(json).unwrap())
            },
            get_token: |_,_,_| { Ok(Default::default()) }
        };
        manager.read_license().expect("Failed to read license!");
        assert_eq!(
            manager.get_url().unwrap(),
            Url::parse("https://tst.phantomauto.com").unwrap()
        );
        assert_eq!(manager.auth.unwrap().license_id, "623089a7a9cf3f001a1e7be7")
    }

    #[ignore]
    #[test]
    fn non_mock_test() {
        //   let url = Url::parse("https://engineering-tlv.phantomauto.com").unwrap();

        let _il_path = PathBuf::from("/home/alex/Downloads/65e8723c4b9437980343a21c.txt");
        let _eng_path = PathBuf::from("/home/alex/Downloads/65d3283a98a2b0684867865d.txt");
        let _qa_path = PathBuf::from("/home/phantom-il-alex/Downloads/6422b6be0335a17dc703f029.txt");
        let _path = PathBuf::from("C:\\ProgramData\\Phantom_Client\\licenses\\license");
        let mut manager = LicenseManager {
            path: _il_path,
            url: None,
            auth: None,
            token: Default::default(),
            read_json: file_utils::file_to_json,
            get_token: |_,_,_| { Ok(Default::default()) },
        };

        manager.read_license().expect("Failed to read license!");
        let mock_get = RestServer::get;

        let challenge = manager.request_challenge(mock_get).unwrap();
        // assert_eq!("yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M=", challenge);
        let challenge = manager
            .auth
            .as_ref()
            .unwrap()
            .get_challenge_response(&challenge)
            .unwrap();

        let mock_post = RestServer::post;

        let token = manager.validate_challenge(challenge, mock_post).unwrap();
        println!("token {token}")
    }

    #[test]
    #[ignore]
    fn mock_error_test() {
        //   let url = Url::parse("https://engineering-tlv.phantomauto.com").unwrap();
        let path = PathBuf::from("C:\\ProgramData\\Phantom_Client\\licenses\\license");
        let mut manager = LicenseManager {
            path,
            url: None,
            auth: None,
            token: Default::default(),
            read_json: file_utils::file_to_json,
            get_token: |_,_,_| { Ok(Default::default()) }
        };
        manager.read_license().expect("Failed to read license!");
        let mock_get = |_: &Url, _: Option<String>| Err(("Failed".to_string(), 400));
        let mock_post =
            |_: &Url, _: &serde_json::Value, _: Option<String>| Err(("Failed".to_string(), 400));
        LicenseManager::get_token(&manager, mock_get, mock_post).unwrap_err();
        // assert_eq!("yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M=", challenge);
    }

    #[test]
    fn mock_server_error_test() {
        //   let url = Url::parse("https://engineering-tlv.phantomauto.com").unwrap();
        let mut manager = LicenseManager {
            path: Default::default(),
            url: None,
            auth: None,
            token: Default::default(),
            read_json: |_| {
                let json = r#"{"LicenseId":"623089a7a9cf3f001a1e7be7","Server":"oden.phantomauto.com","FirstNodeLabel":"PHANTOM-WSL2-ALEX","KeyB64":"cMWS8MqXd6TVWR0cKxc5SH6MAXM="}"#;
                Ok(serde_json::from_str(json).unwrap())
            },
            get_token: |_,_,_| { Ok(Default::default()) }
        };

        manager.read_license().expect("Failed to read license!");
        //  let mock_get =| _:&Url,_:Option<String> |{Err("Failed".to_string())} ;
        let mock_post =
            |_: &Url, _: &serde_json::Value, _: Option<String>| Err(("Failed".to_string(), 400));
        LicenseManager::get_token(&manager, RestServer::get, mock_post).unwrap_err();
        // assert_eq!("yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M=", challenge);
    }
}
