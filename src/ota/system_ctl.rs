use crate::ota::service_control_trait::SystemControlTrait;
use log;
use sysinfo::{ProcessRefreshKind, RefreshKind, SystemExt, ProcessExt};

pub struct SystemCtl {
    sysinfo_handle: sysinfo::System,
}

impl Default for SystemCtl {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemCtl {
    pub fn new() -> Self {
        let sysinfo_handle = sysinfo::System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::new()),
        );
        Self {  sysinfo_handle }
    }

}

impl SystemControlTrait for SystemCtl {
    fn find_process(&mut self, process_name: &str) -> Vec<usize> {
        self.sysinfo_handle.refresh_processes();
        self.sysinfo_handle
            .processes()
            .iter()
            .filter(|(_, element)| element.name() == process_name).map(|(_, element)| element.pid().into())
            .collect::<Vec<_>>()
    }

    fn kill_process(&self, pid: usize) -> Result<(), String> {
        let matching_processes = self.sysinfo_handle
            .processes()
            .iter()
            .find(|(_, element)| element.pid() == pid.into());

        match matching_processes {
            Some((_, element)) => {
                log::info!("Found {}, stopping...", element.name());
                if element.kill() {
                    log::info!("Process {} with PID {pid} killed successfully", element.name());
                    Ok(())
                } else {
                    let error_msg = format!("Failed to find process {}", pid);
                    log::error!("{}", error_msg);
                    Err(error_msg)
                }
            }
            None => {
                let error_msg = format!("Failed to kill process {}", pid);
                log::error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }
}

#[test]
#[ignore]
fn kill_proc() {
    let mut system_ctl = SystemCtl::new();
    let list = system_ctl.find_process("notepad.exe");
    if list.is_empty() {
        println!("Could not find the process")
    }
    else {
        for pid in list {
            match system_ctl.kill_process(pid) {
                Ok(_) => {println!("killed {pid}")},
                Err(error) => { println!("failed killing {pid} with the following {error}") }
            }
        }
    }
}

// #[test]
// fn stop_test() {
//     let system_ctl = SystemCtl::new();
//     let process_name = "snap.phantom-agent.phantom-agent-daemon.service";
//     if let Err(e) = system_ctl.stop(&process_name) {
//         println!("The error is {}", e);
//     }
// }
