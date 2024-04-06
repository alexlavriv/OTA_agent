use crate::{
    logger::logging_configuration::logs_dir,
    rest_comm::{
        coupling_rest_comm::CouplingRestComm,
        coupling_submit_trait::{CouplingRestSubmitter, NodeOtaProgressStatus},
    },
    utils::zip_utils::Zip,
};

#[cfg(windows)]
use crate::{ ota::msi_installer::MsiInstaller, utils::tasklist::create_tasklist_file };

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};
use hyper::Uri;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use crate::rest_comm::coupling_rest_comm::fetch_coupling_rest_comm;
use crate::utils::log_utils::hostname;



static MINIMAL_JIRA_INTERVAL_SECONDS: u64 = 60 * 60 * 24 * 3; // 60*60*24*3 is THREE DAYS
static STANDARD_LOG_LIMIT: usize = 5000000; // 5MB
static SMALL_LOG_LIMIT: usize = 100000; // 100KB
pub(crate) static JIRA_REPORT_TICKET: &str = "DEV-12719";
static JIRA_REPORT_FLAG: &str = "jira_report_flag";

pub struct JiraLogSubmitter {
    pub logs_dir: PathBuf,
    pub flag: PathBuf,
    pub zip_dir: PathBuf,
    pub msi_log_dir: PathBuf,
    fetch_coupling_rest_comm: fn() -> Result<CouplingRestComm, String>,
}

pub fn send_snapshot_to_jira(ticket: &str, force: bool) -> Result<(), String> {
    let submitter = JiraLogSubmitter::new();
    let result = submitter.send_snapshot_to_jira(ticket, force);

    if let Err(e) = result.clone() {
        log::error!("{}", e);
    }

    if !force {
        match fetch_coupling_rest_comm() {
            Ok(coupling_rest_comm) => {
                coupling_rest_comm.put_ota_status(None, None, NodeOtaProgressStatus::Failed);
            }
            Err(e) => { log::error!("Cannot fetch Coupling Rest Comm for ota status: {}", e); }
        }
    }

    result
}

impl Default for JiraLogSubmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl JiraLogSubmitter {
    pub fn new() -> Self {
        #[cfg(unix)]
        let (zip_dir, msi_log_dir) = (PathBuf::from(
            std::env::var("SNAP_USER_COMMON").expect("Can't read SNAP_USER_COMMON from env!"),
        ), PathBuf::from("./"));
        #[cfg(windows)]
        let (zip_dir, msi_log_dir) = (PathBuf::from("./"), MsiInstaller::log_dir());
        Self {
            logs_dir: logs_dir(),
            flag: zip_dir.join(JIRA_REPORT_FLAG),
            zip_dir,
            msi_log_dir,
            fetch_coupling_rest_comm,
        }
    }

    fn check_flag(&self) -> Result<(bool, String), String> {
        let now = SystemTime::now();
        match File::open(&self.flag) {
            Ok(file) => {
                let modified = file
                    .metadata()
                    .map_err(|_| "Failed to get metadata!")?
                    .modified()
                    .map_err(|_| "Failed to get modified!")?;
                let date = OffsetDateTime::from(modified).format(&Rfc3339).unwrap();
                log::info!("Found flag time, last update was {}", date);
                match now.duration_since(modified) {
                    Ok(duration) => {
                        if duration.as_secs() > MINIMAL_JIRA_INTERVAL_SECONDS {
                            let mut file = File::create(&self.flag)
                                .map_err(|_| "Failed to overwrite flag file!")?;
                            file.write_all(b" ").map_err(|_| "Write fail!")?;
                            let modified = file
                                .metadata()
                                .map_err(|_| "Failed to get metadata!")?
                                .modified()
                                .map_err(|_| "Failed to get modified!")?;
                            let date = OffsetDateTime::from(modified).format(&Rfc3339).unwrap();
                            Ok((true, date[0..10].to_string()))
                        } else {
                            log::warn!(
                                "Interval between reports too small, not sending report to jira"
                            );
                            Ok((false, date[0..10].to_string()))
                        }
                    }
                    Err(e) => {
                        log::error!("Could not calculate duration! ({})", e);
                        Ok((false, date[0..10].to_string()))
                    }
                }
            }
            Err(_) => {
                let file = File::create(&self.flag).map_err(|_| "Failed to create flag file!")?;
                let modified = file
                    .metadata()
                    .map_err(|_| "Failed to get metadata!")?
                    .modified()
                    .map_err(|_| "Failed to get modified!")?;
                let date = OffsetDateTime::from(modified).format(&Rfc3339).unwrap();
                log::info!("First time flag checking, time is {}", date);
                Ok((true, date[0..10].to_string()))
            }
        }
    }

    pub fn create_snapshot(&self, zip_path: &Path, license_path: &Path, small: bool) -> Result<(), String> {
        let limit = if small { SMALL_LOG_LIMIT } else { STANDARD_LOG_LIMIT };
        if zip_path.exists() {
            fs::remove_file(zip_path).map_err(|_| "File exists and couldn't remove it".to_string())?;
        }
        let mut zip_file = Zip::new(zip_path);
        if license_path.exists() {
            zip_file.add_file_to_zip(license_path)?;
        }
        #[cfg(windows)]
        {
            Self::zip_directory_with_pattern(&self.msi_log_dir, "install_log", ".txt", &mut zip_file);
            if let Ok(tasklist_file) = create_tasklist_file() {
                zip_file.add_file_to_zip_with_limit(&tasklist_file, limit)?;
            }
        }
        zip_file.add_file_to_zip_with_limit(&self.logs_dir.join("phantom_agent.log"), limit)?;
        zip_file.finish()
    }

    pub fn zip_directory_with_pattern(dir_path: &Path, pattern: &str, extension: &str, zip_file: &mut Zip) {
        if dir_path.exists() && dir_path.is_dir() {
            let paths: Vec<_> = fs::read_dir(dir_path).unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path()).collect();

            for path in paths {
                if path.is_file() {
                    let path_str = path.to_string_lossy().to_string();
                    if (pattern.is_empty() || path_str.contains(pattern)) &&
                        (extension.is_empty() || path_str.ends_with(extension)) {
                        if let Err(e) = zip_file.add_file_to_zip_with_limit(&path, SMALL_LOG_LIMIT) {
                            log::info!("Could not add file to zip [{}]: {}", path.to_string_lossy(), e);
                        }
                        else {
                            log::info!("Added file to zip [{}]", path.to_string_lossy());
                        }
                    }
                }
            }
        }
        else { log::info!("Skipping [{}]: directory not found", dir_path.to_string_lossy()); }
    }

    pub fn send_snapshot_to_jira(&self, ticket: &str, force: bool) -> Result<(), String> {
        let date = {
            if force {
                let date = OffsetDateTime::from(SystemTime::now()).format(&Rfc3339).unwrap();
                date[0..10].to_string()
            }
            else {
                let (flag, date) = self.check_flag()?;
                if !flag {
                    return Ok(());
                }
                date
            }
        };
        let coupling_rest_comm = (self.fetch_coupling_rest_comm)()?;
        log::info!("Sending report to jira");
        let name = if coupling_rest_comm.named { hostname() } else { coupling_rest_comm.name.clone() };
        let zip_path = self.zip_dir.join(format!("{}_{}.zip", date, name));
        self.create_snapshot(&zip_path, &coupling_rest_comm.path, false)?;

        let thread_path = self.zip_dir.join(format!("{}_{}.zip", date, name));
        let thread_ticket = ticket.to_string();

        let result = match std::thread::spawn(move || -> Result<String, String> {
            coupling_rest_comm.send_file_to_jira(&thread_path, &thread_ticket)
        }).join() {
            Ok(result) => { result }
            Err(_) => {
                log::error!("Crash in check version thread");
                Err("Check version thread crashed".to_string())
            }
        };

        result?;
/*
        if let Err(e) = result {
            if e.contains("413") && e.contains("Too Large") {
                log::info!("File was too large, attempting to send smaller report");
                let coupling_rest_comm = (self.fetch_coupling_rest_comm)()?;
                self.create_snapshot(&zip_path, &coupling_rest_comm.path, true)?;
                let thread_path = self.zip_dir.join(format!("{}_{}.zip", date, name));
                let thread_ticket = ticket.to_string();
                match std::thread::spawn(move || -> Result<String, String> {
                    coupling_rest_comm.send_file_to_jira(&thread_path, &thread_ticket)
                }).join() {
                    Ok(result) => {
                        if let Err(e) = result { return Err(e); }
                    }
                    Err(_) => {
                        log::error!("Crash in check version thread");
                        return Err("Check version thread crashed".to_string());
                    }
                }
            }
            else { return Err(e); }
        }
*/
        log::info!("Successfully sent report to jira");
        if let Err(e) = fs::remove_file(&zip_path) {
            return Err(e.to_string());
        }

        Ok(())
    }

    pub fn send_custom_log(uri: Uri, _: String) -> Result<String, String> {
        let parts = uri.path().split('/').collect::<Vec<&str>>();
        if parts.len() < 2 || parts[1].to_lowercase() != "log" {
            log::warn!("Custom log requested, but {} doesn't match expected format", uri);
            return Err("Unexpected URI format".to_string());
        }
        if parts.len() == 2 || parts[2].is_empty() {
            return match send_snapshot_to_jira(JIRA_REPORT_TICKET, true) {  // Forcing (not setting status to failed)
                Ok(_) => { Ok(format!("Log sent to {}", JIRA_REPORT_TICKET)) }
                Err(e) => { Err(e) }
            };
        }
        let ticket = parts[2].to_uppercase();
        let ticket = match ticket.starts_with("DEV-") {
            true => {&ticket[4..]}
            false => {&ticket}
        };

        let mut valid = !ticket.is_empty();
        for c in ticket.chars() {
            if !c.is_numeric() {
                valid = false;
            }
        }
        if !valid {
            log::warn!("Custom log requested, but ticket [{}] is invalid", ticket);
            return Err("Invalid ticket".to_string());
        }
        let ticket = "DEV-".to_owned() + ticket;
        match send_snapshot_to_jira(&ticket, true) {  // Forcing (not setting status to failed)
            Ok(_) => { Ok(format!("Custom log sent to {}", ticket)) }
            Err(e) => { Err(e) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest_request::RestServer;
    use crate::utils::file_utils;
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::auth::license_manager::LicenseManager;
    use crate::auth::license_manager_trait::LicenseManagerTrait;
    use crate::BashExec;
    use log;
    use std::fs;

    fn test_fetch_coupling_rest_comm() -> Result<CouplingRestComm, String> {
        let mut license_manager = LicenseManager::from_path(std::env::current_dir().unwrap().join("license"));
        if let Err(err) = license_manager.read_license() {
            log::info!("Couldn't load the license: {}", err);
            #[cfg(windows)]
            log::info!("Please manually copy the license into ./license (typically from C:/Program Files/phantom_agent/bin/license, and give it read permissions");
            #[cfg(unix)]
            log::info!("Please manually copy the license into ./license (typically from /root/snap/phau-core/common/license, and give it read permissions");
            unreachable!()
        }
        let coupling_rest_comm = CouplingRestComm {
            url: license_manager.get_url().unwrap(),
            token: LicenseManager::get_token(&license_manager, RestServer::get, RestServer::post).unwrap_or_default(),
            name: license_manager.get_name().unwrap_or_default(),
            send: RestServer::send_json,
            path: PathBuf::from("./jira_test_file"),
            named: false
        };
        Ok(coupling_rest_comm)
    }
    #[test]
    #[ignore]
    fn jira_send_file_test() {
        set_logging_for_tests(log::LevelFilter::Info);
        let jira_submitter = JiraLogSubmitter {
            logs_dir: Default::default(),
            flag: Default::default(),
            zip_dir: PathBuf::from("./"),
            msi_log_dir: PathBuf::from("./"),
            fetch_coupling_rest_comm: test_fetch_coupling_rest_comm,
        };
        let file = PathBuf::from("./jira_test_file");
        (file_utils::string_to_file)(&file, "1234567890").expect("File fail!");
        jira_submitter.send_snapshot_to_jira( JIRA_REPORT_TICKET, true).unwrap();
        fs::remove_file(file).expect("Remove fail!");
    }

    #[test]
    #[ignore]
    #[cfg(windows)]
    fn jira_send_snapshot_test() {
        set_logging_for_tests(log::LevelFilter::Info);
        let top = std::env::current_dir().expect("Current dir fail!");
        let test_dir = top.join("send_snapshot_test");
        #[cfg(unix)]
        let real_log_dir = PathBuf::from("/snap/phantom-agent/log/");
        #[cfg(windows)]
        let real_log_dir = PathBuf::from("C:/Program Files/phantom_agent/log/");
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to remove old dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        fs::copy(
            real_log_dir.join("phantom_agent.log"),
            test_dir.join("phantom_agent.log"),
        )
        .expect("Copy fail!");
        if fs::copy(real_log_dir.join("log1.log"), test_dir.join("log1.log")).is_err() {
            println!("Log1 doesn't exist!");
        }
        if fs::copy(real_log_dir.join("log2.log"), test_dir.join("log2.log")).is_err() {
            println!("Log2 doesn't exist!");
        }
        let jira_submitter = JiraLogSubmitter {
            logs_dir: test_dir.clone(),
            flag: PathBuf::from("./flag"),
            zip_dir: PathBuf::from("./"),
            msi_log_dir: MsiInstaller::log_dir(),
            fetch_coupling_rest_comm: test_fetch_coupling_rest_comm
        };

        jira_submitter.send_snapshot_to_jira(JIRA_REPORT_TICKET, true).expect("Send fail!");
        //fs::remove_dir_all(test_dir).expect("Remove fail!");
        //fs::remove_file(jira_submitter.flag).expect("Remove fail!");
    }

    #[test]
    #[ignore]
    fn jira_check_flag() {
        set_logging_for_tests(log::LevelFilter::Info);
        let jira_submitter = JiraLogSubmitter {
            logs_dir: Default::default(),
            flag: PathBuf::from("./flag"),
            zip_dir: PathBuf::from("./"),
            msi_log_dir: PathBuf::from("./"),
            fetch_coupling_rest_comm
        };

        let (flag, date) = jira_submitter.check_flag().expect("Check flag fail!");
        log::info!("Flag is {}, date is {}", flag, date);
        assert!(flag);
        let (flag, date) = jira_submitter.check_flag().expect("Check flag fail!");
        log::info!("Flag is {}, date is {}", flag, date);
        assert!(!flag);
        #[cfg(unix)]
        {
            BashExec::exec_arg("touch", &[&jira_submitter.flag.to_string_lossy(), "-d 2 weeks ago"]).unwrap();
        }
        #[cfg(windows)]
        {
            let command = format!("powershell");
            let arg = format!(
                "(Get-Item \"{}\").LastWriteTime=(\"14 August 2016 13:14:00\")",
                jira_submitter.flag.to_string_lossy()
            );
            BashExec::exec_arg(&command, &[&arg]).unwrap();
        }
        let (flag, date) = jira_submitter.check_flag().expect("Check flag fail!");
        log::info!("Flag is {}, date is {}", flag, date);
        assert!(flag);
        let (flag, date) = jira_submitter.check_flag().expect("Check flag fail!");
        log::info!("Flag is {}, date is {}", flag, date);
        assert!(!flag);
        fs::remove_file(jira_submitter.flag).expect("Remove fail!");
    }

    #[test]
    fn test_hostname() {
        let hostname = hostname();
        assert_ne!(hostname, "DEFAULT");
    }
}
