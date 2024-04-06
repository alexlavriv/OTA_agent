use std::path::Path;
use log;
use serde::{Deserialize, Serialize};
use std::str;
use regex::Regex;

#[cfg(windows)]
use crate::PathBuf;

use crate::ota::ota_error::OTAError;
use crate::ota::manifest::Component;
use crate::utils::bash_exec::ExecArgType;

#[derive(Serialize, Deserialize)]
struct SnapdResult {
    pub id: String,
    pub kind: String,
    pub status: String,
}

#[derive(Serialize, Deserialize)]
struct SnapdResponse {
    pub status: String,
    #[serde(rename(deserialize = "status-code"))]
    pub status_code: u64,
    pub change: Option<String>,
    pub result: Option<SnapdResult>,
}

#[derive(Serialize, Deserialize)]
struct CoreResponse {
    pub success: bool,
    pub msg: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct SnapdStatusResult {
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Serialize, Deserialize)]
struct SnapdStatus {
    pub status: String,
    #[serde(rename(deserialize = "status-code"))]
    pub status_code: u64,
    pub result: Vec<SnapdStatusResult>,
}

pub struct SnapInstaller;

impl SnapInstaller {
    pub fn wait_until_done(change: &str, exec_command: ExecArgType) -> Result<String, OTAError> {
        use std::thread::sleep;
        // We have initiated a change, now we can track it until it's done
        let command = format!(
            "curl --unix-socket /run/snapd.socket http://localhost/v2/changes/{}",
            change
        );
        loop {
            let response = (exec_command)(&command, &[])?;
            let response: SnapdResponse =
                serde_json::from_str(&response).map_err(|e| e.to_string())?;
            if response.status_code != 200 || response.result.is_none() {
                return Err(OTAError::nonfatal(response.status));
            }
            let status = response.result.unwrap();
            log::info!(
                "Status is: {}: {} - {}",
                status.id,
                status.kind,
                status.status
            );

            match status.status.as_str() {
                "Done" => return Ok(status.status),
                "Do" | "Doing" => {}
                _ => return Err(OTAError::nonfatal(status.status)),
            }
            sleep(std::time::Duration::new(1, 0));
        }
    }

    #[cfg(unix)]
    pub fn install(component: &Component, exec_command: ExecArgType) -> Result<String, OTAError> {
        let path = &component.path.clone().unwrap_or_default();
        let path_str = path.to_string_lossy();
        if !path.exists() {
            return Err(OTAError::nonfatal(format!(
                "{} not found",
                path_str
            )));
        }

        let (disable_after_install, snap_name) = if let Ok(snap_name) = Self::extract_snap_name(path, exec_command) {
            if let Ok(enabled) = Self::extract_snap_enabled(&snap_name, exec_command) { (!enabled, snap_name) }
            else { (false, snap_name) }
        }
        else { log::warn!("Could not extract snap name from file!"); (false, Default::default()) };

        if disable_after_install {
            if let Err(e) = Self::snap_enable(&snap_name, exec_command) {
                log::warn!("Tried to enable {} but failed: {}", snap_name, e);
            }
        }

        let command = format!("curl --unix-socket /run/snapd.socket http://localhost/v2/snaps -F snap=@{} -F dangerous=true -F classic=true", path_str);
        log::info!("Installing {}", path.to_string_lossy());
        let response = (exec_command)(&command, &[])?;
        let response: SnapdResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Unrecognized snapd response format - {}: [{}]", e, response))?;

        log::info!(
            "Response is: {} ({}): Change id is {}",
            response.status,
            response.status_code,
            response.change.clone().unwrap_or_else(|| "NONE".to_string())
        );
        if response.status_code != 202 || response.change.is_none() {
            return Err(OTAError::nonfatal(response.status));
        }
        let result = SnapInstaller::wait_until_done(&response.change.unwrap(), exec_command)?;
        if disable_after_install {
            Self::snap_disable(&snap_name, exec_command)?;
        }
        Ok(result)
    }


    pub fn extract_snap_name(path: &Path, exec_command: ExecArgType) -> Result<String, String> {
        let response = (exec_command)("snap info", &[&path.to_string_lossy()])?;
        log::trace!("RESPONSE IS {}", &response);
        let re = Regex::new(r"name:\s*(.*)").unwrap();
        if let Some(cap) = re.captures_iter(&response).next() {
            let size: usize = 1;
            if cap.len() > size {
                return Ok(cap[1].to_string());
            }
        }
        Err("Could not extract snap name from path".to_string())
    }

    pub fn extract_snap_enabled(name: &str, exec_command: ExecArgType) -> Result<bool, String> {
        let command = format!("curl --unix-socket /run/snapd.socket http://localhost/v2/snaps?snaps={}", name);
        let response = (exec_command)(&command, &[])?;
        log::trace!("SNAP STATUS IS {}", &response);
        let status: SnapdStatus = serde_json::from_str(&response)
            .map_err(|e| format!("Unrecognized snapd response format - {}: [{}]", e, response))?;
        let mut enabled = true;
        let mut exists = false;
        for result in status.result {
            let is_enabled = result.status != "installed";
            log::info!("{} ({}) enabled status is {}", &result.name, &result.version, is_enabled);
            enabled = enabled && is_enabled;
            exists = true;
        }
        match exists {
            true => { Ok(enabled) }
            false => { Err("This snap does not exist".to_string()) }
        }

    }

    pub fn snap_enable(name: &str, exec_command: ExecArgType) -> Result<String, OTAError> {
        log::info!("Enabling {}", name);
        let command = format!("curl --unix-socket /run/snapd.socket http://localhost/v2/snaps/{}", name);
        let response = (exec_command)(&command, &["-d", "{\"action\":\"enable\"}"])?;
        let response: SnapdResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Unrecognized snapd response format - {}: [{}]", e, response))?;

        log::info!(
            "Response is: {} ({}): Change id is {}",
            response.status,
            response.status_code,
            response.change.clone().unwrap_or_else(|| "NONE".to_string())
        );
        if response.status_code != 202 || response.change.is_none() {
            return Err(OTAError::nonfatal(response.status));
        }
        SnapInstaller::wait_until_done(&response.change.unwrap(), exec_command)
    }

    pub fn snap_disable(name: &str, exec_command: ExecArgType) -> Result<String, OTAError> {
        log::info!("Disabling {}", name);
        let command = format!("curl --unix-socket /run/snapd.socket http://localhost/v2/snaps/{}", name);
        let response = (exec_command)(&command, &["-d", "{\"action\":\"disable\"}"])?;
        let response: SnapdResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Unrecognized snapd response format - {}: [{}]", e, response))?;

        log::info!(
            "Response is: {} ({}): Change id is {}",
            response.status,
            response.status_code,
            response.change.clone().unwrap_or_else(|| "NONE".to_string())
        );
        if response.status_code != 202 || response.change.is_none() {
            return Err(OTAError::nonfatal(response.status));
        }
        SnapInstaller::wait_until_done(&response.change.unwrap(), exec_command)
    }

    #[cfg(windows)]
    fn decode_core_response(response: &str) -> Result<String, OTAError> {
        let response: serde_json::Result<CoreResponse> = serde_json::from_str(response);

        match response {
            Ok(core_response) => {
                if core_response.success {
                    let snapd_response: SnapdResponse =
                        serde_json::from_str(&core_response.msg.to_string()).unwrap();
                    if snapd_response.status_code == 202 {
                        Ok(snapd_response.status)
                    } else {
                        Err(OTAError::nonfatal(snapd_response.status))
                    }
                } else {
                    Err(OTAError::nonfatal(String::from(
                        core_response.msg.as_str().expect("The error is not string"),
                    )))
                }
            }
            Err(error) => Err(OTAError::nonfatal(error.to_string())),
        }
    }
    #[cfg(windows)]
    pub fn unixify_prefix(path: &Path) -> PathBuf {
        log::info!("unixify_prefix {}", path.display());
        let c_drive = PathBuf::from("c");
        let result = c_drive.join(path.strip_prefix("c:\\").unwrap());
        log::info!("unixify_prefix result {}", result.display());
        result
    }
    #[cfg(windows)]
    pub fn install(
        component: &Component,
        exec_command: ExecArgType,
    ) -> Result<String, OTAError> {
        use path_slash::PathBufExt;
        let path = &component.path.clone().unwrap_or_default();
        log::info!("installing {}", path.to_str().unwrap());
        if !path.exists() {
            log::error!("{} was not found", path.to_str().unwrap());
            return Err(OTAError::nonfatal(format!(
                "{} was not found",
                path.to_str().unwrap()
            )));
        }
        let path = std::env::current_dir().unwrap().join(path);
        let path = SnapInstaller::unixify_prefix(&path);
        let command = "curl -X POST http://localhost:8700/install_snap -d".to_string();
        let json = format!(
            r#"{{"snap_path":"/mnt/{}"}}"#,
            path.to_slash().expect("failed converting to linux path")
        );
        log::info!("Installing {}", json);
        let result = (exec_command)(&command, &[&json])?;
        log::info!("result is: {}", result);
        SnapInstaller::decode_core_response(&result)
    }

    #[cfg(windows)]
    pub fn uninstall(component: &Component, _exec_command: ExecArgType) -> Result<String, OTAError> {
        log::warn!("Uninstall requested for {} on Windows, but it's a snap install. Skipping...", component.component);
        Ok("Install skipped".to_string())
    }

    #[cfg(unix)]
    pub fn cleanup_deprecated_if_needed() {
        use crate::BashExec;
        for deprecated in ["sim-gps-info", "vapp-translator", "phau-core"] {
            if SnapInstaller::extract_snap_enabled(deprecated, BashExec::exec_arg).is_ok() {
                if let Err(e) = SnapInstaller::uninstall_by_name(deprecated, BashExec::exec_arg) {
                    log::info!("We tried to uninstall deprecated {} but couldn't: {}", deprecated, e.message);
                }
            }
        }
    }

    #[cfg(unix)]
    pub fn uninstall_by_name(snap_name: &str, exec_command: ExecArgType) -> Result<String, OTAError> {
        let data="{\"action\":\"remove\"}";
        let command = format!("curl -X POST --unix-socket /run/snapd.socket http://localhost/v2/snaps/{} -d {}", snap_name, data);
        log::info!("Uninstalling {}", snap_name);
        let response = (exec_command)(&command, &[])?;
        log::info!("Response is {}", response);
        let response: SnapdResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Unrecognized snapd response format - {}: [{}]", e, response))?;

        log::info!("Response is: {} ({}): Change id is {}",
            response.status,
            response.status_code,
            response.change.clone().unwrap_or_else(|| "NONE".to_string())
        );
        if response.status_code != 202 || response.change.is_none() {
            return Err(OTAError::nonfatal(response.status));
        }
        SnapInstaller::wait_until_done(&response.change.unwrap(), exec_command)
    }

    #[cfg(unix)]
    pub fn uninstall(component: &Component, exec_command: ExecArgType) -> Result<String, OTAError> {
        use crate::ota::manifest::ComponentType;
        use std::str::FromStr;
        let path = &component.path.clone().unwrap_or_default();
        let snap_name = if !path.exists() {
            if ComponentType::from_str(&component.component).unwrap() == ComponentType::sim_gps_info {
                "sim-gps-info".to_string() // We are uninstalling snap gps even without being given a path!
            } else if ComponentType::from_str(&component.component).unwrap() == ComponentType::translator {
                "vapp-translator".to_string() // We are uninstalling vapp translator even without being given a path!
            } else if ComponentType::from_str(&component.component).unwrap() == ComponentType::core {
                "phau-core".to_string() // We are uninstalling phau core even without being given a path!
            } else {
                return Err(OTAError::nonfatal(format!("{} not found", path.to_string_lossy())));
            }
        } else {
            Self::extract_snap_name(path, exec_command)?
        };
        Self::uninstall_by_name(&snap_name, exec_command)
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::cell::RefCell;
    use std::env::current_dir;
    use std::fs;
    use crate::ota::snap_installer::SnapInstaller;
    use crate::utils::bash_exec::BashExec;
    use crate::utils::log_utils::set_logging_for_tests;
    use std::path::{Path, PathBuf};
    use regex::Regex;
    use serde_json::Value;
    use url::Url;
    use crate::auth::license_manager::LicenseManager;
    use crate::auth::license_manager_trait::LicenseManagerTrait;
    use crate::ota::download_manager::DownloadManager;
    use crate::ota::manifest::{Component, ComponentType, Manifest};

    use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
    use crate::rest_request::{SendType, extract_token};
    use crate::utils::file_utils;

    fn snap_phantom_install(path: &Path) {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(path)),
            package_type: "snap".to_string(),
            ..
            Component::empty()
        };
        let result = SnapInstaller::install(&component, exec);
        if let Err(error) = result.clone() {
            println!("Error occurred: {}", error);
        } else {
            println!("Result is {}", result.unwrap());
        }
    }

    #[test]
    #[ignore]
    fn snap_phantom_new_install() {
        let path = Path::new("phantom-agent_0.6.2_amd64.snap");
        snap_phantom_install(path);
    }

    #[test]
    #[ignore]
    fn snap_phantom_old_install() {
        let path = Path::new("phantom-agent_0.4.2_amd64.snap");
        snap_phantom_install(path);
    }

    #[test]
    #[ignore]
    fn snap_info() {
        let path = "/home/yurikarasik/Downloads/phantom-agent_0.0.20_amd64.snap";
        let command = format!("snap info {}", path);
        let response = (BashExec::exec_arg)(&command, &[]).unwrap();
        println!("RESPONSE IS {}", &response);
        let re = Regex::new(r"name:\s*(.*)").unwrap();
        if let Some(cap) = re.captures_iter(&response).next() {
            println!("GOT A CAP WITH {} HITS", &cap.len());
            println!("CAP: {}", &cap[1]);
        };
    }

    #[test]
    #[ignore]
    fn snap_gps_full_cycle() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let top = current_dir().unwrap();
        let test_dir = top.join(Path::new("snap_gps_full_cycle_test_dir"));
        if test_dir.exists() { fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!"); }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        let remote_path = Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-SimCard-Gps-Info/3.0.4/amd64/sim-gps-info_3.0.4_amd64.snap").unwrap();
        let local_path = Path::new("sim-gps-info_3.0.4_amd64.snap");
        println!("Local path is {}", local_path.to_string_lossy());
        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager = LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            println!("Couldn't load the license: {}", err);
            println!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            assert!(false);
        }
        //println!("Token is: {}", license_manager.get_token());
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(), &license_manager.get_token().unwrap());
        //println!("Jfrog token is: {}", jfrog_token);
        let rest_comm = CouplingRestComm::new(&license_manager,
            |_method: SendType,
                   _url: &Url,
                   _body: Option<&Value>,
                   _authorization: Option<String>|
                -> Result<(String, u16), (String, u16)> { Ok(("[ ]".to_string(), 200)) });

        println!("Creating download manager");
        use crate::ota::service_control_trait::MockSystemControlTrait;
        let mock = MockSystemControlTrait::new();
        let update_ota_status = |_status, _message|{};
        let sys_mock = RefCell::new(mock);
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_ota_status).unwrap();

        println!("Creating manifest");
        let write_function = |_: &Path, _: &str| Ok(());
        let read_function = |_: &Path| Ok(String::from(r#"{}"#));
        let manifest =
            Manifest::new(false, PathBuf::from(""), Default::default(), Default::default(),
                          read_function, write_function).unwrap();
        //let target_dir = PathBuf::from("C://Program Files/OdenVR");
        let previous = test_dir.join("sim_gps_info");

        let component: Component = Component {
            component: "sim_gps_info".to_string(),
            checksum: "2bd0d152b25a2cf10a1dec90d7dcb15fbc1fa349968107c483c0284a6db94d96"
                .to_string(),
            updated: false,
            path: Some(test_dir.clone()),
            link: Some(remote_path),
            target_path: Some(test_dir.clone()),
            version: "3.0.4".to_string(),
            token: Some(jfrog_token.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(previous.clone()),
            processes: vec![],
        };

        let updated_manifest = manifest.update_single_component(&component).unwrap();
        let gps_component = updated_manifest
            .components
            .get(&ComponentType::sim_gps_info)
            .unwrap();
        assert!(!gps_component.updated);
        assert!(gps_component.path.is_some());
        println!("Manifest updated, downloading component...");
        let _updated_manifest = download_manager.run(updated_manifest).unwrap();
        assert!(test_dir.join(local_path).exists());

        let component: Component = Component {
            updated: false,
            path: Some(test_dir.join(local_path)),
            target_path: Some(test_dir.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(test_dir.join("sim_gps_info")),
            ..
            Component::empty()
        };

        SnapInstaller::install(&component, exec).unwrap();

        file_utils::create_dir_if_not_exists(&previous);
        fs::copy(test_dir.join(local_path), previous.join(local_path)).expect("failed to copy");
        let name = SnapInstaller::extract_snap_name(&previous.join(local_path), exec).unwrap();
        println!("Uninstalling {}", name);

        SnapInstaller::uninstall(&component, exec).unwrap();

        fs::remove_dir_all(&test_dir).expect("Failed to clear temp dir!");
    }

    #[test]
    #[ignore]
    fn snap_status() {
        set_logging_for_tests(log::LevelFilter::Info);
        //let path = PathBuf::from("/home/yurikarasik/Downloads/phantom-agent_1.2.0_amd64.snap");
        //if let Ok(snap_name) = SnapInstaller::extract_snap_name(&path, BashExec::exec_arg) {
        let snap_name = "vapp-translator".to_string();
        println!("SNAP NAME is {}", &snap_name);
        if let Ok(enabled) = SnapInstaller::extract_snap_enabled(&snap_name, BashExec::exec_arg) {
            println!("v ENABLED {}", enabled);
            if enabled {
                SnapInstaller::snap_disable(&snap_name, BashExec::exec_arg).unwrap();
            } else {
                SnapInstaller::snap_enable(&snap_name, BashExec::exec_arg).unwrap();
            }
            if let Ok(enabled) = SnapInstaller::extract_snap_enabled(&snap_name, BashExec::exec_arg) {
                println!("^ ENABLED {}", enabled);
            }
        }
    }

    #[test]
    #[ignore]
    fn cleanup_deprecated() {
        set_logging_for_tests(log::LevelFilter::Info);
        SnapInstaller::cleanup_deprecated_if_needed();
    }

    #[test]
    #[ignore]
    fn snap_full_cycle() {
        set_logging_for_tests(log::LevelFilter::Info);

        let test_dir = current_dir().unwrap().join(Path::new("snap_full_cycle_test_dir"));
        if test_dir.exists() { fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!"); }
        fs::create_dir(&test_dir).expect("Failed to create dir!");

        let path1 = PathBuf::from("/home/yurikarasik/Downloads/phantom-agent_1.2.0_amd64.snap");
        let path2 = PathBuf::from("/home/yurikarasik/Downloads/phantom-agent_1.3.0_amd64.snap");

        let component: Component = Component {
            updated: false,
            path: Some(path1.clone()),
            target_path: Some(test_dir.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(test_dir.join("phantom_agent")),
            ..
            Component::empty()
        };

        SnapInstaller::install(&component, BashExec::exec_arg).unwrap();

        if let Ok(snap_name) = SnapInstaller::extract_snap_name(&path1, BashExec::exec_arg) {
            println!("SNAP NAME is {}", &snap_name);
            if let Ok(enabled) = SnapInstaller::extract_snap_enabled(&snap_name, BashExec::exec_arg) {
                println!("v ENABLED {}", enabled);
                if enabled {
                    SnapInstaller::snap_disable(&snap_name, BashExec::exec_arg).unwrap();
                    if let Ok(enabled) = SnapInstaller::extract_snap_enabled(&snap_name, BashExec::exec_arg) {
                        println!("^ ENABLED {}", enabled);
                    }
                }

            }
        }

        let component: Component = Component {
            updated: false,
            path: Some(path2),
            target_path: Some(test_dir.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(test_dir.join("phantom_agent")),
            ..
            Component::empty()
        };
        log::info!("Now installing new component over disabled one!");
        SnapInstaller::install(&component, BashExec::exec_arg).unwrap();
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::ota::snap_installer::SnapInstaller;
    use crate::utils::bash_exec::BashExec;
    use std::path::{Path, PathBuf};
    use std::string::String;
    use crate::Component;

    #[test]
    #[cfg(windows)]
    fn test_strip() {
        use std::path::PathBuf;
        let path = Path::new("c:\\Users\\Alex\\Downloads\\phau-core_2.0.34_amd64.snap");
        let path = SnapInstaller::unixify_prefix(path);
        assert_eq!(
            path,
            PathBuf::from("c\\Users\\Alex\\Downloads\\phau-core_2.0.34_amd64.snap")
        );
    }

    #[test]
    #[cfg(windows)]
    #[ignore]
    fn basic_windows() {
        let exec = |_cmd: &str, _args: &[&str]| {
            Ok(String::from(
                r#"{"type":"async","status-code":202,"status":"Accepted","result":null,"change":"2"}"#,
            ))
        };

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from("c:\\Users\\Alex\\Downloads\\phau-core_2.0.34_amd64.snap")),
            package_type: "snap".to_string(),
            ..
            Component::empty()
        };


        match SnapInstaller::install(&component, exec) {
            Err(error) => { println!("Error occurred: {error}"); }
            Ok(result) => { println!("result is {result}"); }
        }
    }

    #[test]
    #[cfg(windows)]
    #[ignore]
    fn live_windows() {
        let exec = BashExec::exec_arg;

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from("C:\\Program Files\\phantom_agent\\bin\\download\\phau-core_2.0.46_amd64.snap")),
            package_type: "snap".to_string(),
            ..
            Component::empty()
        };

        match SnapInstaller::install(&component, exec) {
            Err(error) => { println!("Error occurred: {error}"); }
            Ok(result) => { println!("result is {result}"); }
        }
    }

    #[test]
    fn decode_core_response() {
        let response = String::from(
            r#" {"success":true,"msg":{"type":"async","status-code":202,"status":"Accepted","result":null,"change":"10"},"core_version":"dev--snap_installer--alexl"}
"#,
        );
        let result = SnapInstaller::decode_core_response(&response).unwrap();
        assert_eq!(result, "Accepted");
    }

    #[test]
    fn decode_core_response_error() {
        let response = String::from(
            r#"{"success":false,"msg":"Error! Internal Server Error","core_version":"dev--snap_installer--alexl"}"#,
        );
        let result = SnapInstaller::decode_core_response(&response).unwrap_err();
        assert_eq!(result.message, "Error! Internal Server Error");
    }

    #[test]
    fn decode_core_response_empty() {
        let response = String::from("");
        let result = SnapInstaller::decode_core_response(&response).unwrap_err();
        assert!(result.message.starts_with("EOF while parsing a value"));
    }

    #[cfg(unix)]
    #[test]
    #[ignore]
    fn snap_fake_plugin_install_full_cycle() {
        use std::env::current_dir;
        use std::cell::RefCell;
        use std::fs;
        use url::Url;
        use crate::auth::license_manager::LicenseManager;
        use crate::auth::license_manager_trait::LicenseManagerTrait;
        use crate::ota::download_manager::DownloadManager;
        use crate::ota::manifest::{ComponentType, Manifest};
        use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
        use crate::rest_request::{RestServer, SendType, extract_token};
        use crate::utils::file_utils;
        use crate::utils::log_utils::set_logging_for_tests;

        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let top = current_dir().unwrap();
        let test_dir = top.join(Path::new("snap_fake_plugin_install_test"));
        if test_dir.exists() { fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!"); }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        let remote_path = Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-SimCard-Gps-Info/3.0.4/amd64/sim-gps-info_3.0.4_amd64.snap").unwrap();
        let local_path = Path::new("sim-gps-info_3.0.4_amd64.snap");
        println!("Local path is {}", local_path.to_string_lossy());
        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager =
            LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            println!("Couldn't load the license: {}", err);
            println!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            assert!(false);
        }

        let license_token = license_manager
            .get_token(RestServer::get, RestServer::post)
            .unwrap();
        println!("Token is: {}", license_token);
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(), license_token.as_str());
        println!("Jfrog token is: {}", jfrog_token);

        let rest_comm = CouplingRestComm {
            url: license_manager.get_url().unwrap(),
            token: license_token.clone(),
            send: |_method: SendType,
                   _url: &Url,
                   _body: &serde_json::Value,
                   _authorization: Option<String>|
                -> Result<(String, u16), (String, u16)> { Ok(("[ ]".to_string(), 200)) },
        };

        println!("Creating download manager");
        use crate::ota::service_control_trait::MockSystemControlTrait;
        let mock = MockSystemControlTrait::new();
        let sys_mock = RefCell::new(mock);
        let update_ota_status =  |_ota_status, _message |{};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_ota_status).unwrap();

        println!("Creating manifest");
        let write_function = |_: &Path, _: &str| Ok(());
        let read_function = |_: &Path| Ok(String::from(r#"{}"#));
        let manifest =
            Manifest::new(true, PathBuf::from(""), Default::default(),
                          read_function, write_function).unwrap();

        //log::info!("\n\n\n=== MANIFEST ===\n{:?}\n=== MANIFEST ===\n", manifest.components);
        let previous = test_dir.join("oden_plugin");

        let component: Component = Component {
            component: "oden_plugin".to_string(),
            checksum: "2bd0d152b25a2cf10a1dec90d7dcb15fbc1fa349968107c483c0284a6db94d96"
                .to_string(),
            updated: false,
            path: Some(test_dir.clone()),
            link: Some(remote_path),
            target_path: Some(test_dir.clone()),
            version: "3.0.4".to_string(),
            token: Some(jfrog_token.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(previous.clone()),
            processes: vec![],
        };

        let updated_manifest = manifest.update_single_component(&component).unwrap();
        log::info!("\n\n\n=== MANIFEST ===\n{:?}\n=== MANIFEST ===\n", updated_manifest.components);
        let oden_plugin_component = updated_manifest
            .components
            .get(&ComponentType::oden_plugin)
            .unwrap();
        assert!(!oden_plugin_component.updated);
        assert!(oden_plugin_component.path.is_some());
        println!("Manifest updated, downloading component...");
        let _updated_manifest = download_manager.run(updated_manifest).unwrap();
        assert!(test_dir.join(local_path).exists());

        let component: Component = Component {
            updated: false,
            path: Some(test_dir.join(local_path)),
            target_path: Some(test_dir.clone()),
            package_type: "snap".to_string(),
            previous_install_path: Some(test_dir.join("oden_plugin")),
            ..
            Component::empty()
        };

        SnapInstaller::install(&component, exec).unwrap();

        file_utils::create_dir_if_not_exists(&previous);
        fs::copy(test_dir.join(local_path), previous.join(local_path)).expect("failed to copy");
        let name = SnapInstaller::extract_snap_name(&previous.join(local_path), exec).unwrap();
        println!("Uninstalling {}", name);
        SnapInstaller::uninstall(&component, exec).unwrap();
        fs::remove_dir_all(&test_dir).expect("Failed to clear temp dir!");
    }

}
