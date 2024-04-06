use std::collections::HashMap;
use std::env::temp_dir;
use std::fs;
use std::path::PathBuf;
use log::{error, info};
use crate::BashExec;
use crate::utils::file_utils::string_to_file;

pub fn is_numeric(str: &str) -> bool {
    for c in str.chars() {
        if !c.is_numeric() {
            return false;
        }
    }
    true
}

pub struct Tasklist {
    dlls: HashMap<String, Vec<String>>,
    paths: HashMap<String, String>,
}

pub fn get_tasklist_csv() -> Result<String, String> {
    BashExec::exec_arg("tasklist", &["/M", "/fo", "csv"])
}

pub fn get_wmic_csv() -> Result<String, String> {
    BashExec::exec_arg("powershell", &["Get-CimInstance Win32_Process | select ExecutablePath,ProcessId | ConvertTo-Csv"])
}

pub fn create_tasklist_file() -> Result<PathBuf, String> {
    let temp_dir = temp_dir().join("TMP_TASKLIST");
    if temp_dir.exists() { fs::remove_dir_all(temp_dir.clone()).expect("Failed to clear temp dir"); }
    fs::create_dir(temp_dir.clone()).expect("Failed to create temp dir");
    let report_path = temp_dir.join("tasklist_dlls.log");
    let report = get_tasklist_csv()?;
    string_to_file(&report_path, &report)?;
    Ok(report_path)
}

fn rem_first_and_last(value: &str) -> &str {
    let mut chars = value.chars();
    chars.next();
    chars.next_back();
    chars.as_str()
}

impl Tasklist {
    pub fn new() -> Self {
        let mut dlls: HashMap<String, Vec<String>> = HashMap::new();
        match get_tasklist_csv() {
            Ok(list) => {
                let mut lines_count = 0;
                let mut dll_count = 0;
                for line in list.lines() {
                    let parts: Vec<&str> = line.split('\"').collect();
                    if parts.len() == 7 {
                        let (_name, pid, list) = (parts[1], parts[3], parts[5]);
                        if list != "N/A" || is_numeric(pid) {
                            let dll_list: Vec<&str> = list.split(',').collect();
                            dll_count += dll_list.len();
                            for dll in dll_list {
                                let lower = dll.to_lowercase();
                                if !dlls.contains_key(&lower) {
                                    dlls.insert(lower.to_owned(), Vec::new());
                                }
                                dlls.get_mut(&lower).expect("Hashmap fail!").push(pid.to_owned());
                            }
                        }
                        lines_count += 1;
                    }
                }
                let keys = dlls.keys();
                info!("Tasklist found {} processes, {} dlls, total {} keys", lines_count, dll_count, keys.len());
                /*for key in keys {
                    let vec = dlls.get(key).expect("Hashmap fail!");
                    info!("{}: {:?}", key, vec);
                }*/
            }
            Err(e) => {
                error!("Failed to run the tasklist command: {}", e);
            }
        }
        let mut paths: HashMap<String, String> = HashMap::new();
        match get_wmic_csv() {
            Ok(list) => {
                for line in list.lines() {
                    let parts: Vec<&str> = rem_first_and_last(line).split("\",\"").collect();
                    if parts.len() == 2 {
                        let (path, pid) = (parts[0].trim(), parts[1].trim());
                        if !paths.contains_key(&pid.to_owned()) {
                            paths.insert(pid.to_owned(), path.to_owned());
                        }
                    }
                }
                info!("Tasklist mapped {} paths", paths.len());
            }
            Err(e) => {
                error!("Failed to run the tasklist wmic command: {}", e);
            }
        }
        Self { dlls, paths }
    }

    pub fn check_dll(&self, dll: &str) -> Vec<(String, String)> {
        let lower = dll.to_lowercase();

        let pids = if self.dlls.contains_key(&lower) {
            self.dlls.get(&lower).expect("Hashmap fail!").to_owned()
        }
        else { Vec::new() };

        let mut pids_paths = Vec::new();
        for pid in pids {
            if self.paths.contains_key(&pid) {
                pids_paths.push((pid.clone(), self.paths.get(&pid).expect("Hashmap fail!").to_owned()));
            }
        }
        pids_paths
    }
}

impl Default for Tasklist { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use crate::utils::log_utils::set_logging_for_tests;
    use super::*;

    #[test]
    #[ignore]
    fn tasklist_test() {
        set_logging_for_tests(log::LevelFilter::Info);
        let _tasklist = Tasklist::new();
    }
}