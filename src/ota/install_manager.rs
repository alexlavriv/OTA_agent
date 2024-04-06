use crate::ota::manifest::{Component, ComponentType, Manifest};
use crate::ota::ota_error::{OTAError, OTAErrorSeverity};
use crate::ota::ota_manager::{as_install_type, PackageType};
use crate::ota::service_control_trait::SystemControlTrait;
use crate::rest_comm::coupling_submit_trait::{CouplingRestSubmitter, NodeOtaProgressStatus};
use crate::utils::color::Coloralex;
use crate::utils::file_utils::get_sha1_checksum;
use log;
use std::cell::RefCell;
use std::str::FromStr;
use std::fs;
use log::info;

#[cfg(windows)]
use {
    std::thread::sleep,
    std::time::Duration,
    crate::BashExec,
    crate::utils::tasklist::Tasklist,
//  crate::ota::manifest::{DOWNLOAD_DIR, WINDOWS_SERVICE_TRIGGER_PATH},
};
use crate::ota::ota_status::OTAStatus;
#[cfg(unix)]
use crate::ota::snap_installer::SnapInstaller;
use crate::rest_comm::jira_log_submitter::{JIRA_REPORT_TICKET, send_snapshot_to_jira};

pub struct InstallManager<'c, A: SystemControlTrait> {
    #[allow(dead_code)]
    system_control: &'c RefCell<A>,
    install_command: fn(component: &Component, installing: bool) -> Result<String, OTAError>,
    status_submitter: &'c dyn CouplingRestSubmitter,
    update_ota_status: fn(OTAStatus, Option<String>),
}

impl<'c, A: SystemControlTrait> InstallManager<'c, A> {
    pub fn new(
        system_control: &'c RefCell<A>,
        install_command: fn(component: &Component, installing: bool) -> Result<String, OTAError>,
        status_submitter: &'c dyn CouplingRestSubmitter,
        update_ota_status: fn(OTAStatus, Option<String>),
    ) -> Self {
        Self {
            system_control,
            install_command,
            status_submitter,
            update_ota_status,
        }
    }

    fn kill_component_processes(&self, component: &Component) {
        if !component.processes.is_empty() {
            log::info!("Found {} processes to stop", component.processes.len());
            for process_name in &component.processes {
                if process_name.len() < 6 {
                    log::warn!("Sanity check - {} is too short to be a process name!", process_name);
                } else {
                    let mut system_control = self.system_control.borrow_mut();
                    let process_ids = system_control.find_process(process_name);
                    if process_ids.is_empty() {
                        log::info!("Process {} is not currently running, continuing...", process_name);
                    } else {
                        log::info!("Found {} instances of {}, stopping...", process_ids.len(), process_name);
                        for pid in &process_ids {
                            match system_control.kill_process(*pid) {
                                Ok(_) => {
                                    log::info!("Process killed successfully");
                                }
                                Err(e) => {
                                    log::error!("Failed to kill process: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(windows)]
        if as_install_type(&component.package_type) == PackageType::TAR {
            self.kill_tar_dll_processes(component);
        }
    }

    // Install a component if it has a PATH, but if it doesn't have a PATH, uninstall it using prev save
    fn install_or_uninstall_component(&self, component: &Component) -> bool {
        if component.updated {  // Sanity check, if it's not marked for install/uninstall, we shouldn't do anything!
            log::info!("{} - update is not required", component.component);
            return true;
        }

        if component.should_install() || component.should_uninstall() {
            let installing = component.should_install();
            let updating = if installing { "Installing" } else { "Uninstalling" };
            log::info!("{} {}", updating, component.component);
            if as_install_type(&component.package_type) == PackageType::TAR && component.target_path.is_none() {
                log::error!("No target path for tar component!");
                return false;
            }

            self.kill_component_processes(component);
            let component = if installing {
                component.clone()
            } else {
                let (prev_exists, file) = component.uninstall_information();
                if prev_exists {
                    Component {
                        path: Some(file),
                        ..
                        component.clone()
                    }
                } else {
                    component.clone()
                }
            };

            let mut result = (self.install_command)(&component, installing);
            if as_install_type(&component.package_type) == PackageType::TAR {
                let mut extra_attempts = 3;
                while extra_attempts > 0 {  // Retrying TAR install if it failed due to "Can't unlink" by killing processes again
                    if let Err(error) = result.clone() {
                        if error.severity == OTAErrorSeverity::NonFatalError && error.message.contains("Can't unlink") {
                            extra_attempts = extra_attempts - 1;
                            log::warn!("Retrying TAR install: {}", error.message);
                            self.kill_component_processes(&component);
                            result = (self.install_command)(&component, installing);
                        }
                        else { extra_attempts = 0; }
                    }
                    else { extra_attempts = 0; }
                }
            }

            if let Err(error) = result {
                let failed_updating = if installing { "Failed installing".red(false) } else { "Failed uninstalling".red(false) };
                let to_from_version = if installing { "to version ".red(false) } else { "".to_string() };
                let version = if installing { component.version.red(true) } else { "".to_string() };
                log::warn!(
                    "{} {} {}{}{} {}",
                    failed_updating,
                    &component.component.red(true),
                    to_from_version,
                    version,
                    ":".red(false),
                    error.message.red(false)
                );
                false
            } else {
                if as_install_type(&component.package_type) == PackageType::TAR {
                    self.kill_component_processes(&component);  // Ensure processes run with new files
                }
                let success_updating = if installing {
                    "successfully installed to version ".green(false)
                } else {
                    "successfully uninstalled".green(false)
                };
                let version = if installing { component.version.green(true) } else { "".to_string() };
                log::info!(
                    "{} {}{}",
                    &component.component.green(true),
                    success_updating,
                    version
                );
                true
            }
        } else {
            log::warn!("Component {} was marked for install/uninstall, but it's already handled!", component.component);
            false
        }
    }

    #[cfg(windows)]
    pub fn kill_tar_dll_processes(&self, component: &Component) {
        if as_install_type(&component.package_type) != PackageType::TAR {
            log::warn!("{}", format!("Kill tar dll process called on {} but it's {} not TAR", component.component, component.package_type).red(false));
            return;
        }
        if let Ok(files) = BashExec::list_files_in_archive(component.path.clone().unwrap_or_default(), BashExec::exec_arg) {
            let tasklist = Tasklist::new();
            for file in files {
                if let Some(name) = file.split(['\\', '/']).collect::<Vec<_>>().last() {
                    if name.ends_with(".dll") {
                        let pids_paths = tasklist.check_dll(name);
                        if pids_paths.is_empty() {
                            log::info!("{}", format!("No processes are using {}", name).green(false));
                        }
                        else {
                            log::trace!("{}", format!("Found {} processes using {}, checking whether to kill", pids_paths.len(), name).yellow(false));
                            for (pid, path) in pids_paths {
                                if let Ok(pid_int) = pid.parse::<usize>() {
                                    if path.to_lowercase().contains("program files") && path.to_lowercase().contains("phantom") {
                                        let message = format!("Killing Phantom process {} with path [{}]", pid_int, path).yellow(false);
                                        log::info!("{}", message);
                                        match self.system_control.borrow().kill_process(pid_int) {
                                            Ok(_) => { log::info!("Process killed successfully"); }
                                            Err(e) => { log::error!("Failed to kill process: {}", e); }
                                        }
                                    }
                                    else {
                                        log::trace!("{}", format!("Process {} with path [{}] is not a Phantom process", pid_int, path).green(false));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn purge_component(&self, component: &Component) {  // Tries to uninstall, and never fails
        if !component.should_uninstall() { // Sanity check, if it's not marked for install/uninstall, we shouldn't do anything!
            log::info!("{} - purge is not required", component.component);
            return;
        }
        log::info!("Purging {}", component.component);
        if as_install_type(&component.package_type) == PackageType::TAR && component.target_path.is_none() {
            log::warn!("No target path for tar component!");
            return;
        }
        self.kill_component_processes(component);
        let component = {
            let (prev_exists, file) = component.uninstall_information();
            if prev_exists {
                Component { path: Some(file), ..component.clone() }
            } else {
                component.clone()
            }
        };

        if let Err(error) = (self.install_command)(&component, false) {
            log::warn!(
                    "{} {} {} {}",
                    "Failed purging".red(false),
                    &component.component.red(true),
                    ":".red(false),
                    error.message.red(false)
                );
        } else {
            if as_install_type(&component.package_type) == PackageType::TAR {
                self.kill_component_processes(&component);  // Ensure processes run with new files
            }
            log::info!(
                "{} {}",
                &component.component.green(true),
                "successfully purged".green(false),
            );
        }
    }

    /*
        fn uninstall_if_needed(&self, component: &Component, uninstall_file: &PathBuf) -> bool {
            use std::path::PathBuf;
            if Manifest::snap_name_to_enum(&component.component) == component_type::phantom_agent {
                log::warn!("Asked to uninstall Phantom Agent, this shouldn't be possible!");
                return true;
            }
            match as_install_type(&component.package_type) {
                PackageType::DEB => {
                    let deb_install_command = ota::deb_installer::DebInstaller::install;
                    deb_install_command(path, BashExec::exec)
                }
                PackageType::TAR => {
                    let tar_install_command = ota::tar_installer::TarInstaller::install_tar;
                    tar_install_command(path, target_path, BashExec::exec_arg)
                }
                PackageType::MSI => {
                    let error_msg = format!(
                        "Unsupported package type on Linux: MSI for path {}",
                        path.to_string_lossy()
                    );
                    log::error!("{error_msg}");
                    panic!("{error_msg}");
                }
                _ => {
                    let snap_install_command = ota::snap_installer::SnapInstaller::install;
                    snap_install_command(path, BashExec::exec_arg)
                }
            }
            true
        }
    */

    #[cfg(windows)]
    fn create_script(&self, source: &str, target: &str) -> String {
        format!(r#"
@echo off
set /a kill_attempts=0

:kill_service

if %kill_attempts% lss 5 (
set /a kill_attempts+=1
call:log "Killing phantom_agent.exe -- %kill_attempts%"
taskkill /F /IM phantom_agent.exe
timeout 1
tasklist /fi "ImageName eq phantom_agent.exe" /fo csv 2>NUL | find /I "phantom_agent.exe">NUL

if NOT "%ERRORLEVEL%"=="0" GOTO not_running

GOTO kill_service
)
call:log "Failed to kill process! Process is apparently still running! Exiting with error!"
GOTO end

:not_running
call:log "Process was killed successfully"

copy "{source}" "{target}"

:restart_service
: Add to schtasksgo
schtasks /delete /tn "Start Phantom Agent" /f


schtasks /run /tn "StartPhantomAgent"
:wait_for_start
timeout 1
:end
EXIT /B %ERRORLEVEL%

:log
curl --request POST --data "%~1" http://localhost:30000/write_to_log
echo.
echo %~1
EXIT /B 0

"#)
    }

    fn install_agent(&self, manifest: Manifest) -> Manifest {
        // Clone required on linux, not on windows
        #[allow(clippy::redundant_clone)]
        if let Some(agent_component) = manifest.components.clone().get(&ComponentType::phantom_agent) {
            if !agent_component.updated {
                // Sending pre-emptive failed status, agent will switch it back to updating (later updated) as soon as it's back up
                self.status_submitter.put_ota_status(
                    Some("Setting to fail while updating Phantom Agent".to_string()),
                    None,
                    NodeOtaProgressStatus::Failed,
                );
                // This will make agent panic on next run UNLESS it was installed and the env version was updated correctly
                manifest
                    .hash_manifest
                    .update_version_file(agent_component.version.clone())
                    .expect("Failed to update version file!");
                // Preemptively saving the hash as if the install was successful
                let new_component: Component = Component {
                    updated: true,
                    ..agent_component.clone()
                };

                #[allow(unused_variables)]
                    let manifest = manifest
                    .update_single_component(&new_component)
                    .unwrap()
                    .write_to_file()
                    .unwrap();

                #[cfg(windows)]
                {
                    let source = agent_component.path.clone().unwrap_or_default();
                    let source = source.to_string_lossy();
                    let target = "C:/Program Files/phantom_agent/bin/phantom_agent.exe";
                    let script = self.create_script(&source, target);
                    fs::write("agent_install.bat", script).expect("Unable to write file");

                    match BashExec::exec("powershell /c Start-Process agent_install.bat") {
                        Ok(_) => { log::info!("Started phantom agent install script"); }
                        Err(e) => { panic!("Failed to start phantom agent install script! {}", e); }
                    }
                    /* We are not triggering the windows_update service. The script will handle it instead
                    let flag_path = PathBuf::from(DOWNLOAD_DIR).join(WINDOWS_SERVICE_TRIGGER_PATH);
                    fs::write(
                        flag_path,
                        "THIS IS A TEMPORARY FILE FOR TRIGGERING WINDOWS UPDATE OF PHANTOM AGENT",
                    )
                    .expect("Failed to write into flag path!");
                     */
                    loop {
                        // The script is supposed to kill us at this point
                        log::info!("Queued phantom agent update (waiting to be killed)...");
                        sleep(Duration::new(1, 0));
                    }
                }

                #[cfg(unix)]
                {
                    // If the install component succeeds, this process should be DEAD! If we're still alive, it means we failed
                    if !self.install_or_uninstall_component(agent_component) {
                        // Reverting hash to indicate we failed to complete the installation process correctly
                        let new_component: Component = Component {
                            updated: false,
                            checksum: "".to_string(),
                            ..agent_component.clone()
                        };
                        return manifest
                            .update_single_component(&new_component)
                            .unwrap()
                            .write_to_file()
                            .unwrap();
                    }
                    return manifest;
                }
            }
        }
        manifest
    }

    pub fn save_prev_components(&self, manifest: Manifest, component_types: &[ComponentType]) -> Result<Manifest, OTAError> {
        let mut success_list = "".to_string();
        for (component_type, component) in &manifest.components {
            if component_types.contains(component_type) {
                if let Some(prev_dir) = component.previous_install_path.clone() {
                    if !success_list.is_empty() {
                        success_list += ", ";
                    }
                    success_list += &component.component;
                    if let Some(download_path) = component.path.clone() {
                        if prev_dir.exists() {
                            if let Err(e) = fs::remove_dir_all(&prev_dir) {
                                return Err(OTAError::fatal(format!("Failed to remove prev dir: {}", e)));
                            }
                        }
                        if let Err(e) = fs::create_dir_all(&prev_dir) {
                            return Err(OTAError::fatal(format!("Failed to create prev dir {}: {}", prev_dir.to_string_lossy(), e)));
                        }
                        if let Some(file) = download_path.file_name() {
                            let prev_path = prev_dir.join(file);
                            if let Err(e) = fs::copy(download_path, prev_path) {
                                return Err(OTAError::fatal(format!("Failed to save backup: {}", e)));
                            }
                        } else {
                            return Err(OTAError::fatal(format!("Failed to extract file name from {}", download_path.to_string_lossy())));
                        }
                    } else { // No path means this component was UNINSTALLED, so we need to remove its prev to match
                        if prev_dir.exists() {
                            if let Err(e) = fs::remove_dir_all(&prev_dir) {
                                return Err(OTAError::fatal(format!("Failed to remove prev dir: {}", e)));
                            }
                        }
                    }
                }
            }
        }
        log::info!("Saving previous installations succeeded: [{}]", success_list);
        Ok(manifest)
    }

    pub fn roll_back_component(&self, component: &Component) -> Result<Component, OTAError> {
        let (prev_exists, file) = component.uninstall_information();

        if component.path.is_some() && (!prev_exists || as_install_type(&component.package_type) == PackageType::MSI) {
            log::info!("Uninstalling {} for roll back", component.component);
            if let Err(error) = (self.install_command)(component, false) {
                log::error!("Failed to uninstall {}: {}", component.component, error.message);
            }
        }

        if prev_exists {    // Reinstalling previous component from stored installer
            let checksum = get_sha1_checksum(&file).unwrap_or_default();
            let component = Component {
                updated: false,
                path: Some(file.clone()),
                ..component.clone()
            };
            if self.install_or_uninstall_component(&component) {
                let component = Component {
                    updated: true,
                    checksum,
                    ..component
                };
                log::info!("Rolled back {} successfully", component.component);
                Ok(component)
            } else {
                Err(OTAError::nonfatal(format!("Failed to reinstall {:?}", file)))
            }
        } else {  // No previous component to fall back to
            let component = Component {
                updated: true,
                checksum: Default::default(),
                ..component.clone()
            };
            log::info!("No previous {} to install, roll back complete", component.component);
            Ok(component)
        }
    }

    pub fn roll_back_components(&self, manifest: Manifest, component_types: &Vec<ComponentType>) -> Result<Manifest, OTAError> {
        let mut message = "".to_string();
        for component_type in component_types {
            if !message.is_empty() {
                message += ", ";
            }
            message += &manifest.components[component_type].component;
            if *component_type == ComponentType::phantom_agent {
                log::error!("Rolling back agent requested (should be impossible)");
                return Err(OTAError::fatal("Attempted to roll back agent".to_string()));
            }
        }
        log::info!("Rolling back components: [{}]", message);
        let mut roll_back_all = true;
        let mut vec: Vec<Component> = manifest.components.into_values().collect();
        vec.sort_by(sort_by_package);
        let components = vec
            .into_iter()
            .map(|component| {
                let component_type = ComponentType::from_str(&component.component).unwrap();
                if component_types.contains(&component_type) {
                    match self.roll_back_component(&component) {
                        Ok(rolled_back_component) => {
                            (component_type, rolled_back_component)
                        }
                        Err(e) => {
                            roll_back_all = false;
                            log::error!("Failed to roll back {}: {}", &component.component, e.message);
                            (component_type, component)
                        }
                    }
                } else { (component_type, component) }
            })
            .collect();

        if roll_back_all {
            log::info!("{}", "All components were rolled back successfully".green(true));
            Ok(Manifest {
                components,
                ..manifest
            })
        } else {
            log::error!("{}", "Some components were not successfully rolled back!".red(true));
            Err(OTAError::fatal("Not all components were rolled back!".to_string()))
        }
    }

    pub fn restore_archives(&self, manifest: &Manifest) {
        info!("Restoring all archive components (post-install)");
        for (_, component) in manifest.components.clone() {
            if component.currently_installed() && as_install_type(&component.package_type) == PackageType::TAR {
                let (prev_exists, file) = component.uninstall_information();
                if prev_exists {    // Restoring tar from previously stored file
                    let component = Component {
                        updated: false,
                        path: Some(file.clone()),
                        ..component.clone()
                    };
                    if self.install_or_uninstall_component(&component) {
                        info!("Restored archive {}", component.component);
                    } else {
                        info!("Failed to restore archive {:?}", file);
                    }
                }
            }
        }
        info!("Restoring archive components complete");
    }

    pub fn install_manifest(&self, manifest: Manifest) -> Result<Manifest, OTAError> {
        log::info!("Installing manifest {:?}", manifest.components);
        if let Some(agent_component) = manifest.components.get(&ComponentType::phantom_agent) {
            if !agent_component.updated {
                (self.update_ota_status)(OTAStatus::INSTALLING(ComponentType::phantom_agent), None);
                return Ok(self.install_agent(manifest));
                // Note that NodeOtaStatus::Updated isn't sent until new run with updated agent, at the other components check
            }
        }

        let mut vec: Vec<Component> = manifest.components.into_values().collect();
        vec.sort_by(sort_by_package);
        let mut updated_all = true;
        let mut updated_list: Vec<ComponentType> = Vec::new();
        let components = vec
            .into_iter()
            .map(|component| {
                let component_type = ComponentType::from_str(&component.component).unwrap();
                if component.should_install() || component.should_uninstall() {
                    // Update status only one time
                    if updated_list.is_empty() {
                        self.status_submitter.put_ota_status(
                            None,
                            None,
                            NodeOtaProgressStatus::Installing,
                        );
                    }
                    (self.update_ota_status)(OTAStatus::INSTALLING(component_type), None);
                    let updated = self.install_or_uninstall_component(&component);
                    if updated {
                        updated_list.push(component_type);
                    }
                    updated_all = updated_all && updated;
                    let checksum = {
                        if updated {
                            if component.should_install() { component.checksum.clone() } // Install success
                            else { "".to_string() } // Uninstall success
                        }
                        else {  // Update failed, setting checksum to prev information or none
                            let (prev_exists, file) = component.uninstall_information();
                            if prev_exists {
                                match get_sha1_checksum(&file) {
                                    Ok(checksum) => { checksum }
                                    Err(_) => { "".to_string() }
                                }
                            }
                            else { "".to_string() }
                        }
                    };

                    (
                        component_type,
                        Component {
                            updated,
                            checksum,
                            ..component
                        },
                    )
                } else {
                    log::info!("{} - update is not required", component.component);
                    (component_type, component)
                }
            })
            .collect();

        if updated_all {
            #[cfg(unix)]
            SnapInstaller::cleanup_deprecated_if_needed(); // This will remove deprecated snap components if it detects them
            log::info!("{}", "All components were updated successfully".green(true));
            let manifest = Manifest {
                components,
                ..manifest
            };
            // Update the cloud OTA status only if actual software update occurred
            if !updated_list.is_empty() {
                info!("The updated_list is not empty, putting OTA status");
                self.status_submitter.put_ota_status(
                    None,
                    None,
                    NodeOtaProgressStatus::Updated,
                );
            } else {
                info!("The updated_list is empty, not putting OTA status");
            }
            let manifest = self.save_prev_components(manifest, &updated_list)?;
            self.restore_archives(&manifest);
            Ok(manifest)
        } else {
            let error = "Some components were not successfully updated!".to_string();
            self.status_submitter.put_ota_status(
                Some(error.clone()),
                None,
                NodeOtaProgressStatus::Failed,
            );
            log::error!("{}", error.red(true));
            if let Err(e) = send_snapshot_to_jira(JIRA_REPORT_TICKET, true) {
                log::error!("Snapshot error: {e}");
            }
            let manifest = Manifest {
                components,
                ..manifest
            };
            match self.roll_back_components(manifest, &updated_list) {
                Ok(manifest) => {
                    self.restore_archives(&manifest);
                    Err(OTAError::nonfatal("Failed install caused rollback".to_string()))
                }
                Err(e) => { Err(OTAError::fatal(e.message)) }
            }
        }
    }
}

// TODO: this should be impl for the num itself
// A function that sorts components so that phantom_agent is first and all the tar modules are last
fn sort_by_package(component1: &Component, component2: &Component) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    if ComponentType::from_str(&component1.component).unwrap() == ComponentType::phantom_agent {
        return Less;
    }
    if ComponentType::from_str(&component2.component).unwrap() == ComponentType::phantom_agent {
        return Greater;
    }
    match as_install_type(&component1.package_type) {
        PackageType::TAR => match as_install_type(&component2.package_type) {
            PackageType::TAR => Equal,
            _ => Greater,
        },
        _ => match as_install_type(&component2.package_type) {
            PackageType::TAR => Less,
            _ => Equal,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::license_manager::LicenseManager;
    use crate::auth::license_manager_trait::LicenseManagerTrait;
    use crate::ota::manifest::Manifest;
    use crate::rest_request::{RestServer, SendType, extract_token};
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::{ota, BashExec};
    use serde_json::Value;
    use std::fs;
    use std::path::{Path, PathBuf};
    use url::Url;

    fn read_function(_path: &Path) -> Result<String, String> {
        Ok(String::from(
            r#"
        {
            "core":"asdf",
            "phantom_agent":"fdsa"
        }"#,
        ))
    }

    fn simple_test_template(component_name: &str, operator: bool) {
        use crate::ota::service_control_trait::MockSystemControlTrait;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;

        let update_ota_status = |_ota_status, _message| {};
        let mut system_control_mock = MockSystemControlTrait::new();
        system_control_mock.expect_find_process().returning(|_| Vec::new());
        system_control_mock.expect_kill_process().returning(|_| Ok(()));
        let system_control = RefCell::new(system_control_mock);
        let mut status_submitter = MockCouplingRestSubmitter::new();
        status_submitter
            .expect_put_ota_status()
            .returning(|_, _, _| {});
        let manifest =
            Manifest::new(operator, Default::default(), Default::default(), Default::default(),
                          read_function, |_, _| Ok(())).unwrap();
        let install_command =   // Always returns successful installation no matter what
            |_component: &Component, _installing: bool| -> Result<String, OTAError> { Ok(String::from("ttt")) };
        let install_manager =
            InstallManager::new(&system_control, install_command, &status_submitter, update_ota_status);
        let component_type = ComponentType::from_str(component_name).unwrap();
        let component: Component = Component {
            component: component_name.to_string(),
            checksum: "89d9a94a717e4092eb064e95e67525ffe58339c16db8bbd1b89f24becac34ad3"
                .to_string(),
            updated: false,
            path: Some(std::env::current_dir().unwrap()),
            link: Some(Url::parse("https://phantomauto.jfrog.io/").unwrap()),
            target_path: Some(std::env::current_dir().unwrap()),
            version: "0.1.2".to_string(),
            token: Some("".to_string()),
            package_type: "snap".to_string(),
            previous_install_path: None,
            processes: vec![],
        };

        let manifest = manifest.update_single_component(&component).unwrap();
        assert!(
            manifest
                .components
                .get(&ComponentType::core)
                .unwrap()
                .updated
        );
        assert!(!manifest.components.get(&component_type).unwrap().updated);
        let manifest: Manifest = install_manager.install_manifest(manifest).unwrap();
        assert!(manifest.components.get(&component_type).unwrap().updated);
    }

    #[test]
    #[cfg(windows)]
    fn simple_test() {
        simple_test_template("oden_plugin", true)
    }

    #[test]
    #[cfg(unix)]
    fn simple_test() {
        simple_test_template("sim_gps_info", false)
    }

    fn read_function2(_: &Path) -> Result<String, String> {
        let result = r#" {
            "translator": "9685123541"
        } "#;
        Ok(result.to_string())
    }

    // Live test on machine
    #[test]
    #[ignore]
    fn live_snap_install() {
        use crate::ota::service_control_trait::MockSystemControlTrait;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        let system_control = RefCell::new(MockSystemControlTrait::new());
        let status_submitter = MockCouplingRestSubmitter::new();

        let manifest =
            Manifest::new(false, Default::default(), Default::default(), Default::default(),
                          read_function2, |_, _| Ok(())).unwrap();

        let component: Component = Component {
            component: "translator".to_string(),
            checksum: "9685123541".to_string(),
            updated: false,
            path: Some(PathBuf::from(
                "/root/snap/phantom-agent/common/download/vapp-translator_1.1.3_amd64.snap",
            )),
            link: Some(Url::parse("https://phantomauto.jfrog.io/").unwrap()),
            target_path: Some(std::env::current_dir().unwrap()),
            version: "0.1.2".to_string(),
            token: Some("".to_string()),
            package_type: "snap".to_string(),
            previous_install_path: None,
            processes: vec![],
        };

        let manifest = manifest.update_single_component(&component).unwrap();
        assert!(
            !manifest
                .components
                .get(&ComponentType::translator)
                .unwrap()
                .updated
        );
        let install_command = |component: &Component, _installing: bool|
                               -> Result<String, OTAError> {
            let snap_install_command = ota::snap_installer::SnapInstaller::install;
            snap_install_command(&component, BashExec::exec_arg)
        };
        let update_ota_status = |_ota_status, _message| {};
        let install_manager =
            InstallManager::new(&system_control, install_command, &status_submitter, update_ota_status);
        let manifest: Manifest = install_manager.install_manifest(manifest).unwrap();
        assert!(
            manifest
                .components
                .get(&ComponentType::translator)
                .unwrap()
                .updated
        );
    }

    #[cfg(test)]
    static mut TEST_FAKE_SERVER_REPLY: String = String::new();

    #[cfg(test)]
    unsafe fn set_test_fake_server_reply(reply: &str) {
        TEST_FAKE_SERVER_REPLY = reply.to_string();
    }

    #[cfg(test)]
    unsafe fn get_test_fake_server_reply() -> String {
        TEST_FAKE_SERVER_REPLY.clone()
    }


    // Live test on machine
    #[test]
    #[ignore]
    fn live_tar_install() {
        use crate::ota::download_manager::DownloadManager;
        use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        set_logging_for_tests(log::LevelFilter::Info);
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("tar_oden_install_test_dir"));
        //if test_dir.exists() { // Not DELETING the dir allows us to test partial download!
        //    fs::remove_dir_all(&test_dir).expect("Failed to create dir!");
        //}
        if !test_dir.exists() { // Not DELETING the dir allows us to test partial download!
            fs::create_dir(&test_dir).expect("Failed to create dir!");
        }

        let prev_dir = test_dir.join("PREV");
        if !prev_dir.exists() { // Not DELETING the dir allows us to test partial download!
            fs::create_dir(&prev_dir).expect("Failed to create dir!");
        }

        let remote_path = {
            #[cfg(unix)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/amd64/phantom-plugin_amd64-3.2.1.tar.gz").unwrap() }
            #[cfg(windows)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/win/phantom_plugin_win.zip").unwrap() }
        };
        let local_path = {
            #[cfg(unix)] { test_dir.join("phantom-plugin_amd64-3.2.1.tar.gz") }
            #[cfg(windows)] { test_dir.join("phantom_plugin_win.zip") }
        };

        #[cfg(unix)]
        BashExec::sync();

        log::info!("Local path is {}", local_path.to_string_lossy());
        log::info!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let mut license_manager =
            LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));

        if let Err(err) = license_manager.read_license() {
            log::info!("Couldn't load the license: {}", err);
            #[cfg(windows)]
            log::info!("Please manually copy the license into ./license (typically from C:/Program Files/phantom_agent/bin/license, and give it read permissions");
            #[cfg(unix)]
            log::info!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            unreachable!();
        }

        let license_token = LicenseManager::get_token(&license_manager, RestServer::get, RestServer::post).unwrap();
        //log::info!("Token is: {}", license_token);
        let jfrog_token = extract_token(&license_manager.get_url().unwrap(), license_token.as_str());
        //log::info!("Jfrog token is: {}", jfrog_token);

        let fake_server_reply = format!("[{{\"token\":\"{}\",\"_id\":\"641076169302da4daa2e54af\",\"component\":\"oden_plugin\",\"version\":\"0.1.2\",\"link\":\"{}\",\"checksum\":\"b39e1753cdcf85e28f68cbf7884223f565cd97b1\",\"arch\":\"WIN\"}}]", jfrog_token.clone(), remote_path.clone());
        unsafe {
            set_test_fake_server_reply(&fake_server_reply);
        }


        let rest_comm = CouplingRestComm::new(&license_manager,
                                              |_method: SendType,
                                               _url: &Url,
                                               _body: Option<&Value>,
                                               _authorization: Option<String>|
                                               -> Result<(String, u16), (String, u16)> { Ok((unsafe { get_test_fake_server_reply() }, 200)) });

        log::info!("Creating download manager");
        use crate::ota::service_control_trait::MockSystemControlTrait;

        let mut mock = MockSystemControlTrait::new();

        mock.expect_find_process().times(8).returning(|process| {
            log::info!("Mock finding process {}", process);
            Vec::new()
        });

        mock.expect_kill_process().times(4).returning(|process| {
            log::info!("Mock killing process {}", process);
            Ok(())
        });

        let mut status_submitter = MockCouplingRestSubmitter::new();

        status_submitter
            .expect_put_ota_status()
            .times(2)
            .returning(|_, _, status| {
                log::info!("Mock putting OTA status {:?}", status);
            });

        #[cfg(unix)]
            let _stops = 1;
        #[cfg(windows)]
            let _stops = 0;


        let sys_mock = RefCell::new(mock);

        let update_ota_status = |_status, _message| {};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_ota_status).unwrap();

        log::info!("Creating manifest");
        let write_function = |_: &Path, _: &str| Ok(());
        let read_function = |_: &Path| Ok(String::from(r#"{}"#));
        let server_name = license_manager.get_server().unwrap_or_default();
        let manifest =
            Manifest::new(true, PathBuf::from(""), prev_dir, server_name,
                          read_function, write_function).unwrap();

        #[cfg(windows)]
            let target_dir = PathBuf::from("C:/Program Files/Phantom Client");
        #[cfg(unix)]
            let target_dir = test_dir.join("target_dir");
        manifest.display_component_actions();
        manifest.standardize_prev_dir();
        //let oden_component = manifest.components.get(&ComponentType::oden_plugin).unwrap();
        //log::info!("ODEN COMPONENT PATH IS {}", oden_component.target_path.as_ref().unwrap().to_string_lossy());
        let manifest = download_manager.run(manifest).unwrap();
        manifest.display_component_actions();
        assert!(local_path.exists());
        let result_path = { // what the archive contains
            #[cfg(unix)] { target_dir.join(Path::new("target/release/libphantom_plugin.so")) }
            #[cfg(windows)] { target_dir.join(Path::new("phantom_plugin.dll")) }
        };

        let components = manifest.components.into_iter().map(|(component_type, component)| {
            if component_type == ComponentType::oden_plugin {
                (component_type, Component { target_path: Some(result_path.clone().parent().unwrap().to_path_buf()), ..component })
            } else { (component_type, component) }
        }).collect();
        let manifest = Manifest { components, ..manifest };
        //let oden_component = manifest.components.get(&ComponentType::oden_plugin).unwrap();
        //log::info!("ODEN COMPONENT PATH IS {}", oden_component.target_path.as_ref().unwrap().to_string_lossy());

        let install_command = |component: &Component, _installing: bool|
                               -> Result<String, OTAError> {
            #[cfg(unix)] { ota::tar_installer::TarInstaller::install_tar(&component, BashExec::exec_arg) }
            #[cfg(windows)] { ota::tar_installer::TarInstaller::install_zip(&component, BashExec::exec_arg) }
        };
        let update_ota_status = |_ota_status, _message| {};
        let install_manager = InstallManager::new(&sys_mock, install_command, &status_submitter, update_ota_status);
        let manifest = install_manager.install_manifest(manifest).unwrap();
        #[cfg(unix)]
        BashExec::sync();
        assert!(manifest.components.get(&ComponentType::oden_plugin).unwrap().updated);
        assert!(result_path.exists());
        fs::remove_dir_all(&test_dir).expect("Failed to create dir!");
    }

    // #[test]
    // #[ignore]
    // fn check_kill_process() {
    //     set_logging_for_tests(log::LevelFilter::Info);
    //     use crate::utils::process_utils::name_process;
    //     let name = "PhantomLauncher.exe";
    //     if let Some(oden_process) = find_process(name) {
    //         if let Some(name) = name_process(&oden_process) {
    //             log::info!("Found {}, killing...", name);
    //             match kill_process(oden_process) {
    //                 Ok(_) => {
    //                     log::info!("Killed successfully");
    //                 }
    //                 Err(e) => {
    //                     log::info!("Failed killing process: {}", e)
    //                 }
    //             }
    //         }
    //     } else {
    //         log::info!("{} not found, is it running?", name);
    //     }
    // }

    #[test]
    fn check_map_sort() {
        set_logging_for_tests(log::LevelFilter::Info);
        let write_function = |_: &Path, _: &str| Ok(());
        let manifest =
            Manifest::new(true, PathBuf::from(""), Default::default(), Default::default(),
                          read_function, write_function).unwrap();
        let mut vec: Vec<Component> = manifest.components.into_values().collect();
        vec.sort_by(sort_by_package);
        let mut check_agent_is_first = true;
        let mut check_tar_is_last = false;
        for v in vec {
            log::info!("Component: {:?} {}", &v.component, v.package_type);
            if check_agent_is_first {
                assert_eq!(
                    ComponentType::from_str(&v.component).unwrap(),
                    ComponentType::phantom_agent
                );
                check_agent_is_first = false;
            } else {
                assert_ne!(
                    ComponentType::from_str(&v.component).unwrap(),
                    ComponentType::phantom_agent
                );
            }
            if !check_tar_is_last {
                if v.package_type == "TAR" {
                    check_tar_is_last = true;
                }
            } else {
                assert_eq!(v.package_type, "TAR");
            }
        }
    }
}
