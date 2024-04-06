
#[cfg(windows)]
use crate::ota::manifest::DOWNLOAD_DIR;

use crate::ota::system_ctl::SystemCtl;
use crate::ota::ota_status::OTAStatusRestResponse;
use crate::ota::rest_listener::{create_rest_listener, get_ota_status, rest_listener, set_ota_status};
use crate::{
    ota::{ota_error::OTAError, ota_manager::PackageType, manifest::Component},
    service_trait::ServiceTrait,
    utils::{bash_exec::BashExec, file_utils::create_dir_if_not_exists},
};

use std::{
    cell::RefCell,
    path::PathBuf,
};

use ota::{version_table::VersionTable};
use crate::ota::ota_manager::{OTAManager, LOG_STRING};
use crate::ota::ota_status::{OTAStatus};
use crate::rest_comm::jira_log_submitter::JiraLogSubmitter;

use crate::config::Config;

pub mod auth;
pub mod config;
pub mod config_watcher;
pub mod logger;
pub mod ota;
pub mod rest_comm;
pub mod rest_request;
pub mod service_trait;
pub mod utils;
pub mod ui;
pub mod resources;

pub enum ServiceType {
    WindowsUpdater,
    WindowsOta,
    LinuxOta,
    Report,
}

#[derive(Debug, Clone)]
pub enum RestMessage {
    UpdateVersion,
    UpdateVersionForce,
    GetStatus,
    UpdateBothSides,
}

#[cfg(unix)]
pub fn create_ota_service(
    dest_path: PathBuf,
    hash_manifest_path: PathBuf,
    config: Config,
) -> Box<dyn ServiceTrait> {
    let system_control = RefCell::new(ota::system_ctl::SystemCtl::new());
    create_dir_if_not_exists(&dest_path);

    // Composing function install snap with bash exec
    let install_command =
        |component: &Component, installing: bool| -> Result<String, OTAError> {
            if installing { // Installing the component
                match ota::ota_manager::as_install_type(&component.package_type) {
                    PackageType::DEB => { (ota::deb_installer::DebInstaller::install)(component, BashExec::exec) }
                    PackageType::TAR => { (ota::tar_installer::TarInstaller::install_tar)(component, BashExec::exec_arg) }
                    PackageType::MSI => {
                        let path = component.path.clone().unwrap_or_default();
                        let error_msg = format!(
                            "Unsupported package type on Linux: MSI for path {}",
                            path.to_string_lossy()
                        );
                        log::error!("{error_msg}");
                        panic!("{error_msg}");
                    }
                    _ => { (ota::snap_installer::SnapInstaller::install)(component, BashExec::exec_arg) }
                }
            }
            else { // Uninstalling the component
                match ota::ota_manager::as_install_type(&component.package_type) {
                    PackageType::DEB => { (ota::deb_installer::DebInstaller::uninstall)(component, BashExec::exec) }
                    PackageType::TAR => { (ota::tar_installer::TarInstaller::uninstall_tar)(component, BashExec::exec_arg) }
                    PackageType::MSI => {
                        let path = component.path.clone().unwrap_or_default();
                        let error_msg = format!( "Unsupported package type on Linux: MSI for path {}", path.to_string_lossy());
                        log::error!("{error_msg}");
                        panic!("{error_msg}");
                    }
                    _ => { (ota::snap_installer::SnapInstaller::uninstall)(component, BashExec::exec_arg) }
                }
            }
        };
    create_rest_listener(Some(config.ota_rest_port), );

    let ota_manager = ota::ota_manager::OTAManager::new(
        system_control,
        hash_manifest_path,
        config,
        dest_path,
        install_command,
        update_status_response
    );
    set_rest_server_routes(&ota_manager);
    Box::new(ota_manager)
}

fn set_rest_server_routes(ota_manager: &OTAManager<SystemCtl>) {
    rest_listener().add_callback(
        "update_version".to_string(),
        Some((ota_manager.get_rest_channel_sender(), RestMessage::UpdateVersion)),
        OTAManager::<SystemCtl>::update_version,
    );
    rest_listener().add_callback(
        "update_version_force".to_string(),
        Some((ota_manager.get_rest_channel_sender(), RestMessage::UpdateVersionForce)),
        |_,_| { Ok(LOG_STRING.to_string()) }
    );
    rest_listener().add_callback(
        "update_version_both".to_string(),
        Some((ota_manager.get_rest_channel_sender(), RestMessage::UpdateBothSides)),
        |_,_| { Ok(LOG_STRING.to_string()) }
    );
    rest_listener().add_callback(
        "status".to_string(),
        None,
        |_,_| {
            let response = get_ota_status();
            let response_string = serde_json::to_string(&response).unwrap();
            Ok(response_string)
        }
    );
    rest_listener().add_callback(
        "log".to_string(),
        None,
        JiraLogSubmitter::send_custom_log,
    );
    rest_listener().add_callback(
        "check".to_string(),
        None,
        OTAManager::<SystemCtl>::check_versions,
    );
    rest_listener().add_callback(
        "write_to_log".to_string(),
        None,
        OTAManager::<SystemCtl>::write_to_log,
    );

}

pub fn service_factory(_service_type: ServiceType) -> Box<dyn ServiceTrait> {
    todo!();
}

pub fn update_status_response(ota_status: OTAStatus, message: Option<String>)
{
    let manifest_version = VersionTable::new().get_version();
    let message = message.unwrap_or_default();
    let response = OTAStatusRestResponse {
        ota_status,
        message,
        manifest_version,
    };
    set_ota_status(response);
}


#[cfg(windows)]
pub fn create_ota_service(
    _dest_path: PathBuf,
    hash_manifest_path: PathBuf,
    config: Config,
) -> Box<dyn ServiceTrait> {
    let dest_path = PathBuf::from(DOWNLOAD_DIR);
    create_rest_listener(Some(config.ota_rest_port));

    let install_command =
        |component: &Component, installing: bool| -> Result<String, OTAError> {
            if installing { // Installing the component
                match ota::ota_manager::as_install_type(&component.package_type) {
                    PackageType::MSI => { (ota::msi_installer::MsiInstaller::install)(component, BashExec::exec_arg, update_status_response) }
                    PackageType::TAR => { (ota::tar_installer::TarInstaller::install_tar)(component, BashExec::exec_arg) }
                    PackageType::SNAP => { (ota::snap_installer::SnapInstaller::install)(component, BashExec::exec_arg) }
                    _ => {
                        let path = component.path.clone().unwrap_or_default();
                        let error_msg = format!(
                            "Unsupported package type on Windows: {} for path {}", component.package_type,
                            path.to_string_lossy()
                        );
                        log::error!("{error_msg}");
                        panic!("{error_msg}");
                    }
                }
            }
            else { // Uninstalling the component
                match ota::ota_manager::as_install_type(&component.package_type) {
                    PackageType::MSI => { (ota::msi_installer::MsiInstaller::uninstall)(component, BashExec::exec_arg) }
                    PackageType::TAR => { (ota::tar_installer::TarInstaller::uninstall_tar)(component, BashExec::exec_arg) }
                    PackageType::SNAP => { (ota::snap_installer::SnapInstaller::uninstall)(component, BashExec::exec_arg) }
                    _ => {
                        let path = component.path.clone().unwrap_or_default();
                        let error_msg = format!(
                            "Unsupported package type on Windows: {} for path {}", component.package_type,
                            path.to_string_lossy()
                        );
                        log::error!("{error_msg}");
                        panic!("{error_msg}");
                    }
                }
            }
        };

    let system_control = RefCell::new(SystemCtl::new());
    create_dir_if_not_exists(&dest_path);
    let ota_manager = Box::new(ota::ota_manager::OTAManager::new(
        system_control,
        hash_manifest_path,
        config,
        dest_path,
        install_command,
        update_status_response
    ));


    set_rest_server_routes(&ota_manager);
    ota_manager

}

