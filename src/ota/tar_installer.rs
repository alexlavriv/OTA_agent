use crate::{BashExec, create_dir_if_not_exists};
use crate::ota::ota_error::OTAError;
use std::path::PathBuf;
use std::{fs, str};
use std::env::temp_dir;
use std::string::String;
use crate::ota::manifest::Component;
use crate::utils::file_utils::get_sha1_checksum;
use crate::utils::color::Coloralex;
use crate::utils::bash_exec::ExecArgType;

pub struct TarInstaller;

impl TarInstaller {
    pub fn install_tar(component: &Component, exec_command: ExecArgType)
        -> Result<String, OTAError> { TarInstaller::install(component, false, exec_command) }

    pub fn install_zip(component: &Component, exec_command: ExecArgType)
        -> Result<String, OTAError> { TarInstaller::install(component, true, exec_command) }

    pub fn uninstall_tar(component: &Component, exec_command: ExecArgType)
        -> Result<String, OTAError> { TarInstaller::uninstall(component, false, exec_command) }

    pub fn uninstall_zip(component: &Component, exec_command: ExecArgType)
        -> Result<String, OTAError> { TarInstaller::uninstall(component, true, exec_command) }

    pub fn install(
        component: &Component,
        zipped: bool,
        exec_command: ExecArgType,
    ) -> Result<String, OTAError> {
        let path = component.path.clone().unwrap_or_default();
        if !path.exists() {
            return Err(OTAError::nonfatal(format!(
                "{} not found",
                path.to_str().unwrap()
            )));
        }
        let target_path = component.target_path.clone().unwrap_or_default();

        let mut path_str = path.to_str().unwrap().to_string();
        if !path.is_absolute() {
            path_str = format!("./{path_str}");
        }
        log::info!("Installing {}", path.to_string_lossy());
        let mut target_path_str = target_path.to_str().unwrap().to_string();
        if !target_path.is_absolute() {
            target_path_str = format!("./{target_path_str}");
        }
        create_dir_if_not_exists(&target_path);
        let files: Vec<String> = BashExec::list_files_in_archive(path.clone(), exec_command).map_err(OTAError::nonfatal)?;
        let flags = if zipped { "-xzf" } else { "-xf" };

        for file in &files { // Checking whether is the new (multiple clients) version paths
            if file.contains("Program Files") {
                target_path_str = "C:/".to_string();
                break;
            }
        }

        let target_path = PathBuf::from(target_path_str.clone()); // Syncing the target_path to match
        let temp_dir = temp_dir().join(format!("TMP_ARCHIVE_{}", component.component));
        if temp_dir.exists() { fs::remove_dir_all(temp_dir.clone()).expect("Failed to clear temp dir"); }
        fs::create_dir(temp_dir.clone()).expect("Failed to create temp dir");
        log::info!("TEMP DIR: Unpacking {} to {}", path.to_string_lossy(), temp_dir.to_string_lossy());
        (exec_command)("tar", &[flags, &path_str, "-C", &temp_dir.to_string_lossy()]).expect("Failed to execute command!");
        log::info!("Unpacking {} to {}", path.to_string_lossy(), target_path.to_string_lossy());
        if let Err(e) = (exec_command)("tar", &[flags, &path_str, "-C",  &target_path_str]) {
            let message = format!("Failed to execute command: {}", e);
            if e.contains("Can't unlink") {
                return Err(OTAError::nonfatal(message));
            }
            else {
                panic!("{}", message);
            }
        }

        // Additionally checking whether we can see all the listed unpacked files
        for file in files {
            let file_path = target_path.join(PathBuf::from(file.clone()));
            let temp_path = temp_dir.join(PathBuf::from(file.clone()));
            if !file_path.exists() {
                log::error!("Cannot find unpacked: {}", file_path.to_string_lossy());
                fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
                return Err(OTAError::nonfatal("Failure".to_string()));
            }
            if !temp_path.exists() {
                log::error!("Cannot find unpacked: {}", temp_path.to_string_lossy());
                fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
                return Err(OTAError::nonfatal("Failure".to_string()));
            }
            let file_checksum =  match get_sha1_checksum(&file_path) {
                Ok(checksum) => { checksum }
                Err(e) => {
                    fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
                    return Err(OTAError::nonfatal(e));
                }
            };
            let temp_checksum =  match get_sha1_checksum(&temp_path) {
                Ok(checksum) => { checksum }
                Err(e) => {
                    fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
                    return Err(OTAError::nonfatal(e));
                }
            };
            if file_checksum == temp_checksum {
                let message = format!("{} integrity verified: {}", file, file_checksum).green(false);
                log::info!("{}", message);
            }
            else {
                let message = format!("{} integrity failed: expected {} but file has {}", file, file_checksum, temp_checksum).red(false);
                log::info!("{}", message);
                fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
                return Err(OTAError::nonfatal(message));
            }
        }
        log::info!("{}", "All content unpacked successfully".green(true));
        fs::remove_dir_all(temp_dir).expect("Failed to remove temp dir!");
        Ok("Success".to_string())
    }

    pub fn uninstall(
        component: &Component,
        zipped: bool,
        exec_command: ExecArgType,
    ) -> Result<String, OTAError> {
        let installed_path = match component.target_path.clone() {
            None => { return Ok("No installed dir path".to_string()); }
            Some(dir ) => { dir }
        };

        if !installed_path.exists() {
            return Ok(format!("Nothing is installed at {}", installed_path.to_str().unwrap()));
        }

        let previous_dir = match &component.previous_install_path {
            None => { return Ok("No previous install path".to_string()); }
            Some(dir) => { dir }
        };

        if !previous_dir.exists() || !previous_dir.is_dir() {
            return Ok("Previous uninstall directory doesn't exist".to_string());
        }

        let count = std::fs::read_dir(previous_dir).unwrap().count();
        if count != 1 {
            return Ok(format!("Expected 1 files, but found {} files", count));
        }

        let archive_path = std::fs::read_dir(previous_dir).unwrap().next().unwrap().unwrap().path();
        if !archive_path.exists() {
            return Ok(format!("Uninstall target {} not found", archive_path.to_str().unwrap()));
        }

        // Creating a list of what the archive contains
        let list_flags = if zipped { "tzf" } else { "tf" };
        let command = format!("tar -{}", list_flags);
        let text = match (exec_command)(&command, &[&archive_path.to_string_lossy()]) {
            Ok(text) => text,
            Err(e) => { return Err(OTAError::nonfatal(format!("Couldn't list the archive: {}", e))); }
        };

        let files: Vec<&str> = text.lines().collect();
        for file in files {
            let installed_file_path = installed_path.join(file);
            if !installed_file_path.exists() {
                log::warn!("Cannot find installed: {}", installed_file_path.to_string_lossy());
            } else {
                match fs::remove_file(installed_file_path.clone()) {
                    Ok(_) => { log::info!("Removed {}", installed_file_path.to_string_lossy()); }
                    Err(e) => { log::warn!("Could not remove {}: {}", installed_file_path.to_string_lossy(), e); }
                }
            }
        }
        log::info!("Uninstall finished");
        Ok("Success".to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::license_manager::LicenseManager;
    use crate::auth::license_manager_trait::LicenseManagerTrait;
    use crate::ota::download_manager::DownloadManager;
    use crate::ota::manifest::{Component, ComponentType, Manifest};
    use crate::ota::tar_installer::TarInstaller;
    use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
    use crate::rest_request::{RestServer, SendType, extract_token};
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::utils::bash_exec::BashExec;
    use serde_json::Value;
    use std::cell::RefCell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use url::Url;

    #[test]
    #[ignore]
    fn tar_oden_full_install() {
        let exec = BashExec::exec_arg;
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("tar_oden_install_test_dir"));
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");

        let remote_path = {
            #[cfg(unix)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/amd64/phantom-plugin_amd64-3.2.1.tar.gz").unwrap() }
            #[cfg(windows)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/win/phantom_plugin_win.zip").unwrap() }
        };
        let local_path = {
            #[cfg(unix)] { test_dir.join("phantom-plugin_amd64-3.2.1.tar.gz") }
            #[cfg(windows)] { test_dir.join("phantom_plugin_win.zip") }
        };

        println!("Local path is {}", local_path.to_string_lossy());
        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager =
            LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            println!("Couldn't load the license: {}", err);
            println!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            assert!(false);
        }

        let license_token = LicenseManager::get_token(&license_manager, RestServer::get, RestServer::post).unwrap();
        //println!("Token is: {}", license_token);
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(),license_token.as_str());
        //println!("Jfrog token is: {}", jfrog_token);

        let rest_comm = CouplingRestComm::new(&license_manager, |_method: SendType,
                   _url: &Url,
                   _body: Option<&Value>,
                   _authorization: Option<String>|
                   -> Result<(String, u16), (String, u16)> { Ok(("[ ]".to_string(), 200)) });

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
                          license_manager.get_server().unwrap_or_default(), read_function, write_function).unwrap();
        //let target_dir = PathBuf::from("C://Program Files/OdenVR");
        let target_dir = test_dir.join("target_dir");

        let component: Component = Component {
            component: "oden_plugin".to_string(),
            checksum: "89d9a94a717e4092eb064e95e67525ffe58339c16db8bbd1b89f24becac34ad3"
                .to_string(),
            updated: false,
            path: Some(test_dir.clone()),
            link: Some(remote_path),
            target_path: Some(target_dir.clone()),
            version: "0.1.2".to_string(),
            token: Some(jfrog_token.clone()),
            package_type: "tar".to_string(),
            previous_install_path: Some(target_dir.clone().join("oden_plugin")),
            processes: vec![],
        };

        let updated_manifest = manifest.update_single_component(&component).unwrap();
        let oden_component = updated_manifest
            .components
            .get(&ComponentType::oden_plugin)
            .unwrap();
        assert!(!oden_component.updated);
        assert!(oden_component.path.is_some());
        println!("Manifest updated, downloading component...");
        let _updated_manifest = download_manager.run(updated_manifest).unwrap();
        assert!(local_path.exists());
        let zipped = {
            #[cfg(unix)] { false }
            #[cfg(windows)] { true }
        };
        let result_path = {  // what the archive contains
            #[cfg(unix)] { target_dir.join(Path::new("libphantom_plugin.so")) }
            #[cfg(windows)] { target_dir.join(Path::new("phantom_plugin.dll")) }
        };

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(local_path)),
            target_path: Some(PathBuf::from(target_dir.clone())),
            package_type: "tar".to_string(),
            previous_install_path: Some(target_dir.join("oden_plugin")),
            ..
            Component::empty()
        };

        TarInstaller::install(&component, zipped, exec).unwrap();
        assert!(result_path.exists());

        fs::remove_dir_all(&test_dir).expect("Failed to clear temp dir!");
    }


    #[test]
    #[ignore]
    fn mocked_install_with_verify() {
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("tar_oden_install_test_dir"));
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");

        let remote_path = {
            #[cfg(unix)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/amd64/phantom-plugin_amd64-3.2.1.tar.gz").unwrap() }
            #[cfg(windows)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/win/phantom_plugin_win.zip").unwrap() }
        };
        let local_path = {
            #[cfg(unix)] { test_dir.join("phantom-plugin_amd64-3.2.1.tar.gz") }
            #[cfg(windows)] { test_dir.join("phantom_plugin_win.zip") }
        };

        println!("Local path is {}", local_path.to_string_lossy());
        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager =
            LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            println!("Couldn't load the license: {}", err);
            println!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            assert!(false);
        }

        let license_token = LicenseManager::get_token(&license_manager, RestServer::get, RestServer::post).unwrap();
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(),license_token.as_str());
        let mut rest_mock = MockCouplingRestSubmitter::new();
        rest_mock.expect_post_checksums().times(1).returning(|_| {
            #[cfg(unix)]
            {
                Ok(std::string::String::from(
                    r#"[
           {
              "token":"eyJhbGciOiJQUzI1NiIsImtpZCI6IjU2bEwtWm1NM0tzel9HTXgzS1ViSCJ9.eyJpc3MiOiJxYS5waGFudG9tYXV0by5kZXYvIiwiYXVkIjpbInFhLnBoYW50b21hdXRvLmRldi9jb3VwbGluZyIsInFhLnBoYW50b21hdXRvLmRldi9wcm94eSJdLCJzdWIiOiI2NDA3NTgzNjAzMzVhMTdkYzdhNzA3NjgiLCJleHAiOjE2ODE0NzUxMjAsImlhdCI6MTY4MTM4ODcyMH0.LeNfLz0TAveALR4QLZcaS9_IwE0DKzRfPXuUEkbBXaWh3cdIEg9M10ESj6ngva9kJDU3-oxrxrWuxudcd3S_nX6I1wIH5sZm28Aj6Y0i9wN9ViJs2mlcAW_9leIDrDoeL7iqCWUQEEdQ8LkwZgrJoxStpnLL6QdeyZ2D4p_VM7BxrJVA-2U01MiwJoOOvVymPJ697lj3j0DlC6YVJksVYsbLQGtE4N4czwN0EMjm5Wr7gH8GJ0tDpXecO0vLXTvMZVfT4qone-RZztiZg6TITDOAxP5gS08a8zM6f6Hxw9jqn71V2tGYx-I9S2PECOg_isr1gP9XfUW2BrljRTrTRQ",
              "_id":"6208c6e48cff329d56642878",
              "component":"oden_plugin",
              "link":"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/amd64/phantom-plugin_amd64-3.2.1.tar.gz",
              "checksum":"89d9a94a717e4092eb064e95e67525ffe58339c16db8bbd1b89f24becac34ad3",
              "version":"9.99.999.test",
              "arch":"AMD64"
           }
        ]"#,
                ))
            }

            #[cfg(windows)]
            {
            Ok(std::string::String::from(
                    r#"[
           {
              "token":"eyJhbGciOiJQUzI1NiIsImtpZCI6IjU2bEwtWm1NM0tzel9HTXgzS1ViSCJ9.eyJpc3MiOiJxYS5waGFudG9tYXV0by5kZXYvIiwiYXVkIjpbInFhLnBoYW50b21hdXRvLmRldi9jb3VwbGluZyIsInFhLnBoYW50b21hdXRvLmRldi9wcm94eSJdLCJzdWIiOiI2NDA3NTgzNjAzMzVhMTdkYzdhNzA3NjgiLCJleHAiOjE2ODE0NzUxMjAsImlhdCI6MTY4MTM4ODcyMH0.LeNfLz0TAveALR4QLZcaS9_IwE0DKzRfPXuUEkbBXaWh3cdIEg9M10ESj6ngva9kJDU3-oxrxrWuxudcd3S_nX6I1wIH5sZm28Aj6Y0i9wN9ViJs2mlcAW_9leIDrDoeL7iqCWUQEEdQ8LkwZgrJoxStpnLL6QdeyZ2D4p_VM7BxrJVA-2U01MiwJoOOvVymPJ697lj3j0DlC6YVJksVYsbLQGtE4N4czwN0EMjm5Wr7gH8GJ0tDpXecO0vLXTvMZVfT4qone-RZztiZg6TITDOAxP5gS08a8zM6f6Hxw9jqn71V2tGYx-I9S2PECOg_isr1gP9XfUW2BrljRTrTRQ",
              "_id":"6208c6e48cff329d56642878",
              "component":"oden_plugin",
              "link":"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/win/phantom_plugin_win.zip",
              "checksum":"89d9a94a717e4092eb064e95e67525ffe58339c16db8bbd1b89f24becac34ad3",
              "version":"9.99.999.test",
              "arch":"WIN"
           }
        ]"#,
                ))
            }
        });

        let remote_path_copy = remote_path.clone();
        let license_token_copy = license_token.clone();
        rest_mock.expect_get_url_and_token().times(1).returning(move || {
            (remote_path_copy.clone(), license_token_copy.clone())
        });

        println!("Creating download manager");
        use crate::ota::service_control_trait::MockSystemControlTrait;
        let mock = MockSystemControlTrait::new();
        let sys_mock = RefCell::new(mock);

        let update_ota_status = |_status, _message|{};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_mock, test_dir.clone(), update_ota_status).unwrap();

        println!("Creating manifest");
        let write_function = |_: &Path, _: &str| Ok(());
        let read_function = |_: &Path| Ok(String::from(r#"{}"#));
        let manifest = Manifest::new(true, test_dir.clone(), Default::default(),
                          license_manager.get_server().unwrap_or_default(), read_function, write_function).unwrap();
        let target_dir = test_dir.join("target_dir");

        let component: Component = Component {
            component: "oden_plugin".to_string(),
            checksum: "89d9a94a717e4092eb064e95e67525ffe58339c16db8bbd1b89f24becac34ad3"
                .to_string(),
            updated: false,
            path: Some(test_dir.clone()),
            link: Some(remote_path),
            target_path: Some(target_dir.clone()),
            version: "0.1.2".to_string(),
            token: Some(jfrog_token.clone()),
            package_type: "tar".to_string(),
            previous_install_path: Some(target_dir.clone().join("oden_plugin")),
            processes: vec![],
        };

        let updated_manifest = manifest.update_single_component(&component).unwrap();

        let oden_component = updated_manifest
            .components
            .get(&ComponentType::oden_plugin)
            .unwrap();
        assert!(!oden_component.updated);
        assert!(oden_component.path.is_some());
        println!("Manifest updated, downloading component...");
        let _updated_manifest = download_manager.run(updated_manifest).unwrap();
        assert!(local_path.exists());
        let zipped = {
            #[cfg(unix)] { false }
            #[cfg(windows)] { true }
        };
        let result_path = {  // what the archive contains
            #[cfg(unix)] { target_dir.join(Path::new("libphantom_plugin.so")) }
            #[cfg(windows)] { target_dir.join(Path::new("phantom_plugin.dll")) }
        };

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(local_path)),
            target_path: Some(PathBuf::from(target_dir.clone())),
            package_type: "tar".to_string(),
            previous_install_path: Some(target_dir.join("oden_plugin")),
            ..
            Component::empty()
        };

        TarInstaller::install(&component, zipped, exec).unwrap();
        assert!(result_path.exists());

        fs::remove_dir_all(&test_dir).expect("Failed to clear temp dir!");
    }

    #[test]
    #[ignore]
    fn tar_oden_untar_only() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("tar_oden_install_test_dir"));
        let local_path = {
            #[cfg(unix)] { test_dir.join("phantom-plugin_amd64.tar") }
            #[cfg(windows)] { test_dir.join("phantom_plugin_win.zip") }
        };

        println!("Local path is {}", local_path.to_string_lossy());
        #[cfg(windows)]
        let target_dir = PathBuf::from("C://Program Files/OdenVR");
        #[cfg(unix)]
        let target_dir = PathBuf::from("./");
        assert!(local_path.exists());
        let zipped = {
            #[cfg(unix)] { false }
            #[cfg(windows)] { true }
        };
        let result_path = { // what the archive contains
            #[cfg(unix)] { target_dir.join(Path::new("target/release/libphantom_plugin.so")) }
            #[cfg(windows)] { target_dir.join(Path::new("phantom_plugin.dll")) }
        };

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(local_path)),
            target_path: Some(PathBuf::from(target_dir.clone())),
            package_type: "tar".to_string(),
            previous_install_path: Some(target_dir.join("oden_plugin")),
            ..
            Component::empty()
        };

        TarInstaller::install(&component, zipped, exec).unwrap();
        assert!(result_path.exists());
    }
}
