use crate::ota::ota_error::OTAError;
use msi;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::str;
use std::str::FromStr;
use std::string::String;
use std::thread::sleep;
use std::time::Duration;
use std::env::temp_dir;
use winreg::enums::*;
use winreg::RegKey;
use single_instance::SingleInstance;
use crate::ota::manifest::{Component, ComponentType};
use crate::OTAStatus;
use crate::utils::file_utils::file_to_string;
use crate::utils::bash_exec::ExecArgType;

pub struct MsiInstaller;

impl MsiInstaller {
    fn verify_success_from_log_file(log_file: &Path, indicators: &[&str]) -> bool {
        let mut result = false;
        let text = file_to_string(log_file).unwrap();
        for line in text.lines() {
            for success in indicators {
                if line.contains(*success) {
                    log::info!(">>>{}", &line);
                    result = true;
                }
            }
        }
        result
    }

    // Creates links for a product about to be installed so it's visible to all users
    fn advertise(path: &Path, exec_command: ExecArgType) -> Result<(), String> {
        if !path.exists() {
            return Err(format!("{} not found", path.to_string_lossy()));
        }
        let path_str = path.to_string_lossy();
        let log_file = Path::new("advertise_log.txt");
        fs::remove_file(log_file).unwrap_or(());
        exec_command("msiexec /qn /Li", &[&log_file.to_string_lossy(), "REBOOT=R", "/jm", &path_str])?;
        if !log_file.exists() {
            return Err("Log file missing!".to_string());
        }
        let success = &[
            "Advertisement completed successfully",
            "re-advertisement of this product",
        ];
        if !MsiInstaller::verify_success_from_log_file(log_file, success) {
            return Err("Failed to validate advertising!".to_string());
        }
        fs::remove_file(log_file).expect("Error removing file!");
        log::info!("Advertising succeeded");
        Ok(())
    }

    // Installs an msi file, but first it will check backup and uninstall the previous setup if exists
    pub fn install(
        component: &Component,
        exec_command: ExecArgType,
        update_status: fn(OTAStatus, Option<String>),
    ) -> Result<String, OTAError> {
        let path = component.path.clone().unwrap_or_default();
        let success_str = "Installation succeeded".to_string();
        if !path.exists() {
            return Err(OTAError::nonfatal(format!(
                "{} not found",
                path.to_string_lossy()
            )));
        }

        let temp_dir = Self::log_dir();

        if let Ok((product_name, new_version, code)) = MsiInstaller::analyze_msi(&path) {
            log::info!(
                "Preparing to install {} version {} code {}",
                product_name,
                new_version,
                code
            );
            // Ensuring no other msiexec installation is running at the same time
            if !Self::ensure_msiexec_mutex() {
                let mut sec_timeout = 0;
                const TIMEOUT_SEC :i32 = 300;
                let component_type = ComponentType::from_str(&component.component).unwrap();
                update_status(OTAStatus::INSTALLING(component_type), Some("Waiting for another installation to complete".to_string()));
                while !Self::ensure_msiexec_mutex() {
                    if sec_timeout >= TIMEOUT_SEC {
                        log::error!("Failed to obtain msiexec mutex");
                        return Err(OTAError::nonfatal("Failed to obtain msiexec mutex".to_string()));
                    }
                    log::warn!("Another instance of msiexec is running, update will continue when it closes (will wait {} more seconds)", TIMEOUT_SEC - sec_timeout);
                    sleep(Duration::from_secs(5));
                    sec_timeout += 5
                }
                update_status(OTAStatus::INSTALLING(component_type), None);
            }

            let (prev_exists, prev_path) = component.uninstall_information();
            if prev_exists {
                log::info!("Found previous at ({}), uninstalling", prev_path.to_string_lossy());
                if let Err(e) = MsiInstaller::uninstall_from_file(&prev_path, &component.component, exec_command) {
                    log::warn!("Could not uninstall previous ({})", e);
                }
            }

            MsiInstaller::clean_up_registry_from_code(&code);

            match MsiInstaller::advertise(&path, exec_command){
                Ok(()) => log::info!("Advertise succeeded"),
                Err(error) => log::warn!("Advertise failed with error {error}")
            }
            sleep(Duration::from_secs(1));
            let path_str = path.to_string_lossy();
            let log_file = temp_dir.join(format!("install_log_{}.txt", component.component));
            let prev_log_file = temp_dir.join(format!("install_log_rb_{}.txt", component.component));
            fs::remove_file(log_file.clone()).unwrap_or(());
            fs::remove_file(prev_log_file.clone()).unwrap_or(());

            if let Err(e) = exec_command("msiexec /qn /L*V",
                                         &[&log_file.to_string_lossy(), "REBOOT=R", "/i", &path_str]) {
                if prev_exists {
                    log::error!("Install failed, rolling back...");
                    if exec_command("msiexec /qn /L*V",
                                    &[&prev_log_file.to_string_lossy(), "REBOOT=R", "/i", &prev_path.to_string_lossy()]).is_err() {
                        log::error!("Rollback failed...");
                    }
                    else { fs::remove_file(prev_log_file).unwrap_or(()); }
                }
                return Err(OTAError::nonfatal(e));
            }

            let ids = MsiInstaller::get_registry_ids(&product_name);
            for (_id, reg_version) in ids.clone() {
                if new_version != reg_version {
                    log::warn!(
                        "Version {} of {} still exists in the registry!",
                        reg_version,
                        product_name,
                    );
                }
            }
            for (_id, reg_version) in ids {
                if new_version == reg_version {
                    fs::remove_file(log_file).expect("Error removing file!");
                    log::info!("{}", success_str);
                    return Ok(success_str);
                }
            }

            let error_str = "Failed to validate installation!".to_string();
            log::error!("{}", error_str);
            if prev_exists {
                log::error!("Rolling back...");
                let prev_code = match MsiInstaller::analyze_msi(&prev_path) {
                    Ok((_, _, prev_code)) => { prev_code }
                    Err(()) => { "".to_string() }
                };
                MsiInstaller::clean_up_registry_from_code(&prev_code);
                match MsiInstaller::advertise(&prev_path, exec_command){
                    Ok(()) => log::info!("Advertise succeeded"),
                    Err(error) => log::warn!("Advertise failed with error {error}")
                }
                sleep(Duration::from_secs(1));
                if exec_command("msiexec /qn /L*V",
                                &[&prev_log_file.to_string_lossy(), "REBOOT=R", "/i", &prev_path.to_string_lossy()]).is_err() {
                    log::error!("Rollback failed...");
                }
                else { fs::remove_file(prev_log_file).unwrap_or(()); }
            }
            Err(OTAError::nonfatal(error_str))
        } else {
            let error_str = format!(
                "Could not extract name from msi file {}",
                path.to_string_lossy()
            );
            log::error!("{}", error_str);
            Err(OTAError::nonfatal(error_str))
        }
    }

    // Uninstalls an installed product by its id in the Windows Registry e.g. {7B1A4E6D-049C-4165-AFD6-F997702EB7E6}
    pub fn uninstall_by_id(
        id: &str, exec_command: ExecArgType,
    ) -> Result<(), String> {
        let temp_dir = Self::log_dir();
        let log_file = temp_dir.join(format!("uninstall_log_{}.txt", id));
        fs::remove_file(log_file.clone()).unwrap_or(());
        exec_command("msiexec /qn /L*V", &[&log_file.to_string_lossy(), "REBOOT=R", "/x", id])?;
        if !log_file.exists() {
            return Err("Log file missing!".to_string());
        }
        let success = &[
            "Removal completed successfully",
            "This action is only valid for products that are currently installed",
        ];
        if !MsiInstaller::verify_success_from_log_file(&log_file, success) {
            return Err("Failed to validate uninstallation!".to_string());
        }
        fs::remove_file(log_file).expect("Error removing file!");
        log::info!("Removal succeeded");
        Ok(())
    }

    pub fn uninstall_from_file(
        path: &Path,
        name: &str,
        exec_command: ExecArgType,
    ) -> Result<(), String> {
        let path_str = path.to_string_lossy();
        let temp_dir = Self::log_dir();
        let log_file = temp_dir.join(format!("uninstall_log_{}.txt", name));
        fs::remove_file(log_file.clone()).unwrap_or(());
        exec_command("msiexec /qn /L*V",
                     &[&log_file.to_string_lossy(), "REBOOT=R", "/x", &path_str])?;
        if !log_file.exists() {
            return Err("Log file missing!".to_string());
        }
        let success = &[
            "Removal completed successfully",
            "This action is only valid for products that are currently installed",
        ];

        if !MsiInstaller::verify_success_from_log_file(&log_file, success) {
            return Err("Failed to validate uninstallation!".to_string());
        }
        fs::remove_file(log_file).expect("Error removing file!");
        log::info!("Removal succeeded");
        Ok(())
    }

    pub fn uninstall(
        component: &Component,
        exec_command: ExecArgType,
    ) -> Result<String, OTAError> {
        let path = component.path.clone().unwrap_or_default();
        let success_str = "Uninstallation succeeded".to_string();
        if !path.exists() {
            return Err(OTAError::nonfatal(format!(
                "{} not found",
                path.to_string_lossy()
            )));
        }
        match MsiInstaller::uninstall_from_file(&path, &component.component, exec_command) {
            Ok(()) => { Ok(success_str) }
            Err(e) => { Err(OTAError::nonfatal(e)) }
        }
    }

    fn transform_guid_to_product_code(guid: String) -> String {
        let guid: Vec<char> = guid.replace(['-','{','}'],"").chars().collect();
        let reorder = [7,6,5,4,3,2,1,0,11,10,9,8,15,14,13,12,17,16,19,18,21,20,23,22,25,24,27,26,29,28,31,30];
        if guid.len() != 32 {
            return "".to_string()
        }
        let mut code: Vec<char> = Vec::new();
        for i in 0..32 {
            code.push(guid[reorder[i]]);
        }
        code.into_iter().collect()
    }

    // Takes an msi file and returns the tuple of (NAME, VERSION, CODE) of the installation package
    fn analyze_msi(path: &Path) -> Result<(String, String, String), ()> {
        let mut package = match msi::open(path) {
            Ok(package) => package,
            Err(e) => {
                log::error!("{}", e.to_string());
                return Err(());
            }
        };

        let query = msi::Select::table("Property").columns(&["Property", "Value"]);
        let rows = package.select_rows(query).unwrap();
        let mut name = String::from("Unknown");
        let mut version = String::from("Unknown");
        let mut code = String::from("Unknown");
        for row in rows {
            let property = row["Property"].as_str().unwrap();
            if property == "ProductName" {
                name = row.clone()["Value"].as_str().unwrap().to_string();
            }
            if property == "ProductVersion" {
                version = row.clone()["Value"].as_str().unwrap().to_string();
            }
            if property == "ProductCode" {
                //A6C9DC59-2028-4C27-8BCE-B24890910CF7
                //95CD9C6A820272C4B8EC2B840919C07F
                code = row.clone()["Value"].as_str().unwrap().to_string();
                println!("code is {}", code);
                code = Self::transform_guid_to_product_code(code);
            }
        }
        Ok((name, version, code))
    }

    // Takes a product name and returns a vector of [(REGISTRY ID, VERSION)] registry uninstall info (can be empty)
    pub fn get_registry_ids(product_name: &str) -> Vec<(String, String)> {
        let mut result: Vec<(String, String)> = Vec::new();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let uninst = hklm
            .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
            .unwrap();
        for subkey_name in uninst.enum_keys().map(|x| x.unwrap()) {
            let subkey = uninst.open_subkey(subkey_name.clone()).unwrap();
            if let Ok(name) = subkey.get_value::<String, &str>("DisplayName") {
                if name == product_name {
                    let version = match subkey.get_value::<String, &str>("DisplayVersion") {
                        Ok(version) => version,
                        Err(_) => String::from("0.0.0"),
                    };
                    result.push((subkey_name, version));
                }
            }
        }
        result
    }

    fn clean_up_registry_from_code(code: &str) {
        if code.is_empty() || code == "Unknown" {
            log::warn!("Clean up called with empty code");
            return;
        }
        let code_path = format!("SOFTWARE\\Classes\\Installer\\Products\\{}", code);
        let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
        match machine.delete_subkey_all(code_path) {
            Ok(()) => { log::info!("Cleaned up registry for {}", code); }
            Err(e) => { log::info!("No need to clean up registry for {}: {}", code, e); }
        }
    }

    fn ensure_msiexec_mutex() -> bool {
        let instance = SingleInstance::new("Global\\_MSIExecute").expect("failed to access mutex");
        instance.is_single()
    }

    pub fn log_dir() -> PathBuf {
        let temp_dir = temp_dir().join("PHANTOM_MSI_INSTALLER");
        if !temp_dir.exists() { fs::create_dir(temp_dir.clone()).expect("Failed to create temp dir"); }
        temp_dir
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use std::cell::RefCell;
    use std::env::current_dir;
    use crate::ota::msi_installer::MsiInstaller;
    use crate::utils::bash_exec::BashExec;
    use crate::utils::log_utils::set_logging_for_tests;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread::sleep;
    use std::time::Duration;
    use serde_json::Value;
    use tokio::process::Command;
    use url::Url;
    use crate::auth::license_manager::LicenseManager;
    use crate::auth::license_manager_trait::LicenseManagerTrait;
    use crate::{Component};
    use crate::ota::download_manager::DownloadManager;
    use crate::ota::manifest::{ComponentType, Manifest};
    use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
    use crate::rest_request::{RestServer, SendType, extract_token};
    use crate::utils::file_utils;

    #[tokio::main]
    pub async fn download_package(remote_path: &Path, local_path: &Path) -> Result<String, String> {
        println!(
            "Downloading package... {} into {}",
            remote_path.to_str().unwrap(),
            local_path.to_str().unwrap()
        );
        let mut input = Command::new("curl");
        input
            .arg(remote_path.to_str().unwrap())
            .arg("--output")
            .arg(local_path.to_str().unwrap());
        let output = input.output().await.expect("Error!");
        let out_code = output.status.code().unwrap();
        if out_code != 0 {
            return Err("Failed to download package!".to_string());
        }
        Ok("Download success".to_string())
    }

    #[test]
    #[ignore]
    fn msi_oden_install() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let remote_path = Path::new("https://releases.voysys.dev/2.28.9/OdenVR_2.28.9.msi");
        let local_path = Path::new("OdenVR_2.28.9.msi");
        println!("Downloading component");
        download_package(remote_path, local_path).expect("couldn't download component");
        println!("Installing component");
        let component: Component = Component {
            component: "oden_test".to_string(),
            updated: false,
            path: Some(PathBuf::from(local_path)),
            package_type: "msi".to_string(),
            ..
            Component::empty()
        };
        MsiInstaller::install(&component, exec, |_,_|{} ).expect("Install failed!");
        fs::remove_file(local_path).expect("Error removing file!")
    }

    #[test]
    #[ignore]
    fn msi_oden_uninstall_from_file() {
        let exec = BashExec::exec_arg;
        let remote_path = Path::new("https://releases.voysys.dev/2.22.1/OdenVR_2.22.1.msi");
        let local_path = Path::new("OdenVR_2.22.1.msi");
        download_package(remote_path, local_path).unwrap();

        MsiInstaller::uninstall_from_file(local_path, "oden", exec).unwrap();
        fs::remove_file(local_path).expect("Error removing file!");
    }

    #[tokio::main]
    async fn check_oden_version() -> Result<String, ()> {
        let oden_path = Path::new("C:/Program Files/OdenVR/OdenVR.exe");
        if !oden_path.exists() {
            return Err(());
        }
        let mut command = Command::new("C:/Program Files/OdenVR/OdenVR.exe");
        command.arg("--version");
        let result = command.output().await;
        let output = result.expect("Command failed to execute!");
        if !output.status.success() {
            return Err(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut lines = stdout.lines();
        let version = lines.next().unwrap().split(' ').nth(1).unwrap().to_string();
        Ok(version)
    }

    #[test]
    #[ignore]
    fn msi_oden_downgrade() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let high_remote_path = Path::new("https://releases.voysys.dev/2.26.0/OdenVR_2.26.0.msi");
        let high_local_path = Path::new("OdenVR_2.26.0.msi");
        if high_local_path.exists() {
            println!("High component located");
        } else {
            println!("Downloading high component");
            download_package(high_remote_path, high_local_path)
                .expect("couldn't download component");
        }
        let low_remote_path = Path::new("https://releases.voysys.dev/2.24.11/OdenVR_2.24.11.msi");
        let low_local_path = Path::new("OdenVR_2.24.11.msi");
        if low_local_path.exists() {
            println!("Low component located");
        } else {
            println!("Downloading low component");
            download_package(low_remote_path, low_local_path).expect("couldn't download component");
        }

        let (name, version, code) = MsiInstaller::analyze_msi(high_local_path).unwrap();
        println!("HIGH: {name}, {version}, {code}");
        let (name, version, code) = MsiInstaller::analyze_msi(low_local_path).unwrap();
        println!("LOW: {name}, {version}, {code}");

        let ids = MsiInstaller::get_registry_ids("OdenVR");

        if !ids.is_empty() {
            println!("Old versions of OdenVR found ({version}), uninstalling...");
            for (id, _version) in ids {
                MsiInstaller::uninstall_by_id(&id, exec).expect("Uninstall failed!");
            }
        }
        let version = check_oden_version();
        assert_eq!(version.unwrap_err(), ());
        println!("Uninstall was a success, no version detected");

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(high_local_path)),
            package_type: "msi".to_string(),
            ..
            Component::empty()
        };
        println!("Installing high version...");
        MsiInstaller::install(&component, exec.clone(), |_,_|{}).expect("Install failed!");
        let version = check_oden_version().unwrap();
        assert_eq!(version, "2.26.0");
        println!("Install was a success, version is now {}", version);

        let prev_dir = Path::new("msi_oden_downgrade_prev");
        if prev_dir.exists() { fs::remove_dir_all(&prev_dir).expect("Failed to remove prev dir!"); }
        fs::create_dir(&prev_dir).expect("Failed to create dir!");
        let prev_file = prev_dir.join(high_local_path);
        fs::copy(&high_local_path, &prev_file).expect("failed to copy");

        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(low_local_path)),
            package_type: "msi".to_string(),
            previous_install_path: Some(PathBuf::from(prev_dir)),
            ..
            Component::empty()
        };
        println!("Installing low version...");
        MsiInstaller::install(&component, exec, |_,_|{}).expect("Install failed!");
        let version = check_oden_version().unwrap();
        assert_eq!(version, "2.24.11");
        println!("Reinstall was a success, version is now {version}");
        fs::remove_dir_all(&prev_dir).expect("Error removing dir!");
        fs::remove_file(high_local_path).expect("Error removing file!");
        fs::remove_file(low_local_path).expect("Error removing file!");
    }

    #[test]
    #[ignore]
    fn test_check_oden_version() {
        match check_oden_version() {
            Err(()) => {
                println!("Oden is not installed");
            }
            Ok(version) => {
                println!("Oden is installed ({version})");
            }
        }
    }

    #[test]
    #[ignore]
    fn test_analyze_msi() {
        let high_remote_path = Path::new("https://releases.voysys.dev/2.28.9/OdenVR_2.28.9.msi");
        let high_local_path = Path::new("OdenVR_2.28.9.msi");
        if high_local_path.exists() {
            println!("High component located");
        } else {
            println!("Downloading high component");
            download_package(high_remote_path, high_local_path)
                .expect("couldn't download component");
        }
        let low_remote_path = Path::new("https://releases.voysys.dev/2.26.0/OdenVR_2.26.0.msi");
        let low_local_path = Path::new("PhantomClient_2.29.4_phantom_plugin_4.0.3.msi");
        if low_local_path.exists() {
            println!("Low component located");
        } else {
            println!("Downloading low component");
            download_package(low_remote_path, low_local_path).expect("couldn't download component");
        }

        let (name, version, code) = MsiInstaller::analyze_msi(high_local_path).unwrap();
        println!("HIGH: {name}, {version}, {code}");
        let (name, version, code) = MsiInstaller::analyze_msi(low_local_path).unwrap();
        println!("LOW: {name}, {version}, {code}");
    }

    #[test]
    #[ignore]
    fn test_analyze_registry() {
        let product_name = "log2jira";
        let ids = MsiInstaller::get_registry_ids(product_name);
        if !ids.is_empty() {
            for (id, version) in ids {
                println!("Found {product_name} with registry id {id} and version {version}",);
            }
        } else {
            println!("{product_name} is not found in the registry");
        }
    }

    #[test]
    #[ignore]
    fn msiexec_mutex_test() {
        let mut seconds = 0;
        use single_instance::SingleInstance;

        while seconds < 100 {
            sleep(Duration::from_secs(1));
            let instance = SingleInstance::new("Global\\_MSIExecute").unwrap();
            if instance.is_single() {
                println!("{}: SINGLE", seconds);
            } else {
                println!("{}: UNSINGLE", seconds);
            }
            seconds += 1;
        }
    }


    #[test]
    #[ignore]
    fn msi_oden_full_cycle() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec_arg;
        let top = current_dir().unwrap();
        let test_dir = top.join(Path::new("msi_oden_full_cycle_test_dir"));
        if test_dir.exists() { fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!"); }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        let remote_path = Url::parse("https://releases.voysys.dev/2.24.11/OdenStreamer_2.24.11.msi").unwrap();
        let local_path = Path::new("OdenStreamer_2.24.11.msi");
        println!("Local path is {}", local_path.to_string_lossy());
        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager = LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            println!("Couldn't load the license: {}", err);
            println!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            assert!(false);
        }

        let license_token = LicenseManager::get_token(&license_manager, RestServer::get, RestServer::post).unwrap();
        //println!("Token is: {}", license_token);
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(), license_token.as_str());
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
        let update_status_response = |_ota_status, _message|{};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_status_response).unwrap();

        println!("Creating manifest");
        let write_function = |_: &Path, _: &str| Ok(());
        let read_function = |_: &Path| Ok(String::from(r#"{}"#));
        let manifest =
            Manifest::new(true, PathBuf::from(""), Default::default(), Default::default(),
                          read_function, write_function).unwrap();
        //let target_dir = PathBuf::from("C://Program Files/OdenVR");
        let previous = test_dir.join(license_manager.get_server().unwrap()).join("oden_player");

        let component: Component = Component {
            component: "oden_player".to_string(),
            checksum: "1b27e9b21e6daccee6a8a419d35a567f429834ea"
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
            .get(&ComponentType::oden_player)
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
            previous_install_path: Some(test_dir.join(license_manager.get_server().unwrap()).join("oden_player")),
            ..
            Component::empty()
        };

        MsiInstaller::install(&component, exec, |_,_|{}).unwrap();

        file_utils::create_dir_if_not_exists(&previous);
        fs::copy(test_dir.join(local_path), previous.join(local_path)).expect("failed to copy");
        println!("Uninstalling {}", component.clone().previous_install_path.expect("").to_string_lossy());
        sleep(std::time::Duration::from_secs(10));
        MsiInstaller::uninstall(&component, exec).unwrap();
        fs::remove_dir_all(&test_dir).expect("Failed to clear temp dir!");
    }

    #[test]
    #[ignore]
    fn test_msi_log_dir() {
        let dir = MsiInstaller::log_dir();
        println!("DIR IS {}", dir.to_string_lossy());
    }
}
