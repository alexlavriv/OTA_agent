

#![windows_subsystem = "windows"]

fn main() {
    phantom_agent_snap::main();
}

mod phantom_agent_snap {
    extern crate reqwest;
    extern crate url;
    extern crate single_instance;

    use phantom_agent::{config::Config, config_watcher, config_watcher::ConfigWatcher, create_ota_service, logger::logging_configuration, ota::manifest::HASH_MANIFEST_PATH, utils::{
        color::Coloralex,
        file_utils::get_path,
    }};
    #[cfg(windows)]
    use phantom_agent::ui::system_tray;
    use phantom_agent::auth::license_manager::LicenseManager;
    use std::{
        env,
        path::{Path, PathBuf},
        str::FromStr,
        time::Duration,
    };
    use phantom_agent::utils::verify_ntp;
    use single_instance::SingleInstance;

    fn get_hash_manifest_path(user_common_path: &Path) -> PathBuf {
        //get_path(user_common_path, "manifest/manifest.json")
        get_path(user_common_path, Path::new(HASH_MANIFEST_PATH))
    }

    #[cfg(not(windows))]
    fn get_common_path() -> PathBuf {
        if let Ok(user_common) = env::var("SNAP_USER_COMMON") {
            PathBuf::from(user_common)
        } else {
            log::error!("Did not get SNAP_USER_COMMON");
            panic!("Did not get SNAP_USER_COMMON")
        }
    }
    #[cfg(windows)]
    fn get_common_path() -> PathBuf {
        PathBuf::from("C:\\Program Files\\phantom_agent\\bin")
    }

    pub fn main() {
        let args: Vec<String> = env::args().collect();
        let version = env!("CARGO_PKG_VERSION");
        handle_args(&args, version);
        println!("{}", "Starting phantom agent...".green(true));
         
        let arg0 = &args[0];
        if arg0.is_empty() {
            println!("arg0 is empty");
        } else {
            println!("arg0 is {arg0}");

            match Path::new(arg0).parent(){
            Some(current_dir) => { 
                println!("Setting the current dir to: {}", current_dir.to_string_lossy());
                match env::set_current_dir(current_dir){
                    Ok(_) => println!("The current directory is set to: {}", current_dir.to_string_lossy()),
                    Err(e) => println!("Failed setting the current directory to: {} with error {e}", current_dir.to_string_lossy()),
                }
            }
            None => println!("Could not get parent directory of {arg0}")
            }
        }

        match env::current_dir() {
            Ok(current_dir) => println!("The current dir is: {}", current_dir.to_string_lossy()),
            Err(err) =>  println!("Got error while getting the current dirctory: {err}"),
        }


        let user_common_path = get_common_path();
        let user_common_path = user_common_path.as_path();
        logging_configuration::log_configure();
        let hash_manifest_path = get_hash_manifest_path(user_common_path);

        let instance = SingleInstance::new("Phantom Agent Main").unwrap();
        if !instance.is_single() {
            let error_message = "Phantom agent cannot run, another instance of phantom agent is already running!".to_string();
            log::error!("{}", error_message.red(true));
            return;
        }

        log::info!(
            "Starting Phantom Agent...\nVersion: {}\ngit hash: {}\ngit branch: {}",
            version.green(true),
            option_env!("CI_COMMIT_SHORT_SHA").unwrap_or("UNKNOWN").green(true),
            option_env!("CI_COMMIT_REF_NAME").unwrap_or("UNKNOWN").green(true)
        );
        LicenseManager::move_license();

        let config = Config::new();
        let config_path = get_path(user_common_path, Path::new("config"));
        let config_watcher = ConfigWatcher::new(config_path.clone(), logging_configuration::configure_logging);
        config_watcher.watch();
config_watcher::ConfigWatcher::update_logging_config(&config_path, logging_configuration::configure_logging);
        logging_configuration::configure_logging(config.logging.clone());

        std::thread::Builder::new()
            .name("NTP Thread".to_string())
            .spawn(move || {
                log::info!("Started NTP service thread");

                let interval_secs = u32::from_str(
                    env::var("NTP_INTERVAL")
                        .unwrap_or_else(|_| "300".to_string())
                        .as_str(),
                ).unwrap_or(300);

                loop {
                    // Verify the ntp service is off
                    if !verify_ntp::check_ntp_service_status() {
                        verify_ntp::set_ntp_service_status();
                    }

                    #[cfg(windows)]
                    // Run our sync service
                    if let Err(e) = verify_ntp::windows_ntp_service::NTPService::run_ntp_service_default() {
                        log::error!("Failed syncing system time: {e}");
                    }

                    // Sleep the designated time
                    std::thread::sleep(Duration::from_secs(interval_secs as u64));
                }
            }).expect("Could not launch NTP thread");

        #[cfg(windows)]
        let _tray = system_tray::SystemTray::load();
        #[cfg(windows)]
        phantom_agent::ui::progress_ui::ProgressUI::init();
        #[cfg(windows)]
        phantom_agent::resources::resource_manager::ResourceManager::init();
        if config.enable_ota {
            let dest_path = get_common_path();
            #[cfg(not(windows))]
            let dest_path =
                    dest_path.join(Path::new("/root/snap/phantom-agent/common/download"));


            let ota_manager = create_ota_service(
                dest_path,
                hash_manifest_path,
                config,
            );

            ota_manager.run();
        }
    }



    // If received args, means we are not service -> exit
    fn handle_args(args: &Vec<String>, version: &str) {
        if args.len() == 2 && (&args[1][..] == "--version" || &args[1][..] == "-v") {
            println!("{version}");
            std::process::exit(0)
        }
    }



}
