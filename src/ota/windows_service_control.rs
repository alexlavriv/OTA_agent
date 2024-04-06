
use std::ffi::OsStr;
use std::thread::sleep;
use std::time::Duration;

// use windows_service::service_manager::ServiceManager;
// use windows_service::{
//     service::ServiceAccess,
//     service_manager::{ServiceManager, ServiceManagerAccess},
// };

pub struct WindowsServiceControl {
    service_manager: ServiceManager,
}

use windows_service::{
    service::ServiceAccess,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

impl WindowsServiceControl {
    pub fn new() -> Result<Self, String> {
        let service_manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).unwrap();
        Ok(Self { service_manager })
    }

    pub fn start_service(&self, service_name: &str) -> windows_service::Result<()> {
        let service = self
            .service_manager
            .open_service(service_name, ServiceAccess::START)?;

        println!("Start {service_name}");
        service.start(&[OsStr::new("Started from Rust!")])?;
        println!("{service_name} has started");
        Ok(())
    }

    pub fn stop_service(&self, service_name: &str) -> windows_service::Result<()> {
        let service = self
            .service_manager
            .open_service(service_name, ServiceAccess::STOP)?;

        println!("Stop {service_name}");
        let service_state = service.stop()?;
        sleep(Duration::new(2, 0));
        println!("{:?}", service_state.current_state);

        Ok(())
    }
    pub fn handle_responese(result: windows_service::Result<()>) {
        if let Err(err) = result {
            println!("{err:?}");
        }
    }
}
// impl SystemControlTrait for WindowsServiceControl {

// fn find_process(&mut self, process_name: &str) -> Option<Vec<usize>>
//     { None}

// fn kill_process(&self, pid: usize) -> Result<(), String>{Ok(())}

// }



