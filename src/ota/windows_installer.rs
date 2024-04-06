// #[cfg(windows)]
// pub mod windows_installer_module {
//     use crate::ota::manifest::{
//         Component, ComponentType, Manifest, WINDOWS_PHANTOM_AGENT_PATH,
//         WINDOWS_SERVICE_TRIGGER_PATH, META_SERVER_NAME,  PREVIOUS_INSTALL_PATH,
//     };
//     use crate::ota::service_control_trait::SystemControlTrait;
//     use crate::service_trait::Action;
//     use crate::utils::file_utils::get_sha1_checksum;

//     use crate::ServiceTrait;
//     use log;
//     use std::fs;
//     use std::path::Path;
//     use std::path::PathBuf;
//     use std::string::String;
//     use std::thread::sleep;
//     use std::time::Duration;

//     pub struct WindowsInstaller<'a, A: SystemControlTrait> {
//         pub manifest_reader: fn(path: &Path) -> Result<String, String>,
//         pub hash_manifest_path: PathBuf,
//         pub write_function: fn(path: &Path, content: &str) -> Result<(), String>,
//         pub service_control: A,
//         pub move_function: fn(source: &Path, dest: &Path) -> Result<(), String>,
//         pub source_dir: &'a Path,
//         pub destination_dir: &'a Path,
//     }

//     impl<'a, T: SystemControlTrait> ServiceTrait for WindowsInstaller<'a, T> {
//         fn run(&self) {
//             todo!()
//         }

//         fn run_once(&self) -> Action {
//             self.run_once()
//         }
//     }

//     impl<'a, A: SystemControlTrait> WindowsInstaller<'a, A> {
//         pub fn update_manifest(&self, manifest: Manifest) -> Manifest {
//             let components = manifest
//                 .components
//                 .into_iter()
//                 .map(|(component_type, component)| {
//                     if component_type == ComponentType::phantom_agent {
//                         (
//                             component_type,
//                             Component {
//                                 installed: true,
//                                 ..component
//                             },
//                         )
//                     } else {
//                         (component_type, component)
//                     }
//                 })
//                 .collect();
//             Manifest {
//                 components,
//                 ..manifest
//             }
//         }
//         pub fn run_once(&self) -> Action {
//             // This will be running every couple seconds, so avoid any heavy lifting outside the conditional
//             let flag_path = self.source_dir.join(WINDOWS_SERVICE_TRIGGER_PATH);
//             let source_path = self.source_dir.join(WINDOWS_PHANTOM_AGENT_PATH);
//             let previous_install_path = match self.hash_manifest_path.parent() {
//                 None => PathBuf::from(format!("./{}", PREVIOUS_INSTALL_PATH)),
//                 Some(parent) => parent.join(PREVIOUS_INSTALL_PATH),
//             };
//             if flag_path.exists() && source_path.exists() {
//                 let manifest = Manifest::new(
//                     true,
//                     self.hash_manifest_path.clone(),
//                     previous_install_path.clone(),
//                     META_SERVER_NAME.to_string(),
//                     self.manifest_reader,
//                     self.write_function,
//                 )
//                 .unwrap();
//                 let phantom_agent_component = manifest
//                     .components
//                     .get(&ComponentType::phantom_agent)
//                     .unwrap()
//                     .clone();
//                 log::info!("Windows installer is installing phantom agent");
//                 fs::remove_file(flag_path).expect("Failed to remove flag file!");
//                 let checksum =
//                     get_sha1_checksum(&source_path).expect("Failed to extract checksum!");
//                 log::info!(
//                     "Checksum for {} calculated as {}",
//                     source_path.to_string_lossy(),
//                     &checksum
//                 );
//                 const SERVICE_NAME: &str = "windows_service.exe";
//                 const PROCESS_NAME: &str = "PhantomAgent";
//                 let destination_path = self.destination_dir.join(WINDOWS_PHANTOM_AGENT_PATH);
//                 if let Some(service) = find_process(SERVICE_NAME) {
//                     if let Some(name) = name_process(&service) {
//                         log::info!("Found {}, killing...", name);
//                         match kill_process(service) {
//                             Ok(_) => {
//                                 log::info!("Process killed successfully");
//                             }
//                             Err(e) => {
//                                 log::error!("Failed to kill process: {}", e);
//                                 return Action::CONTINUE;
//                             }
//                         }
//                     }
//                 } else {
//                     log::info!(
//                         "Process {} is not currently running, continuing...",
//                         SERVICE_NAME
//                     );
//                 }

//                 let mut attempts = 10; // We want to use move but it might fail because the system still holds the file after process stopped
//                 while attempts > 0 {
//                     attempts -= 1;
//                     sleep(Duration::new(1, 0));
//                     match (self.move_function)(source_path.as_path(), destination_path.as_path()) {
//                         Ok(_) => {
//                             let new_component: Component = Component {
//                                 installed: true,
//                                 checksum,
//                                 ..phantom_agent_component.clone()
//                             };
//                             // Saving hash before running the new process to not interfere with it
//                             manifest
//                                 .update_single_component(&new_component)
//                                 .unwrap()
//                                 .write_to_file()
//                                 .unwrap();

//                             // if self.service_control.start(PROCESS_NAME).is_ok() {
//                             //     log::info!("Windows installer updated phantom agent");
//                             // } else {
//                             //     log::error!("Windows installer failed to update phantom agent");
//                             //     // Reverting hash to indicate we failed to start the process correctly
//                             //     let manifest = Manifest::new(
//                             //         true,
//                             //         self.hash_manifest_path.clone(),
//                             //         previous_install_path,
//                             //         META_SERVER_NAME.to_string(),
//                             //         self.manifest_reader,
//                             //         self.write_function,
//                             //     )
//                             //     .unwrap();
//                             //     let new_component: Component = Component {
//                             //         installed: false,
//                             //         checksum: "".to_string(),
//                             //         ..phantom_agent_component
//                             //     };
//                             //     manifest
//                             //         .update_single_component(&new_component)
//                             //         .unwrap()
//                             //         .write_to_file()
//                             //         .unwrap();
//                             // }
//                             break;
//                         }
//                         Err(e) => {
//                             log::info!(
//                                 "Couldn't move file, error {}... {}",
//                                 e,
//                                 {
//                                     if attempts > 0 {
//                                         "retrying"
//                                     } else {
//                                         "giving up"
//                                     }
//                                 }
//                                 .to_string()
//                             );
//                             continue;
//                         }
//                     }
//                 }
//             }
//             Action::CONTINUE
//         }

//         #[allow(dead_code)]
//         pub(crate) fn install(_path: &Path) -> Result<String, String> {
//             // let result = WindowsInstaller::install_internal(path);
//             // if let Err(error) = result{
//             //     println!("{:?}", error);
//             //
//             // }
//             Ok(String::from("ok"))
//         }

//         #[allow(dead_code)]
//         pub fn install_internal(_path: &Path) -> windows_service::Result<()> {
//             use std::ffi::OsString;
//             use windows_service::{
//                 service::{
//                     ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
//                 },
//                 service_manager::{ServiceManager, ServiceManagerAccess},
//             };

//             let manager_access =
//                 ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
//             let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

//             // This example installs the service defined in `examples/ping_service.rs`.
//             // In the real world code you would set the executable path to point to your own binary
//             // that implements windows service.
//             let service_binary_path = std::env::current_exe()
//                 .unwrap()
//                 .with_file_name("ping_service.exe");

//             let service_info = ServiceInfo {
//                 name: OsString::from("ping_service"),
//                 display_name: OsString::from("Ping service"),
//                 service_type: ServiceType::OWN_PROCESS,
//                 start_type: ServiceStartType::OnDemand,
//                 error_control: ServiceErrorControl::Normal,
//                 executable_path: service_binary_path,
//                 launch_arguments: vec![],
//                 dependencies: vec![],
//                 account_name: None, // run as System
//                 account_password: None,
//             };
//             let service =
//                 service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
//             service.set_description("Windows service example from windows-service-rs")?;
//             Ok(())
//         }
//     }

//     #[cfg(test)]
//     mod test {
//         use crate::ota::windows_installer::windows_installer_module::WindowsInstaller;
//         use std::path::Path;

//         #[ignore]
//         #[test]
//         fn simple_test() {
//             // let path = Path::new("service.exe");
//             // let result = WindowsInstaller::install(path);
//             // result.unwrap();
//         }

//         fn read_function(_: &Path) -> Result<String, String> {
//             Ok(String::from(
//                 r#"
//             {}"#,
//             ))
//         }

//         #[test]
//         fn basic_test() {
//             use crate::ota::service_control_trait::MockSystemControlTrait;

//             let mut mock = MockSystemControlTrait::new();

//             mock.expect_stop().times(0).returning(|_| Ok(()));
//             mock.expect_start().times(0).returning(|_| Ok(()));

//             let windows_installer = WindowsInstaller {
//                 manifest_reader: read_function,
//                 hash_manifest_path: Default::default(),
//                 write_function: |_, _| Ok(()),
//                 service_control: mock,
//                 move_function: |_, _| Ok(()),
//                 source_dir: Path::new("/test"),
//                 destination_dir: Path::new("/test"),
//             };
//             windows_installer.run_once();
//         }
//     }
// }
