use log;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    string::String,
};

pub type ExecArgType = fn(&str, &[&str]) -> Result<String, String>;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;

pub struct BashExec;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

impl BashExec {
    // This function reads from stdin and outputs "| Command arg1 arg2"
    pub fn exec_pipe(command: &str, _arg: Option<&str>, input: &str) -> Result<String, String> {
        let mut split = command.split(' ');
        let program = split.next().unwrap();
        let mut command_cmd = Command::new(program);
        let command_cmd = command_cmd
            .args(split)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());

        let process = match command_cmd.spawn() {
            Err(why) => {
                let error = format!("Couldn't spawn {program}: {why}");
                log::error!("{}", error);
                return Err(error);
            }
            Ok(process) => process,
        };

        if let Err(why) = process.stdin.unwrap().write_all(input.as_bytes()) {
            let error = format!("Couldn't write to {program} stdin: {why}");
            log::error!("{}", error);
            return Err(error);
        }

        let mut output = String::new();
        if let Err(why) = process.stdout.unwrap().read_to_string(&mut output) {
            let error = format!("Couldn't read {program} stdout: {why}");
            log::error!("{error}");
            return Err(error);
        }
        // Trim the last char
        Ok(output.to_string())
    }
    pub fn exec_arg(windows_command: &str, args: &[&str]) -> Result<String, String> {
        Self::exec_arg_log(windows_command, args, true)
    }
    pub fn exec_arg_log(command: &str, args: &[&str], write_info_log: bool) -> Result<String, String> {
        let mut split = command.split(' ');
        let mut command_cmd = Command::new(split.next().unwrap());
        let command_cmd = command_cmd.args(split);

        for arg in args {
            command_cmd.arg(arg);
        }
        let log_entry = format!("bash exec executing: {:?}", command_cmd);

        if write_info_log {
            log::info!("{}", log_entry);
        } else {
            log::trace!("{}",log_entry);
        }
        #[cfg(windows)]
        command_cmd.creation_flags(CREATE_NO_WINDOW);
        match command_cmd.output() {
            Ok(output) if output.status.success() || !output.stdout.is_empty() => {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
            Ok(output) if !output.stderr.is_empty() => {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
            Err(error) => {
                log::error!(
                    "Error occurred while executing {}: {}",
                    command,
                    error
                );
                Err(format!("Error while executing {command}"))
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                log::error!(
                    "Error occurred\nstdout: {stdout}\nstderror: {stderr}\nstatus: {}\n",
                    output.status
                );
                Err(format!("Error while executing {command}"))
            }
        }
    }

    pub fn exec(command: &str) -> Result<String, String> {
        BashExec::exec_arg(command, &[])
    }
    pub fn exec_cmd(command: &str) -> Result<(), String> {
        Self::exec_cmd_write_info_log(command, true)
    }
    pub fn exec_cmd_write_info_log(command: &str, write_info_log: bool) -> Result<(), String> {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(command);
        #[cfg(windows)]
        shell.creation_flags(CREATE_NO_WINDOW);
        let result = shell.output();
        match result {
            Ok(_) => {
                if write_info_log {
                    log::info!("Running success: {}", command);
                } else {
                    log::trace!("Running success: {}", command);
                }
                Ok(())
            }
            Err(e) => {
                log::error!("Running error [{command}]: {}", e.to_string());
                Err(format!("error: {e}"))
            }
        }
    }

    pub fn list_files_in_archive(path: PathBuf, exec_command: ExecArgType) -> Result<Vec<String>, String> {
        let mut path_str = path.to_str().unwrap().to_string();
        if !path.is_absolute() {
            path_str = format!("./{path_str}");
        }
        // Creating a list of what the archive contains
        let list_flags = { "tf" };
        let command = format!("tar -{list_flags}");
        let text = match (exec_command)(&command, &[&path_str]) {
            Ok(text) => text,
            Err(e) => {
                return Err(format!("Couldn't list the archive: {e}"));
            }
        };
        let mut vec: Vec<String> = Vec::new();
        for line in text.lines() {
            vec.push(line.to_string());
        }
        Ok(vec)
    }

    #[cfg(unix)]
    pub fn sync() {
        log::info!("Syncing...");
        let now = std::time::Instant::now();
        if Self::exec("sync").is_ok() {
            log::info!("Synced in {:.2?}", now.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(unix, target_pointer_width = "64"))]
    fn simple_test() {
        let res = BashExec::exec("ls").unwrap();
        assert!(!res.is_empty());
        let res = BashExec::exec("ls -l").unwrap();
        assert!(!res.is_empty());
        let res = BashExec::exec("abs").unwrap_err();
        assert!(!res.is_empty());

        // let command = String::from("timeout 1 ping -I eno1 -c 1 8.8.8.8");
        // BashExec::exec(&command).unwrap();
    }

    #[test]
    #[cfg(all(unix, target_pointer_width = "64"))]
    fn pipe_test() {
        let res = BashExec::exec_pipe("wc -w", None, "two words").unwrap();
        let word_count: u64 = res.trim_end().parse().unwrap();
        assert_eq!(word_count, 2);
    }

    #[test]
    #[ignore]
    #[cfg(all(windows, target_pointer_width = "64"))]
    fn simple_test1() {
        //  log::info!("Bash test");
        let res = BashExec::exec("ping 8.8.8.1 -n 1 -w 1000").unwrap();
        log::info!("{}", res);
        let res = BashExec::exec("dir").unwrap();
        log::info!("{}", res);
        // let command = String::from("timeout 1 ping -I eno1 -c 1 8.8.8.8");
        // BashExec::exec(&command).unwrap();
    }

    #[test]
    fn test_schtasks() {
        let depricated_scheduled_task_name = "Start Phantom Agent";

        let res = BashExec::exec_arg("schtasks", &["/delete", "/tn", depricated_scheduled_task_name, "/f"]);
        println!("{:?}", res);
    }

    #[test]
    #[ignore]
    #[cfg(all(windows, target_pointer_width = "64"))]
    fn windows_wsl_command_test() {
        //wsl -d core -u root bash -ic "curl --unix-socket /run/snapd.socket http://localhost/v2/snaps -F snap=@/mnt/c/Users/Alex/Downloads/phau-core_2.0.34_amd64.snap -F dangerous=true -F classic=true"
        let windows_command = "wsl -d core -u root bash -ic";
        let wsl_command = "curl --unix-socket /run/snapd.socket http://localhost/v2/snaps -F snap=@/mnt/c/Users/Alex/Downloads/phau-core_2.0.34_amd64.snap -F dangerous=true -F classic=true";

        let res = BashExec::exec_arg(&format!("{} {}", windows_command, wsl_command), &[]).unwrap();
        log::info!("{}", res);
    }

    #[test]
    #[ignore]
    #[cfg(all(windows, target_pointer_width = "64"))]
    fn windows_extract_tar() {
        //wsl -d core -u root bash -ic "curl --unix-socket /run/snapd.socket http://localhost/v2/snaps -F snap=@/mnt/c/Users/Alex/Downloads/phau-core_2.0.34_amd64.snap -F dangerous=true -F classic=true"
        let windows_command =
            "tar -xvzf ./tar_oden_install_test_dir/phantom_plugin_win_d3766331.zip -C";
        let wsl_command = "C:/Program Files/OdenVR";

        let res = BashExec::exec_arg(&format!("{} {}", windows_command, wsl_command), &[]).unwrap();
        log::info!("{}", res);
    }
}
