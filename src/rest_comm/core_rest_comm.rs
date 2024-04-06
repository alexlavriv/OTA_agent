use crate::rest_comm::core_rest_comm_trait::CoreRestCommTrait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Result as SerdeResult};
use std::{path::Path, string::String};


#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CouplingStatus {
    Online,
    Connected,
}

#[derive(Deserialize, Serialize)]
pub struct StatusData {
    pub node_status: String,
}

// Just ignore everything else
#[derive(Serialize, Deserialize)]
pub struct StatusMessage {
    data: StatusData,
}

// Just ignore everything else
#[derive(Serialize, Deserialize)]
pub struct CoreSnapResponse {
    success: bool,
}

// Just ignore everything else
#[derive(Serialize, Deserialize)]
pub struct SnapResponse {
    status: bool,
    msg: serde_json::Value,
}

pub type PostRequestFunction =
fn(&Url, &serde_json::Value, Option<String>) -> Result<(String, u16), (String, u16)>;
pub type GetRequestFunction = fn(&Url, Option<String>) -> Result<(String, u16), (String, u16)>;

pub struct CoreRestComm {
    pub url: Url,
    pub post: PostRequestFunction,
    pub get: GetRequestFunction,
}

impl CoreRestComm {
    fn is_core_has_connected_session(&self) -> bool {
        // If Core has at least one connected session, the answer is true.
        if let Ok((response_string, _)) = (self.get)(&self.url.join("status").unwrap(), None) {
            if let Ok(status_json) = serde_json::from_str::<StatusMessage>(&response_string) {
                return status_json.data.node_status == "connected";
            }
        }
        false
    }


    fn update_manifest_version(&self, version: &str){
        let request = json!({"manifest_name":version});
        match (self.post)(&self.url.join("manifest_version").unwrap(), &request, None) {
            Ok((_response_string, _code)) => {
                log::info!("Submitted manifest_version {} to Core Plugin", version);

            }
            Err((err, code)) => {
                let error = format!("Failed submitting manifest_version {} to Core plugin with error {} code {code}", version, err);
                log::error!("{}", error);
            }
        }
    }


    fn install_snap(&self, snap_path: &Path) -> Result<(), String> {
        let request = json!({"snap_path": snap_path.to_string_lossy()});

        match self.url.join("install_snap") {
            Err(error) => Err(error.to_string()),
            Ok(url) => {
                let (result, _) = (self.post)(&url, &request, None).map_err(|(err_message, _err_code)| err_message)?;
                log::info!("Installed snap with message {}", result);
                let result: SerdeResult<CoreSnapResponse> = serde_json::from_str(&result);
                if let Ok(result) = result {
                    if result.success {
                        Ok(())
                    } else {
                        Err(String::from("Error occurred during snap installation"))
                    }
                } else {
                    Err(String::from("Error occurred during JSON parsing "))
                }
            }
        }
    }
}

impl CoreRestCommTrait for CoreRestComm {
    fn is_core_has_connected_session(&self) -> bool {
        self.is_core_has_connected_session()
    }
    fn install_snap(&self, snap_path: &Path) -> Result<(), String> {
        self.install_snap(snap_path)
    }

    fn update_manifest_version(&self, version: &str) {
        self.update_manifest_version(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;

    #[test]
    fn is_core_connected_test1() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| Err((String::from("error"), 400)),
            get: |_, _| {
                Ok((
                    String::from(
                        r#"{
   "core_version":"dev--DEV-6827--noa",
   "data": {
        "node_status": "online"
   },
   "success":true
}
"#,
                    ),
                    200,
                ))
            },
        };

        let res = core_rest_comm.is_core_has_connected_session();
        assert_eq!(res, false);
    }

    #[test]
    fn is_core_connected_test2() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| Err((String::from("error"), 400)),
            get: |_, _| {
                Ok((
                    String::from(
                        r#"{
   "core_version":"dev--DEV-6827--noa",
   "data": {
        "node_status": "connected"
   },
   "success":true
}
"#,
                    ),
                    200,
                ))
            },
        };

        let res = core_rest_comm.is_core_has_connected_session();
        assert!(res);
    }

    #[test]
    fn is_core_connected_test3() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| Err((String::from("error"), 400)),
            get: |_, _| {
                Ok((
                    String::from(
                        r#"{
   "core_version":"dev--DEV-6827--noa",
   "msg":[
   ],
   "success":true
}
"#,
                    ),
                    200,
                ))
            },
        };

        let res = core_rest_comm.is_core_has_connected_session();
        assert_eq!(res, false);
    }

    #[test]
    fn is_core_connected_test4() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| Err((String::from("error"), 400)),
            get: |_, _| Err((String::from("Something went wrong"), 400)),
        };

        let res = core_rest_comm.is_core_has_connected_session();
        assert_eq!(res, false);
    }

    #[test]
    fn install_snap() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| {
                Ok((
                    String::from(
                        r#"{"success":true,"msg":{"type":"async","status-code":202,"status":"Accepted","result":null,"change":"9"},"core_version":"dev--snap_installer--alexl"}"#,
                    ),
                    200,
                ))
            },
            get: |_, _| Err((String::from("Something went wrong"), 400)),
        };
        let path = Path::new("/package.snap");
        core_rest_comm
            .install_snap(path)
            .expect("should not get here");
    }

    #[test]
    fn install_snap_error() {
        let core_rest_comm = CoreRestComm {
            url: Url::parse("http://localhost").unwrap(),
            post: |_, _, _| {
                Ok((
                    String::from(
                        r#"{"success":false,"msg":"Error! Internal Server Error","core_version":"dev--snap_installer--alexl"}"#,
                    ),
                    200,
                ))
            },
            get: |_, _| Err((String::from("Something went wrong"), 400)),
        };
        let path = Path::new("/package.snap");
        let expected = Err(String::from("Error occurred during snap installation"));
        let actual = core_rest_comm.install_snap(path);
        assert_eq!(expected, actual);
    }
}
