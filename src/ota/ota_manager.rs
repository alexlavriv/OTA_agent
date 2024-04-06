
use crate::{auth::{
    license_manager_trait::{AuthError, LicenseManagerTrait},
    auth_manager::AuthManager,
}, ota::{
    download_manager::DownloadManager,
    file_system::FileSystem,
    install_manager::InstallManager,
    manifest::Manifest,
    ota_error::{OTAError, OTAErrorSeverity},
    service_control_trait::SystemControlTrait,
}, rest_comm::{
    core_rest_comm::CoreRestComm,
    core_rest_comm_trait::CoreRestCommTrait,
    coupling_rest_comm::{CouplingRestComm, RESTRequestFunction},
    coupling_submit_trait::{CouplingRestSubmitter, NodeOtaProgressStatus},
    jira_log_submitter::{send_snapshot_to_jira, JIRA_REPORT_TICKET},
}, rest_request::RestServer, service_trait::Action, utils::color::Coloralex, VersionTable};
use crate::{utils, RestMessage, ServiceTrait, config::Config};
use log;
use serde::Serialize;
use std::{
    cell::RefCell, fs, panic, panic::PanicInfo, path::PathBuf, sync::mpsc, thread::sleep, time::Duration,
    backtrace::{Backtrace, BacktraceStatus}, ops::Deref
};
use hyper::Uri;
use url::Url;
use serde_json::{json, Value};
use crate::auth::auth_manager::fetch_license_manager;
use crate::config::{ArchType, get_arch};

use crate::ota::manifest::{Component, current_agent_version, PREVIOUS_INSTALL_PATH};
use crate::utils::file_utils::{create_dir_if_not_exists, file_to_string};

#[cfg(not(windows))]
use crate::utils::bash_exec::BashExec;
pub const UPDATE_BOTH_STATUS_FILE: &str = "update_both_status";
pub const INCOMPLETE_INSTALL_STATUS_FILE: &str = "incomplete_install";
#[cfg(not(windows))]
pub const LOG_STRING: &str ="Phantom Agent is checking for updates, run\n\njournalctl -u snap.phantom-agent.phantom-agent-daemon.service -fo cat\n\nto follow the progress.\n";
#[cfg(windows)]
pub const LOG_STRING: &str ="Phantom Agent is checking for updates, run\n\npowershell Get-Content 'C:\\Program Files\\phantom_agent\\log\\phantom_agent.log' -Wait -Tail 30\n\nto follow the progress.\n";

use crate::ota::ota_status::{OTAStatus, OTAStatusRestResponse};
use crate::rest_comm::coupling_rest_comm::fetch_coupling_rest_comm;

#[derive(Debug, Serialize)]
pub struct Credentials {
    pub url: Url,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct Versions {
    pub update_available: bool,
    pub available_versions: Vec<String>,
    pub error: String,
}

pub struct OTAManager<A: SystemControlTrait> {
    system_control: RefCell<A>,
    core_rest_comm: Box<dyn CoreRestCommTrait>,
    fetch_license_manager: fn() -> Result<Box<dyn LicenseManagerTrait>, AuthError>,
    hash_manifest_path: PathBuf,
    previous_install_path: PathBuf,
    config: Config,
    dest_path: PathBuf,
    install_command: fn(component: &Component, installing: bool) -> Result<String, OTAError>,
    file_system: FileSystem,
    send_json: RESTRequestFunction,
    rest_channel_sender: mpsc::Sender<RestMessage>,
    rest_channel_receiver: mpsc::Receiver<RestMessage>,
    update_ota_status: fn(OTAStatus, Option<String>),
    override_operator: RefCell<Option<bool>>,
}

#[derive(PartialEq, Eq)]
pub enum PackageType {
    SNAP,
    MSI,
    DEB,
    TAR,
}

pub fn as_install_type(install_type: &str) -> PackageType {
    match install_type {
        "msi" => PackageType::MSI,
        "deb" => PackageType::DEB,
        "tar" => PackageType::TAR,
        _ => PackageType::SNAP,
    }
}

#[derive(PartialEq, Eq)]
pub enum UpdateBothStatus {
    None,
    Operator,
    Vehicle,
}

impl<A: SystemControlTrait> ServiceTrait for OTAManager<A> {
    fn run(&self) {
        self.run();
    }
    fn run_once(&self) -> Action {
        self.run_once()
    }
    fn start_update_both(&self) { self.set_update_both_status(UpdateBothStatus::Operator) }
}

impl<A: SystemControlTrait> OTAManager<A> {
    pub fn new(
        system_control: RefCell<A>,
        hash_manifest_path: PathBuf,
        config: Config,
        dest_path: PathBuf,
        install_command: fn(component: &Component, installing: bool) -> Result<String, OTAError>,
        update_ota_status: fn(OTAStatus, Option<String>),
    ) -> Self {
        let core_rest_comm = Box::new(CoreRestComm {
            url: config.core_uri.clone(),
            get: RestServer::get,
            post: RestServer::post,
        });
        update_ota_status(OTAStatus::UPDATED, None);
        let file_system = FileSystem {
            write_function: utils::file_utils::string_to_file,
            read_function: utils::file_utils::file_to_string,
            empty_folder: utils::file_utils::clear_folder,
            remove_file: utils::file_utils::rm_file,
        };
        let (rest_channel_sender, rest_channel_receiver) =
            mpsc::channel::<RestMessage>();
        let fetch_license_manager = fetch_license_manager;
        let previous_install_path = match hash_manifest_path.parent() {
            None => PathBuf::from(format!("./{}", PREVIOUS_INSTALL_PATH)),
            Some(parent) => parent.join(PREVIOUS_INSTALL_PATH),
        };
        create_dir_if_not_exists(&previous_install_path);
        Self {
            system_control,
            hash_manifest_path,
            previous_install_path,
            config,
            dest_path,
            install_command,
            core_rest_comm,
            file_system,
            fetch_license_manager,
            send_json: RestServer::send_json,
            rest_channel_sender,
            rest_channel_receiver,
            update_ota_status,
            override_operator: RefCell::new(None),
        }
    }
    pub fn get_operator(&self) -> bool {
        match *self.override_operator.borrow() {
            Some(operator) => {
                log::warn!("Overriding operator to {}, not using cloud config!", operator);
                operator
            },
            None => {
                let arch = get_arch();
                let is_operator = arch == ArchType::WIN || arch == ArchType::AMD64;
                let arch_type = arch.to_string().green(true);
                let node_type = if is_operator {"operator".green(true)} else {"vehicle".green(true)};
                log::info!("Architecture is {}, so our node type is {}", arch_type, node_type);
                is_operator
            }
        }
    }

    pub fn get_manifest(&self, operator: bool) -> Manifest {
        let server_name = match (self.fetch_license_manager)() {
            Ok(manager) => { manager.get_server().unwrap_or_default() }
            Err(e) => {
                let error = format!("Error occurred during loading the license manager file: {e}, OTA disabled");
                log::error!("{error}");
                (self.update_ota_status)(OTAStatus::ERROR, Some(error));
                "".to_string()
            }
        };
        Manifest::new(
            operator,
            self.hash_manifest_path.clone(),
            self.previous_install_path.clone(),
            server_name,
            self.file_system.read_function,
            self.file_system.write_function,
        ).unwrap()
    }

    pub fn run_once(&self) -> Action {
        (self.update_ota_status)(OTAStatus::CHECKING, None);

        let license_manager = match (self.fetch_license_manager)() {
            Ok(license_manager) => { license_manager }
            Err(error) => {
                return match error {
                    AuthError::NetworkError(error) => {
                        log::error!("Network error occurred while getting license token {error}, retrying...");
                        Action::RETRY
                    }
                    AuthError::LicenseError(error) | AuthError::DecodingError(error) => {
                        log::error!("Error occurred while getting license token: {error}");
                        Action::CONTINUE
                    }
                    AuthError::NotFoundError(error) => {
                        log::error!("Not Found Error occurred while getting license token: {error}");
                        Action::CONTINUE
                    }
                };
            }
        };

        let coupling_rest_comm = CouplingRestComm::new(license_manager.deref(), self.send_json);

        match self.get_update_both_status() {
            UpdateBothStatus::None => {}
            UpdateBothStatus::Operator => { *self.override_operator.borrow_mut() = Some(true); }
            UpdateBothStatus::Vehicle => { *self.override_operator.borrow_mut() = Some(false); }
        }

        let operator = self.get_operator();

        let manifest = self.get_manifest(operator);
        manifest.hash_manifest.verify_version(current_agent_version()); // This WILL panic if version is wrong!
        manifest.standardize_prev_dir();
        // Run the download logic only if core has not a connected session
        if self.core_rest_comm.is_core_has_connected_session() {
            log::info!("Core has connected session");
            return Action::RETRY
        }

        let manifest = match self.get_incomplete_install_status() {
            None => { manifest }
            Some(server) => {
                if manifest.server_name != server {
                    log::warn!("Incomplete install detected (for {}) but we are in ({}), doing full factory reset",
                        server, manifest.server_name);
                    if let Err(e) = self.purge_hash_manifest() {
                        log::error!("Error in full factory reset! ({})", e);
                        return Action::RETRY
                    }
                    self.get_manifest(operator) // replacing the manifest with an empty one and continuing
                }
                else {
                    log::warn!("Incomplete install detected (for {}), doing partial factory reset", server);
                    match self.purge_server_manifest(manifest, server) {
                        Err(e) => {
                            log::error!("Error in partial factory reset ({}), doing full factory reset!", e);
                            if let Err(e) = self.purge_hash_manifest() {
                                log::error!("Error in full factory reset! ({})", e);
                                return Action::RETRY
                            }
                            self.get_manifest(operator) // replacing the manifest with an empty one and continuing
                        }
                        Ok(manifest) => {
                            let manifest = manifest.write_to_file().expect("Failed to save to file!");
                            manifest
                        }
                    }
                }

            }
        };

        let download_manager = DownloadManager::new(
            &self.system_control,
            &coupling_rest_comm,
            self.dest_path.clone(),
            self.update_ota_status
        ).unwrap();

        let manifest = match download_manager.run(manifest) {
            Ok(manifest) => manifest,
            Err(error) => {
                log::error!("Download manager error: {error}");
                coupling_rest_comm.put_ota_status(
                    Some(error.message()),
                    None,
                    NodeOtaProgressStatus::Failed
                );
                (self.update_ota_status)(OTAStatus::ERROR, Some(error.message()));
                if error.severity == OTAErrorSeverity::FatalError {
                    if let Err(e) = send_snapshot_to_jira(JIRA_REPORT_TICKET, false) {
                        log::error!("Snapshot error: {e}");
                    }
                }
                return Action::CONTINUE;
            }
        };

        let manifest = if !manifest.is_fully_installed() {
            let install_manager = InstallManager::new(
                &self.system_control,
                self.install_command,
                &coupling_rest_comm,
                self.update_ota_status
            );
            self.set_incomplete_install_status(Some(manifest.server_name.clone())); // Detects if install did not finish
            match install_manager.install_manifest(manifest) {
                Ok(manifest) => {
                    self.core_rest_comm.update_manifest_version(&manifest.version);
                    manifest
                },
                Err(error) => {
                    coupling_rest_comm.put_ota_status(
                        Some(error.message()),
                        None,
                        NodeOtaProgressStatus::Failed
                    );
                    (self.update_ota_status)(OTAStatus::ERROR, Some(error.message()));
                    log::error!("Install manifest error: {error}");

                    if error.severity == OTAErrorSeverity::FatalError {
                        if let Err(e) = send_snapshot_to_jira(JIRA_REPORT_TICKET, false) {
                            log::error!("Snapshot error: {e}");
                        }
                    }
                    self.set_incomplete_install_status(None); // Install finished (error)
                    return Action::CONTINUE;
                }
            }
        }
        else {
            log::info!("Skipping install, since no components need to be updated");
            let status = coupling_rest_comm.get_ota_status();
            if status == NodeOtaProgressStatus::Updated {
                log::warn!("OTA status was already [Updated], so NOT sending it");
            } else {
                log::info!("OTA status was [{}], changing it to [Updated] now", status.as_str());
                coupling_rest_comm.put_ota_status(None, None, NodeOtaProgressStatus::Updated);
            }
            manifest
        };
        // If we got here the install completed successfully so we can safely clear the dest folder
        log::info!("Clearing download folder: {}", self.dest_path.to_string_lossy());
        (self.file_system.empty_folder)(self.dest_path.as_path())
            .expect("Download cleanup failed!");
        #[cfg(unix)]
        BashExec::sync(); // Making sure all the installed files are synced before we save the hash
        manifest.write_to_file().expect("Failed to save to file!");
        self.set_incomplete_install_status(None);   // Install finished (success)
        (self.update_ota_status)(OTAStatus::UPDATED, None);
        // If we're in the update both mode, we go to the next stage
        match self.get_update_both_status() {
            UpdateBothStatus::None => {}
            UpdateBothStatus::Operator => {
                self.set_update_both_status(UpdateBothStatus::Vehicle);
                log::info!("{}", "UPDATE BOTH: Install Operator complete. Switching to Vehicle...".blue(true));
                return self.run_once();
            }
            UpdateBothStatus::Vehicle => {
                self.set_update_both_status(UpdateBothStatus::None);
                log::info!("{}", "UPDATE BOTH: Install Vehicle complete. Both stages are complete!".blue(true));
            }
        }

        Action::CONTINUE
    }

    pub fn get_ota_status(&self) -> OTAStatusRestResponse {
        OTAStatusRestResponse { ota_status: OTAStatus::UPDATED, message: "".to_string(), manifest_version: "test".to_string() }
    }

    pub fn get_update_both_status(&self) -> UpdateBothStatus {
        let update_both_status_file = match self.hash_manifest_path.parent() {
            None => PathBuf::from(format!("./{}", UPDATE_BOTH_STATUS_FILE)),
            Some(parent) => parent.join(UPDATE_BOTH_STATUS_FILE),
        };
        if update_both_status_file.exists() {
            if let Ok(text) = file_to_string(&update_both_status_file) {
                if text == "operator" { return UpdateBothStatus::Operator; }
                else if text == "vehicle" { return UpdateBothStatus::Vehicle; }
            }
        }
        UpdateBothStatus::None
    }

    pub fn set_update_both_status(&self, status: UpdateBothStatus) {
        let update_both_status_file = match self.hash_manifest_path.parent() {
            None => PathBuf::from(format!("./{}", UPDATE_BOTH_STATUS_FILE)),
            Some(parent) => parent.join(UPDATE_BOTH_STATUS_FILE),
        };
        let text = match status {
            UpdateBothStatus::None => {
                if update_both_status_file.exists() && fs::remove_file(update_both_status_file).is_err() {
                    log::warn!("Failed to remove update both status file");
                }
                return;
            }
            UpdateBothStatus::Operator => { "operator" }
            UpdateBothStatus::Vehicle => { "vehicle" }
        };

        if utils::file_utils::string_to_file(&update_both_status_file, text).is_err() {
            log::warn!("Failed to set update both status file");
        }
    }

    pub fn get_incomplete_install_status(&self) -> Option<String> {
        let incomplete_install_status_file = match self.hash_manifest_path.parent() {
            None => PathBuf::from(format!("./{}", INCOMPLETE_INSTALL_STATUS_FILE)),
            Some(parent) => parent.join(INCOMPLETE_INSTALL_STATUS_FILE),
        };
        if incomplete_install_status_file.exists() {
            if let Ok(text) = file_to_string(&incomplete_install_status_file) {
                return Some(text);
            }
        }
        None
    }

    pub fn set_incomplete_install_status(&self, status: Option<String>) {
        let incomplete_install_status_file = match self.hash_manifest_path.parent() {
            None => PathBuf::from(format!("./{}", INCOMPLETE_INSTALL_STATUS_FILE)),
            Some(parent) => parent.join(INCOMPLETE_INSTALL_STATUS_FILE),
        };
        let text = match status {
            None => {
                if incomplete_install_status_file.exists() && fs::remove_file(incomplete_install_status_file).is_err() {
                    log::error!("Failed to remove incomplete install status file!");
                }
                return;
            }
            Some(text) => { text }
        };

        if utils::file_utils::string_to_file(&incomplete_install_status_file, &text).is_err() {
            log::error!("Failed to set update incomplete install file!");
        }
    }

    fn run_until_complete(&self) {
        loop {
            match self.run_once() {
                Action::RETRY => {
                    let ota_poll_frequency = self.config.ota_poll_frequency;
                    log::info!("OTA will retry, in {ota_poll_frequency} seconds");
                    sleep(Duration::new(u64::from(ota_poll_frequency), 0));
                }
                Action::CONTINUE => return,
            }
        }
    }

    fn log_and_error(text: &str) {
        log::error!("{}", text); // Write to log
        eprintln!("{}", text); // And also to stderr
    }

    pub fn panic_report(info: &PanicInfo) {
        let info_str = info.to_string();
        Self::log_and_error(&format!("Panic: [{}]", &info_str).red(true));

        if info_str.contains("PoisonError") {
            Self::log_and_error(&"Preventing looping with PoisonError".to_string().red(true));
            return;
        }
        let backtrace = Backtrace::force_capture();
        let backtrace_log = match backtrace.status() {
            BacktraceStatus::Unsupported => { "Backtrace is unsupported".to_string() }
            BacktraceStatus::Disabled => { "Backtrace is disabled".to_string() }
            BacktraceStatus::Captured => {
                format!("------- BACKTRACE -------\n{backtrace}-------------------------\n")
            }
            _ => { "Backtrace is unknown future status".to_string() }
        };
        Self::log_and_error(&backtrace_log.cyan(false));
        if let Err(e) = send_snapshot_to_jira(JIRA_REPORT_TICKET, false) {
            Self::log_and_error(&format!("Could not send snapshot: {e}"));
        }
        Self::log_and_error(&"Due to unrecoverable error, OTA process has terminated. Fix the error and restart the service.".red(true));
    }

    pub fn run(&self) {
        // This hook will intercept the panic and write it into log before the process ends
        panic::set_hook(Box::new(|info| {
            Self::panic_report(info);
        }));

        loop {
            self.run_until_complete();
            let mut count_seconds = self.config.ota_interval;
            while count_seconds > 0 {
                count_seconds -= 1;
                if let Ok(message) = self.rest_channel_receiver.recv_timeout(Duration::new(1, 0)) {
            
                    match message {
                        RestMessage::UpdateVersion => {
                            log::info!("Received request to check for updated version");
                            #[cfg(windows)]
                            crate::ui::progress_ui::ProgressUI::show();
                            self.run_until_complete();
                        }
                        RestMessage::UpdateVersionForce => {
                            log::info!("Received request for forcing a version");
                            #[cfg(windows)]
                            crate::ui::progress_ui::ProgressUI::show();
                            let manifest = self.get_manifest(self.get_operator());
                            let server_name = manifest.server_name.clone();
                            if let Ok(manifest) = self.purge_server_manifest(manifest, server_name) {
                                manifest.write_to_file().expect("Failed to save to file!");
                                self.run_until_complete();
                            }
                        }
                        RestMessage::UpdateBothSides => {
                            log::info!("{}", "UPDATE VERSION BOTH: Update both sides requested. Starting with Operator...".blue(true));
                            self.set_update_both_status(UpdateBothStatus::Operator);
                            self.run_until_complete();
                        }
                        _ => {
                            log::error!("Got the following value {:?}", message)
                        }
                    }

                    count_seconds = self.config.ota_interval; // Reset timer after run once
                }
            }
        }
    }

    pub fn update_version(_: Uri, auth_str: String) -> Result<String, String> {
        let auth: Value = if auth_str.is_empty() {
            if AuthManager::auth_path().exists() {
                match AuthManager::get_auth_values() {
                    Ok(auth) => { auth }
                    Err(e) => { return Err(format!("Please provide valid credentials ({})", e)); }
                }
            }
            else { return Ok(LOG_STRING.to_string()); } // No auth file and no credentials, using old license file
        } else {
            match serde_json::from_str(&auth_str) {
                Ok(value) => { value }
                Err(e) => { return Err(format!("Cannot parse credentials: {}", e)); }
            }
        };
        if !AuthManager::valid_auth(&auth) {
            return Err("Cannot parse credentials".to_string());
        }
        if AuthManager::save_auth_values(&auth).is_ok() {
            log::info!("Auth file saved");
        }
        Ok(LOG_STRING.to_string())
    }

    pub fn get_rest_channel_sender(&self) -> mpsc::Sender<RestMessage> {
        self.rest_channel_sender.clone()
    }

    pub fn fetch_versions_from_cloud(license_manager: &AuthManager) -> Result<Versions, String> {
        log::info!("Fetching versions from server");
        let coupling_rest_comm = CouplingRestComm::new(license_manager, RestServer::send_json);
        match std::thread::spawn(move || -> Result<String, String> {
            coupling_rest_comm.check_versions()
        }).join() {
            Ok(result) => {
                match result {
                    Ok(versions) => {
                        log::info!("Fetched versions: {}", versions);
                        let available_versions: Vec<String> = serde_json::from_str(&versions).unwrap_or_default();
                        let current_version = VersionTable::new().get_version();
                        let update_available = available_versions.is_empty() || available_versions[0] != current_version;
                        let versions = Versions { update_available, available_versions, error: "".to_string() };
                        Ok(versions)
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch versions: {}", e);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                log::error!("Crash in check version thread");
                Err("Check version thread crashed".to_string())
            }
        }
    }

    pub fn make_versions_error(error: String) -> String {
        log::warn!("Making a versions error: {}", error);
        let versions = Versions {
            update_available: false,
            available_versions: Vec::new(),
            error
        };
        json!(versions).to_string()
    }

    pub fn parse_credentials(credentials_str: &str) -> Option<Value> {
        if credentials_str.is_empty() {
            None
        } else if let Ok(credentials) = serde_json::from_str::<Value>(credentials_str) {
            match credentials.clone() {
                Value::Object(value) => {
                    if !value["url"].is_string() {
                        log::error!("No url provided!");
                        return None;
                    }

                    if !value["token"].is_string() {
                        log::error!("No token provided!");
                        return None;
                    }
                    Some(credentials)
                }
                _ => { log::error!("Could not parse credentials!"); None }
            }
        }
        else { log::error!("Could not parse credentials!"); None }
    }

    pub fn purge_hash_manifest(&self) -> Result<(), String> {
        (self.file_system.remove_file)(&self.hash_manifest_path)
    }

    pub fn purge_server_manifest(&self, manifest: Manifest, server: String) -> Result<Manifest, String> {
        if manifest.server_name != server { // Sanity check
            log::error!("Purge requested on ({}) but our server is ({}), should be impossible!", server, manifest.server_name);
        }
        let manifest = manifest.prepare_for_server_purge();
        let coupling_rest_comm = fetch_coupling_rest_comm()?;
        let install_manager = InstallManager::new(
            &self.system_control,
            self.install_command,
            &coupling_rest_comm,
            self.update_ota_status
        );
        for (_, component) in &manifest.components {
            if component.should_uninstall() {
                install_manager.purge_component(component);
            }
        }
        Ok(manifest)
    }

    pub fn write_to_log(_: Uri, str: String) -> Result<String, String> {
        log::info!("[external log]: {}", str);
        Ok("Written to log".to_string())
    }

    pub fn check_versions(_: Uri, credentials_str: String) -> Result<String, String> {
        let credentials = Self::parse_credentials( &credentials_str );

        let auth_manager = {
            let mut auth_manager = AuthManager::default();
            if auth_manager.read_license().is_err() {
                match credentials {
                    Some(mut value) => {
                        value["version"] = serde_json::Value::from("");
                        match auth_manager.update_from_value(value) {
                            Ok(()) => { auth_manager }
                            Err(e) => { return Err(Self::make_versions_error(format!("Failed to update auth manager: {}", e))); }
                        }
                    }
                    None => { return Err(Self::make_versions_error("Please provide valid credentials".to_string())); }
                }
            }
            else {
                if let Some(mut value) = credentials {
                    let version = auth_manager.get_name().unwrap_or_default();
                    value["version"] = serde_json::Value::from(version);
                    if let Err(e) = auth_manager.update_from_value(value) {
                        return Err(Self::make_versions_error(format!("Failed to update auth manager: {}", e)));
                    }
                }
                auth_manager
            }
        };

        match Self::fetch_versions_from_cloud(&auth_manager) {
            Err(e) => { Err(Self::make_versions_error(e)) }
            Ok(versions) => {
                match std::thread::spawn(move || -> Result<(), String> {
                    auth_manager.save_into_file()
                }).join() {
                    Ok(result) => {
                        match result {
                            Err(e) => { Err(Self::make_versions_error(e)) }
                            Ok(()) => {
                                log::info!("Auth manager saved");
                                Ok(json!(versions).to_string())
                            }
                        }
                    }
                    Err(_) => {
                        log::error!("Crash in check version thread");
                        Err("Check version thread crashed".to_string())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::auth::license_manager_trait::{
        AuthError, LicenseManagerTrait, MockLicenseManagerTrait,
    };
    use crate::ota::file_system::FileSystem;
    use crate::ota::ota_error::OTAError;
    use crate::ota::ota_manager::OTAManager;
    use crate::ota::service_control_trait::MockSystemControlTrait;
    use crate::ota::manifest::Component;
    use crate::rest_comm::core_rest_comm_trait::MockCoreRestCommTrait;
    use crate::rest_request::SendType;
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::RestMessage;
    use std::{cell::RefCell, path::Path, str::FromStr, sync::mpsc};
    use url::Url;

    fn send_json(
        method: SendType,
        _: &Url,
        _: Option<&serde_json::Value>,
        _authorization: Option<String>,
    ) -> Result<(String, u16), (String, u16)> {
        match method {
            SendType::POST => {
                let result = r#"
[
  {
    "token": "eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
    "_id": "621c78f3fd12780012c795da",
    "component": "core",
    "version": "0.1.2",
    "link": "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
    "checksum": "101010101",
    "arch": "AMD64"
  }
]
                    "#;
                Ok((result.to_string(), 200))
            }
            SendType::GET => {
                let result = r#"{
                        "type" : "operator"
                    }"#;
                Ok((result.to_string(), 204))
            }
            SendType::PUT => {
                let result = r#""#;
                Ok((result.to_string(), 204))
            }
            _ => {
                panic!("Unsupported method!");
            }
        }
    }

    fn get_json() -> Result<String, String> {
        let result = r#"
        {
            "oden_plugin":"1234567890abcdef1234567890abcdef12345678"
        }
       "#;
        Ok(result.to_string())
    }

    pub fn fetch_for_ota_manager_test() -> Result<Box<dyn LicenseManagerTrait>, AuthError> {
        let mut license_manager = Box::new(MockLicenseManagerTrait::new());
        license_manager.expect_read_license().returning(|| Ok(()));
        license_manager.expect_get_token().returning(|| Ok("token".to_string()));
        license_manager.expect_get_url().returning(|| Ok(Url::from_str("https://test.com").unwrap()));
        license_manager.expect_get_server().returning(|| Ok("test_server".to_string()));
        license_manager.expect_get_name().returning(|| Ok("".to_string()));
        license_manager.expect_get_path().returning(|| Ok("".to_string().into()));
        Ok(license_manager)
    }

    pub fn fetch_for_server_error_test() -> Result<Box<dyn LicenseManagerTrait>, AuthError> {
        Err(AuthError::NetworkError("No network!".to_string()))
    }

    #[test]
    fn ota_manager_test() {
        let system_control = RefCell::new(MockSystemControlTrait::new());
        let mut core_rest_comm = MockCoreRestCommTrait::new();

        let install_command =
            |_component: &Component, _installing: bool| Ok("alex".to_string());
        let write_function = |_: &Path, content: &str| {
            let components: serde_json::Value = serde_json::from_str(content).unwrap();
            let installed = !components
                .get("oden_plugin")
                .unwrap()
                .as_str()
                .unwrap()
                .is_empty();
            assert!(installed);
            Ok(())
        };
        let file_system = FileSystem {
            write_function,
            read_function: |_| get_json(),
            empty_folder: |_| Ok(()),
            remove_file: |_| Ok(()),
        };

        core_rest_comm
            .expect_is_core_has_connected_session()
            .times(1)
            .returning(|| false);

        let update_ota_status =  |_ota_status, _message |{};
        let (rest_channel_sender, rest_channel_receiver) =
            mpsc::channel::<RestMessage>();
        let manager = OTAManager {
            system_control,
            core_rest_comm: Box::new(core_rest_comm),
            fetch_license_manager: fetch_for_ota_manager_test,
            hash_manifest_path: Default::default(),
            previous_install_path: Default::default(),
            config: Default::default(),
            dest_path: Default::default(),
            install_command,
            file_system,
            send_json,
            rest_channel_sender,
            rest_channel_receiver,
            update_ota_status,
            override_operator: RefCell::new(None),
        };

        manager.run_once();
        std::fs::remove_file(std::path::PathBuf::from(format!(
            "./{}",
            crate::ota::manifest::FUTURE_VERSION_PATH
        )))
        .expect("Failed to remove file");
    }

    #[test]
    fn ota_manager_server_error_test() {
        let system_control = RefCell::new(MockSystemControlTrait::new());
        let core_rest_comm = MockCoreRestCommTrait::new();
        let install_command =
            |_component: &Component, _installing: bool| Ok("alex".to_string());
        let write_function = |_: &Path, content: &str| {
            let components: serde_json::Value = serde_json::from_str(content).unwrap();
            println!("WRITE FUNC {}", content);
            let installed = !components
                .get("oden_plugin")
                .unwrap()
                .as_str()
                .unwrap()
                .is_empty();
            assert!(installed);
            Ok(())
        };
        let file_system = FileSystem {
            write_function,
            read_function: |_| get_json(),
            empty_folder: |_| Ok(()),
            remove_file: |_| Ok(()),
        };

        let update_ota_status =  |_ota_status, _message |{};
        let (rest_channel_sender, rest_channel_receiver) =
            mpsc::channel::<RestMessage>();
        let manager = OTAManager {
            system_control,
            core_rest_comm: Box::new(core_rest_comm),
            fetch_license_manager: fetch_for_server_error_test,
            hash_manifest_path: Default::default(),
            previous_install_path: Default::default(),
            config: Default::default(),
            dest_path: Default::default(),
            install_command,
            file_system,
            send_json,
            rest_channel_sender,
            rest_channel_receiver,
            update_ota_status,
            override_operator: RefCell::new(None),
        };

        manager.run_once();
    }

    pub fn string_func() -> Result<String, String> {
        Err("We return a string error".to_string())
    }
    pub fn ota_error_func() -> Result<String, OTAError> {
        string_func()?;
        Ok("We return okay if the ? didn't trigger".to_string())
    }

    #[test]
    #[ignore]
    fn error_conversion_test() {
        match ota_error_func() {
            Ok(s) => {
                println!("OKAY: {}", s);
            }
            Err(e) => {
                println!("ERROR: {}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn arch_operator_display_test() {
        use crate::config::ArchType;
        use crate::utils::color::Coloralex;
        set_logging_for_tests(log::LevelFilter::Info);
        let operators = ["operator", "vehicle"];
        let arches = [ArchType::WIN, ArchType::ARM64, ArchType::AMD64];
        for operator in operators {
            for arch in arches {
                let is_operator = operator.eq("operator");
                if arch == ArchType::ARM64 && is_operator || arch == ArchType::WIN && !is_operator {
                    let node_type = operator.red(true);
                    let arch_type = arch.to_string().red(true);
                    log::error!("Got node {} type which is impossible in {} architecture!", node_type, arch_type);
                }
                else {
                    let node_type = operator.green(true);
                    log::info!("Got node {} type from cloud config", node_type);
                }
            }
        }
    }
}
