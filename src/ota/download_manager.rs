use crate::{config::{get_arch, ArchType}, ota::{
    disk_space_verifier::DiskSpaceVerifier,
    manifest::{ComponentType, Manifest},
    ota_error::OTAError,
    service_control_trait::SystemControlTrait,
}, rest_comm::coupling_submit_trait::CouplingRestSubmitter, rest_request::{DownloadStats, RestServer}};
use crate::ota::ota_status::OTAStatus;
use futures_util::future::join_all;
use log;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    str,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};
use url::Url;


const DOWNLOAD_ATTEMPTS: i32 = 5;

#[derive(Serialize, Deserialize, Clone)]
pub struct CheckSums {
    checksums: HashMap<ComponentType, String>,
    arch: ArchType,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Arch {
    arch: ArchType,
}

pub struct DownloadManager<'a, A: SystemControlTrait> {
    _system_control: &'a RefCell<A>,
    coupling_rest_submitter: &'a dyn CouplingRestSubmitter,
    dest_path: PathBuf,
    disk_space_verifier: Option<DiskSpaceVerifier>,
    update_ota_status: fn(OTAStatus, Option<String>),
}

async fn report_eta(url: Url, token: String, update_ota_status:  fn(OTAStatus, Option<String>),
    stats_ptr: Arc<Mutex<DownloadStats>>
) -> bool {
    while stats_ptr.lock().unwrap().download_count != 0 {
        let eta = stats_ptr.lock().unwrap().eta;
        update_ota_status(OTAStatus::DOWNLOADING(eta), None);
        RestServer::report_eta(url.clone(), &token, eta).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    true
}

async fn download(
    url: Url,
    path: PathBuf,
    checksum: Option<String>,
    token: String,
    stats_ptr: Arc<Mutex<DownloadStats>>,
    callback: fn(&str, u64, u64, Arc<Mutex<DownloadStats>>),
) -> bool {
    let mut attempt = 1;
    loop {
        let result = RestServer::download_file_with_callback(
            &url,
            path.clone(),
            checksum.clone(),
            &token,
            stats_ptr.clone(),
            callback,
        )
        .await;
        match result {
            Err(e) => {
                log::warn!(
                    "Error in downloading {} into {}: {}",
                    url,
                    path.to_string_lossy(),
                    e
                );
                if attempt >= DOWNLOAD_ATTEMPTS {
                    log::error!("Giving up on the download for {}", path.to_string_lossy());
                    stats_ptr.lock().unwrap().dec_download_count();
                    return false;
                }
                sleep(Duration::from_secs(5));
                attempt += 1;
                log::info!("Retrying the download for {} (attempt {})", path.to_string_lossy(), attempt);
            }
            Ok(s) => {
                log::info!(
                    "Downloaded component: {} into {}: {}",
                    url,
                    path.to_string_lossy(),
                    s
                );
                stats_ptr.lock().unwrap().dec_download_count();
                return true;
            }
        }
    }
}

impl<'a, A: SystemControlTrait> DownloadManager<'a, A> {
    pub fn new(
        _system_control: &'a RefCell<A>,
        coupling_rest_submitter: &'a dyn CouplingRestSubmitter,
        dest_path: PathBuf,
        update_ota_status: fn(OTAStatus, Option<String>),

    ) -> Result<Self, String> {
        let disk_space_verifier = match DiskSpaceVerifier::new() {
            Ok(verifier) => Some(verifier),
            Err(error) => {
                log::error!("Failed initiating disk space verifier with error {}", error);
                None
            }
        };
        Ok(Self {
            _system_control,
            coupling_rest_submitter,
            dest_path,
            disk_space_verifier,
            update_ota_status
        })
    }

    pub fn run(&self, manifest: Manifest) -> Result<Manifest, OTAError> {
        log::info!("Download Manager running...");
        // post the checksum values of components
        let server_checksum_response = self.post_empty_checksums()?;
        let json_response: Value = serde_json::from_str(server_checksum_response.as_str()).map_err(|error|{
            log::error!("Failed parsing to JSON with the following error: {error},\n post_checksum response is: {server_checksum_response}");
            OTAError::nonfatal("Failed parsing post_checksums response".to_string())
        })?;
        let json_response = serde_json::to_string_pretty(&json_response).unwrap();
        log::info!("Response: {json_response}");
        if json_response == "[]" {
            log::info!("The response is empty, no download is needed");
            return Ok(manifest);
        }
        let manifest_res = manifest.update_with_json(server_checksum_response.as_str());
        if let Err(resp) = manifest_res {
            return Err(OTAError::nonfatal(format!(
                "Failed converting server response to json: {resp}"
            )));
        }
        let manifest = manifest_res.unwrap();
        #[cfg(test)]
        manifest.display_component_actions();
        // Check if there is enough disk space for downloading components
        match &self.disk_space_verifier {
            Some(disk_verifier) => match disk_verifier.verify(&manifest) {
                Ok(result) => {
                    if !result {
                        return Err(OTAError::fatal(
                            "No available disk space for downloading components".to_string(),
                        ));
                    }
                }
                Err(error) => {
                    return Err(OTAError::nonfatal(format!(
                        "Error occurred while checking disk space {error}. Update stopped"
                    )));
                }
            },
            None => log::error!("Failed creating disk space verifier"),
        }
        if manifest.is_fully_installed() {
            log::info!("The manifest is fully installed, no download is needed");
            return Ok(manifest);
        }
        // not deleting the contents of destination folder here allows resuming partial downloads
        log::info!("Downloading components");
        self.download_components(self.dest_path.clone(), manifest)
    }

    pub fn _post_checksums(&self, manifest: &Manifest) -> Result<String, String> {
        let check_sums = self._convert_manifest_to_server_manifest(manifest)?;
        let check_sums_json = serde_json::to_value(check_sums).unwrap();
        self.coupling_rest_submitter.post_checksums(check_sums_json)
    }

    pub fn post_empty_checksums(&self) -> Result<String, String> {
        //let check_sums = CheckSums { arch: get_arch(), checksums: Default::default() };
        let check_sums = Arch { arch: get_arch() };
        let check_sums_json = serde_json::to_value(check_sums).unwrap();
        self.coupling_rest_submitter.post_checksums(check_sums_json)
    }

    fn _convert_manifest_to_server_manifest(
        &self,
        manifest: &Manifest,
    ) -> Result<CheckSums, String> {
        // get arch
        let arch_enum = get_arch();
        let checksums = manifest
            .components
            .iter()
            .map(|(component_type, component)|
            (*component_type, component.checksum.clone()))
            .collect();
        Ok(CheckSums {
            checksums,
            arch: arch_enum,
        })
    }

    // tokio treats async function as synced one
    #[tokio::main]
    async fn download_components(
        &self,
        dest_path: PathBuf,
        manifest: Manifest
    ) -> Result<Manifest, OTAError> {
        let mut components_paths = HashMap::new();
        let mut futures : Vec<std::pin::Pin<Box<dyn std::future::Future<Output=bool>>>> = vec![];
        let mut paths = vec![];

        let stats_ptr = Arc::new(Mutex::new(DownloadStats::new()));
        for (component_type, component) in &manifest.components {
            log::debug!("{:?}", component);
            if component.updated {
                continue;
            }

            if let Some(link) = &component.link {
                // Extract file name from link
                let extracted_file_name = link.path().split('/').last().unwrap();
                let file_full_path = dest_path.join(Path::new(extracted_file_name));
                log::trace!(
                    "Start downloading component: {} from: {} to: {}.",
                    &component.component,
                    link,
                    file_full_path.display()
                );
                let token = format!("Bearer {}", component.token.as_ref().unwrap());
                // Checking that we are not asked to download two component into the same file
                if let Some(found_index) = paths
                    .iter()
                    .position(|found: &(ComponentType, PathBuf)| found.1 == file_full_path)
                {
                    let component = ComponentType::from_str(&component.component).unwrap();
                    log::error!(
                        "Components {:?} and {:?} have the same target path!",
                        component,
                        paths.get(found_index).expect("Get fail!").0
                    );
                    return Err(OTAError::fatal(
                        "Cannot download two components into the same file!".to_string(),
                    ));
                }
                paths.push((*component_type, file_full_path.clone()));
                stats_ptr.lock().unwrap().update_entry(
                    String::from(file_full_path.to_string_lossy()),
                    0,
                    0
                );
                stats_ptr.lock().unwrap().inc_download_count();

                futures.push(Box::pin(download(link.clone(), file_full_path.clone(), Some(component.checksum.clone()), token, stats_ptr.clone(),
                    |file: &str, progress: u64, total: u64, stats_ptr: Arc<Mutex<DownloadStats>>| {
                        stats_ptr.lock().unwrap().update_entry(String::from(file), progress, total);
                    },
                )));
            }
        }

        let (url, token) = self.coupling_rest_submitter.get_url_and_token();
        futures.push(Box::pin(report_eta(url, token, self.update_ota_status, stats_ptr.clone())));
        let result = join_all(futures).await;

        let mut successes = 0;
        for i in 0..paths.len() {
            if result[i] {
                log::info!("Download was successful for {:?}", paths[i].0.clone());
                components_paths.insert(paths[i].0, paths[i].1.clone());
                successes += 1;
            } else {
                log::error!("Download was not successful for {:?}", paths[i].0.clone());
            }
        }

        if successes < paths.len() {
            return Err(OTAError::fatal(format!("{} component(s) failed to download!", paths.len()-successes)));
        }

        manifest.update_components_paths(components_paths)
    }
}

#[cfg(test)]
mod tests {
    use crate::ota::download_manager::{download, DownloadManager};
    use crate::ota::manifest::{Component, Manifest};
    use crate::ota::ota_error::OTAErrorSeverity;
    use crate::ota::service_control_trait::MockSystemControlTrait;
    use crate::rest_request::DownloadStats;
    use crate::utils::log_utils::set_logging_for_tests;
    use futures_util::future::join_all;
    use std::cell::RefCell;
    use std::fs::{create_dir, remove_dir_all};
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::Instant;
    use url::Url;
    use crate::auth::auth_manager::fetch_license_manager;
    use crate::rest_comm::coupling_submit_trait::CouplingRestSubmitter;


    #[ignore]
    #[test]
    fn test_download_basic() {
        set_logging_for_tests(log::LevelFilter::Info);
        use crate::ota::service_control_trait::MockSystemControlTrait;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        let  mock = MockSystemControlTrait::new();
        let mut rest_mock = MockCouplingRestSubmitter::new();
        rest_mock
            .expect_post_checksums()
            .times(1)
            .returning(|_| Ok(std::string::String::from("[]")));
        let manifest =
            Manifest::new(true, Default::default(), Default::default(), Default::default(), read_function, |_, _| Ok(())).unwrap();

        let json = r#" {
            "component": "phantom_agent",
            "checksum": "test checksum",
            "link": "https://test_url",
            "token": "test token",
            "path": "./test_path"
        }"#;
        let component: Component = serde_json::from_str(&json).unwrap();
        let manifest = manifest.update_single_component(&component).unwrap();

        let mock = RefCell::new(mock);
        let update_ota_status = |_status, _message|{};
        let download_manager = DownloadManager::new(&mock, &rest_mock, Default::default(), update_ota_status);
        let _manifest = download_manager.unwrap().run(manifest).unwrap();
    }

    fn read_function(_: &Path) -> Result<String, String> {
        Ok(String::from(r#" { "core": "" } "#))
    }

    #[tokio::main]
    async fn download_several(num: u32, url: &str) {
        let test_dir = Path::new("./download_sample_test_dir");
        if test_dir.exists() {
            remove_dir_all(&test_dir).expect("Failed to remove old dir!");
        }
        create_dir(&test_dir).expect("Failed to create dir!");
        let stats_ptr = Arc::new(Mutex::new(DownloadStats::new()));

        println!("================ Start Downloads Block ================");
        let mut futures : Vec<std::pin::Pin<Box<dyn std::future::Future<Output=bool>>>> = vec![];

        for i in 0..num {
            let path = String::from(format!("test_download_file{}", i));
            let path = test_dir.join(Path::new(&path));
            let url = Url::parse(url).unwrap();
            let token = format!("");
            println!("Start downloading {} to: {}.", url, path.display());
            stats_ptr
                .lock()
                .unwrap()
                .update_entry(String::from(path.to_string_lossy()), 0, 0);
            futures.push(Box::pin(download(
                url,
                path,
                None,
                token,
                stats_ptr.clone(),
                |file: &str, progress: u64, size: u64, stats_ptr: Arc<Mutex<DownloadStats>>| {
                    stats_ptr
                        .lock()
                        .unwrap()
                        .update_entry(String::from(file), progress, size);
                },
            )));
        }
        let result = join_all(futures).await;
        for r in result {
            assert!(r, "Failed to download test file!");
        }
        println!("================ Close Downloads Block ================");
        remove_dir_all(&test_dir).expect("Failed to remove test dir!");
    }

    fn download_several_with_benchmark(num: u32, repeat: u32, url: &str) {
        let mut total: Duration = Duration::new(0, 0);

        for i in 0..repeat {
            let now = Instant::now();
            download_several(num, url);
            let elapsed = now.elapsed();
            println!("Round {} Elapsed: {:.2?}", i + 1, elapsed);
            total += elapsed;
        }
        println!(
            "Total {:.2?} for {} rounds, {:.2?} on the average",
            total,
            repeat,
            total / repeat
        );
    }

    // Test downloading 10 small files at the same time
    #[ignore]
    #[test]
    fn test_download_sample() {
        set_logging_for_tests(log::LevelFilter::Info);
        download_several(10, "http://speedtest.ftp.otenet.gr/files/test10Mb.db");
        // Tests cannot be async, so we are calling the async function from here
    }

    // Compare downloading many small files with downloading one big file of the same combined size
    #[ignore]
    #[test]
    fn test_download_benchmarks() {
        set_logging_for_tests(log::LevelFilter::Info);
        download_several_with_benchmark(10, 5, "http://speedtest.ftp.otenet.gr/files/test1Mb.db"); // Tests cannot be async, so we are calling the async function from here
        download_several_with_benchmark(1, 5, "http://speedtest.ftp.otenet.gr/files/test10Mb.db");
        // Tests cannot be async, so we are calling the async function from here
    }

    // Downloads 2 files from a test site in async manner, using DownloadManager and manifest
    #[ignore]
    #[test]
    fn test_download_with_manifest() {
        use crate::ota::service_control_trait::MockSystemControlTrait;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        set_logging_for_tests(log::LevelFilter::Info);
        let  mock = MockSystemControlTrait::new();
        let mut rest_mock = MockCouplingRestSubmitter::new();

        rest_mock.expect_put_ota_status().returning(|_,_,_| {
            futures::executor::block_on(async {
                let client = reqwest::Client::new();
                let _response = client
                    .get(Url::parse("https://google.com").unwrap()).send().await;
                log::info!("{:?}", _response);

            })
        });

        let remote_path = {
            #[cfg(unix)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/amd64/phantom-plugin_amd64-3.2.1.tar.gz").unwrap() }
            #[cfg(windows)] { Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Phantom.Oden.Integration/3.2.1/win/phantom_plugin_win.zip").unwrap() }
        };

        println!("Creating license manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let license_manager = fetch_license_manager().unwrap();
        let token = license_manager.get_token().unwrap();
        rest_mock.expect_get_url_and_token().times(1).returning(move || {
            (remote_path.clone(), token.clone())
        });
        rest_mock.expect_post_checksums().times(1).returning(|_| {
            Ok(std::string::String::from(
                r#"[
           {
              "token":"token",
              "_id":"6208c6e48cff329d56642878",
              "component":"oden_player",
              "link":"http://speedtest.ftp.otenet.gr/files/test10Mb.db",
              "checksum":"8c206a1a87599f532ce68675536f0b1546900d7",
              "path":"./"
           }
        ]"#,
            ))
        });

        let manifest = Manifest::new(
            true,
            Default::default(),
            Default::default(),
            Default::default(),
            read_download_multi_manifest,
            |_, _| Ok(()),
        )
        .unwrap();

        let mock = RefCell::new(mock);
        let test_dir = Path::new("./download_with_manifest_test_dir");
        let update_ota_status = |_status, _message|{};
        let download_manager =
            DownloadManager::new(&mock, &rest_mock, PathBuf::from(test_dir), update_ota_status).unwrap();
        // Setup
        if test_dir.exists() {
            remove_dir_all(&test_dir).expect("Failed to remove old dir!");
        }
        create_dir(&test_dir).expect("Failed to create dir!");
        // This will download two files from a global test site based on the manifest
        let _new_manifest = download_manager.run(manifest).unwrap();
        // Cleanup
        remove_dir_all(test_dir).expect("Failed to cleanup!");
    }

    fn read_download_multi_manifest(_: &Path) -> Result<String, String> {
        Ok(String::from(
            r#" {
            "core":"asdf",
            "oden_plugin":"asdf"
        }"#,
        ))
    }

    #[ignore]
    #[test]
    fn test_download_same_path() {
        use crate::ota::service_control_trait::MockSystemControlTrait;
        use crate::rest_comm::coupling_submit_trait::MockCouplingRestSubmitter;
        set_logging_for_tests(log::LevelFilter::Info);
        let  mock = MockSystemControlTrait::new();
        let mut rest_mock = MockCouplingRestSubmitter::new();
        rest_mock.expect_post_checksums().times(1).returning(|_| {
            Ok(std::string::String::from(
                r#"[
           {
              "token":"token",
              "_id":"6208c6e48cff329d56642878",
              "component":"core",
              "link":"http://speedtest.ftp.otenet.gr/files/test1Mb.db",
              "checksum":"asdf",
              "path":null,
              "installed":false,
           },
           {
              "token":"token",
              "_id":"6208c6e48cff329d56642878",
              "component":"oden_plugin",
              "version":"1.0.0",
              "link":"http://speedtest.ftp.otenet.gr/files/test1Mb.db",
              "checksum":"asdf",
              "path":null,
              "installed":false,
           }
        ]"#,
            ))
        });

        let manifest = Manifest::new(
            true,
            Default::default(),
            Default::default(),
            Default::default(),
            read_download_multi_manifest,
            |_, _| Ok(()),
        )
        .unwrap();

        let test_dir = Path::new("./download_same_path_dir");
        let mock = RefCell::new(mock);
        let update_ota_status = |_status, _message|{};
        let download_manager =
            DownloadManager::new(&mock, &rest_mock, PathBuf::from(test_dir), update_ota_status).unwrap();
        match download_manager.run(manifest) {
            Ok(_) => {
                panic!("Expected error!");
            }
            Err(e) => {
                assert_eq!(
                    e.message,
                    "Cannot download two components into the same file!"
                );
                assert_eq!(e.severity, OTAErrorSeverity::FatalError);
                assert_eq!(
                    format!("{}", e),
                    "FATAL: Cannot download two components into the same file!"
                );
            }
        }
        //assert!(res.is_err());
        //let e = res.unwrap_err();
        //assert_eq!(res.unwrap_err(), "Cannot download two components into the same file!");
    }

    fn read_manifest_function(_path: &Path) -> Result<String, String> {
        Ok(String::from(
            r#"{
            "test_server":
                {
                    "core":"core checksum",
                    "oden_plugin":"plugin checksum",
                    "oden_webview":"webview checksum"
                }
        }"#,
        ))
    }

    #[test]
    #[ignore]
    fn empty_manifest_cloud_reply() {
        use crate::rest_request::RestServer;
        use crate::ota::download_manager::DownloadManager;
        use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
        use crate::auth::license_manager::LicenseManager;
        use crate::auth::license_manager_trait::LicenseManagerTrait;
        use std::fs;
        set_logging_for_tests(log::LevelFilter::Info);
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

        let rest_comm = CouplingRestComm::new(&license_manager, RestServer::send_json);
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("empty_manifest_cloud_test_dir"));
        if test_dir.exists() { // Not DELETING the dir allows us to test partial download!
            remove_dir_all(&test_dir).expect("Failed to create dir!");
        }
        create_dir(&test_dir).expect("Failed to create dir!");

        let mock = MockSystemControlTrait::new();
        let sys_mock = RefCell::new(mock);
        let update_ota_status = |_status, _message|{};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_ota_status).unwrap();
        let reply = download_manager.post_empty_checksums().unwrap();
        log::info!("REPLY IS <<<{}>>>", reply);

        let write_function = |_: &Path, _: &str| Ok(());
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            Default::default(),
            read_manifest_function,
            write_function,
        ).unwrap();

        let manifest = manifest.update_with_json(&reply).unwrap();
        for (_component_type, component) in manifest.components {
            let url = match component.link {
                None => { "NONE".to_string() }
                Some(url) => { url.as_str().to_string() }
            };
            log::info!("{}: Version {}, link {}, checksum {}", component.component, component.version, url, component.checksum);
        }
        fs::remove_dir_all(&test_dir).expect("Failed to remove dir");
    }

    #[test]
    #[ignore]
    fn auth_empty_manifest() {
        use crate::rest_request::RestServer;
        use crate::ota::download_manager::DownloadManager;
        use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
        use std::fs;
        set_logging_for_tests(log::LevelFilter::Info);
        log::info!("Creating auth manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!

        let license_manager = match fetch_license_manager() {
            Ok(license_manager) => { license_manager }
            Err(err) => {
                log::info!("Couldn't load the license: {}", err);
                unreachable!();
            }
        };
        let rest_comm = CouplingRestComm::new(license_manager.deref(), RestServer::send_json);

        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("auth_empty_manifest_test_dir"));
        if test_dir.exists() { // Not DELETING the dir allows us to test partial download!
            fs::remove_dir_all(&test_dir).expect("Failed to create dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");

        let mock = MockSystemControlTrait::new();
        let sys_mock = RefCell::new(mock);
        let update_ota_status = |_status, _message|{};
        let download_manager =
            DownloadManager::new(&sys_mock, &rest_comm, test_dir.clone(), update_ota_status).unwrap();
        let reply = download_manager.post_empty_checksums().unwrap();
        log::info!("REPLY IS <<<{}>>>", reply);
        let write_function = |_: &Path, _: &str| Ok(());
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            Default::default(),
            read_manifest_function,
            write_function,
        ).unwrap();

        let manifest = manifest.update_with_json(&reply).unwrap();
        for (_component_type, component) in manifest.components {
            let url = match component.link {
                None => { "NONE".to_string() }
                Some(url) => { url.as_str().to_string() }
            };
            log::info!("{}: Version {}, link {}, checksum {}", component.component, component.version, url, component.checksum);
        }
        fs::remove_dir_all(&test_dir).expect("Failed to remove dir");
    }

    #[test]
    #[ignore]
    fn test_get_node_information() {
        use crate::rest_request::RestServer;
        use crate::rest_comm::coupling_rest_comm::CouplingRestComm;
        use std::fs;
        set_logging_for_tests(log::LevelFilter::Info);
        log::info!("Creating auth manager"); // Copy from "/root/snap/phau-core/common/license" to "./license" and give chmod!
        let license_manager = match fetch_license_manager() {
            Ok(license_manager) => { license_manager }
            Err(err) => {
                log::info!("Couldn't load the license: {}", err);
                unreachable!();
            }
        };
        let rest_comm = CouplingRestComm::new(license_manager.deref(), RestServer::send_json);

        let status = rest_comm.get_ota_status();
        println!("{:?}", status);

        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("test_get_node_information"));
        if test_dir.exists() { // Not DELETING the dir allows us to test partial download!
            fs::remove_dir_all(&test_dir).expect("Failed to create dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        fs::remove_dir_all(&test_dir).expect("Failed to remove dir");
    }
}
