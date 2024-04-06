use crate::ota::ota_error::OTAError;
use crate::utils::color::Coloralex;
use crate::BashExec;
use std::{env::set_var, path::Path, str, string::String};
use tokio::process::Command;
use crate::ota::manifest::Component;

// https://releases.voysys.dev/2.22.1/OdenVR_2.22.1.msi

pub struct DebInstaller;

impl DebInstaller {
    #[tokio::main]
    pub async fn extract_package_info(path: &Path) -> Result<(String, String), String> {
        if !path.exists() {
            return Err(format!("{} not found", path.to_string_lossy()));
        }
        let mut input = Command::new("dpkg");
        input.arg("--info").arg(path.to_str().unwrap());
        let output = input.output().await.expect("Error!");
        let out_code = output.status.code().unwrap();
        if out_code != 0 {
            return Err(format!(
                "Failed to extract info from package {}",
                path.to_string_lossy()
            ));
        }
        let mut name = "".to_string();
        let mut version = "".to_string();
        for line in String::from_utf8_lossy(&output.stdout).to_string().lines() {
            if line.starts_with(" Package:") {
                name = line[10..].to_string();
            }
            if line.starts_with(" Version:") {
                version = line[10..].to_string();
            }
        }
        if !name.is_empty() {
            Ok((name, version))
        } else {
            Err("Failed to extract name from package!".to_string())
        }
    }

    #[tokio::main]
    pub async fn check_installed_version(package_name: &str) -> Option<String> {
        let mut input = Command::new("apt");
        input.arg("show").arg(package_name);

        let output = input.output().await.expect("Error!");
        let out_code = output.status.code().unwrap();
        if out_code == 0 {
            for line in String::from_utf8_lossy(&output.stdout).to_string().lines() {
                if line.starts_with("Version:") {
                    let version = line[9..].to_string();
                    log::info!("The installed version for {} is {}", package_name, version);
                    return Some(version);
                }
            }
            log::warn!("Could not find installed version for {}", package_name);
        } else {
            log::error!("Error finding installed version for {}, out_code {}", package_name, out_code);
        }
        None
    }

    fn _set_selection_flag(
        package_name: &str,
        flag_name: &str,
        flag_value: &str,
    ) -> Result<String, String> {
        let command = format!("{package_name} {package_name}/{flag_name} select {flag_value}");
        BashExec::exec_pipe("debconf-set-selections", None, &command)
    }

    pub fn install(
        component: &Component,
        exec_command: fn(&str) -> Result<String, String>,
    ) -> Result<String, OTAError> {
        let path = component.path.clone().unwrap_or_default();
        if !path.exists() {
            return Err(OTAError::nonfatal(format!(
                "{} not found",
                path.to_string_lossy()
            )));
        }
        log::info!("Installing {}", path.to_str().unwrap());
        let (name, version) = DebInstaller::extract_package_info(&path).unwrap();
        log::info!("Target package name is {} and version is {}", name, version);
        let mut path_str = path.to_str().unwrap().to_string();
        if !path.is_absolute() {
            path_str = format!("./{path_str}");
        }
        set_var("DEBIAN_FRONTEND", "noninteractive");
        let command = format!("apt-get -y install {path_str} --allow-downgrades");
        if let Err(e) = (exec_command)(&command) {
            if e.contains("dpkg was interrupted") {
                log::warn!("Attempting to recover from interruption");
                let recovery_command = "dpkg --configure -a".to_string();
                if let Err(e) = (exec_command)(&recovery_command) {
                    log::warn!("Recovery was not successful: {}", e);
                }
            } else {
                return Err(OTAError::nonfatal(e));
            }
        }
        match DebInstaller::check_installed_version(&name) {
            Some(installed_version) => {
                if version == installed_version {
                    Ok("Install completed successfully".to_string())
                }
                else {
                    Err(OTAError::nonfatal(format!("Install did not succeed, expected {}, but version is {}", version, installed_version)))
                }
            }
            None => { Err(OTAError::nonfatal("Install did not succeed, could not check version".to_string())) }
        }
    }

    pub fn uninstall(
        component: &Component,
        exec_command: fn(&str) -> Result<String, String>,
    ) -> Result<String, OTAError> {
        match &component.previous_install_path {
            None => { Ok("No previous install path".to_string()) }
            Some(previous_dir) => {
                if previous_dir.exists() && previous_dir.is_dir() {
                    match std::fs::read_dir(previous_dir).unwrap().count() {
                        1 => {
                            let file = std::fs::read_dir(previous_dir).unwrap().next().unwrap().unwrap().path();
                            let (name, _version) = DebInstaller::extract_package_info(&file).unwrap();
                            DebInstaller::uninstall_by_package_name(&name, exec_command)
                        }
                        count => { Ok(format!("Expected 1 files, but found {} files", count))}
                    }
                }
                else {
                    Ok("Previous uninstall directory doesn't exist".to_string())
                }
            }
        }
    }

    pub fn uninstall_by_package_name(
        package_name: &str,
        exec_command: fn(&str) -> Result<String, String>,
    ) -> Result<String, OTAError> {
        let command = format!("apt-get remove {package_name} -y");
        log::info!("Uninstalling {}", package_name);
        match (exec_command)(&command) {
            Err(e) => {
                log::warn!(
                    "{} {}",
                    "Failed to remove".red(false),
                    &package_name.red(true)
                );
                Err(OTAError::nonfatal(e))
            }
            Ok(res) => {
                if DebInstaller::check_installed_version(package_name).is_none() {
                    log::info!(
                        "{} {}",
                        &package_name.green(true),
                        "removed successfully".green(false)
                    );
                    Ok(res)
                } else {
                    log::warn!(
                        "{} {}",
                        "Failed to remove".red(false),
                        &package_name.red(true)
                    );
                    Err(OTAError::nonfatal("Could not validate uninstallation".to_string()))
                }
            }
        }
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use crate::ota::deb_installer::DebInstaller;
    use crate::utils::bash_exec::BashExec;
    use crate::utils::log_utils::set_logging_for_tests;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tokio::process::Command;
    use crate::ota::manifest::Component;
    use crate::utils::file_utils;

    #[tokio::main]
    pub async fn download_package(remote_path: &Path, local_path: &Path) -> Result<String, String> {
        let mut input = Command::new("curl");
        input
            .arg(remote_path.to_str().unwrap())
            .arg("--output")
            .arg(local_path.to_str().unwrap());
        let output = input.output().await.expect("Error!");
        let out_code = output.status.code().unwrap();
        if out_code != 0 {
            Err("Failed to download package!".to_string())
        } else {
            Ok("Download success".to_string())
        }
    }

    #[test]
    #[ignore]
    fn deb_oden_install() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec;
        let remote_path =
            Path::new("https://releases.voysys.dev/2.20.2/oden-streamer_2.20.2_amd64_20.04.deb");
        let local_path = Path::new("oden-streamer_for_oden_install.deb");
        download_package(remote_path, local_path).unwrap();
        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(local_path)),
            package_type: "deb".to_string(),
            previous_install_path: Some(current_dir().unwrap().join("oden_streamer")),
            ..
            Component::empty()
        };
        DebInstaller::install(&component, exec).unwrap();
        let (name, version) = DebInstaller::extract_package_info(local_path).unwrap();
        assert!(DebInstaller::check_installed_version(&name) == Some(version));
        fs::remove_file(local_path).unwrap();
    }

    #[test]
    #[ignore]
    fn deb_oden_uninstall() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec;
        let remote_path =
            Path::new("https://releases.voysys.dev/2.20.2/oden-streamer_2.20.2_amd64_20.04.deb");
        let local_path = Path::new("oden-streamer_for_oden_uninstall.deb");
        download_package(remote_path, local_path).unwrap();
        let (name, _version) = DebInstaller::extract_package_info(local_path).unwrap();
        DebInstaller::uninstall_by_package_name(&name, exec).unwrap();
        assert!(DebInstaller::check_installed_version(&name).is_none());
        fs::remove_file(local_path).unwrap();
    }

    #[test]
    #[ignore]
    fn deb_oden_component_full_cycle() {
        set_logging_for_tests(log::LevelFilter::Info);
        let exec = BashExec::exec;
        let remote_path =
            Path::new("https://releases.voysys.dev/2.20.2/oden-streamer_2.20.2_amd64_20.04.deb");
        let local_path = Path::new("oden-streamer_for_oden_uninstall.deb");
        download_package(remote_path, local_path).unwrap();
        let previous = current_dir().unwrap().join("oden_streamer");
        let component: Component = Component {
            updated: false,
            path: Some(PathBuf::from(local_path)),
            package_type: "deb".to_string(),
            previous_install_path: Some(previous.clone()),
            ..
            Component::empty()
        };
        DebInstaller::install(&component, exec).unwrap();
        let (name, version) = DebInstaller::extract_package_info(local_path).unwrap();
        assert!(DebInstaller::check_installed_version(&name) == Some(version));

        file_utils::create_dir_if_not_exists(&previous);
        fs::copy(local_path, previous.join(local_path)).expect("failed to copy");

        match DebInstaller::uninstall(&component, exec) {
            Ok(message) => {log::info!("UNINSTALL OK: {}", message);}
            Err(error) => {log::info!("UNINSTALL ERR: {}", error.message);}
        }
        assert!(DebInstaller::check_installed_version(&name).is_none());
        fs::remove_dir_all(&previous).expect("failed to remove dir");
        fs::remove_file(local_path).expect("failed to remove file");
    }
}
