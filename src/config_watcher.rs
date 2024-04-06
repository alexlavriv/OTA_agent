use crate::config::{Config, LoggingConfig};
use config::FileFormat;
use log::{error, info};
use notify::event::EventKind;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub struct ConfigWatcher {
    config_path: PathBuf,
    logging_update_event: fn(config: LoggingConfig),
}

impl ConfigWatcher {
    pub fn new(config_path: PathBuf, logging_update_event: fn(config: LoggingConfig)) -> ConfigWatcher {
        Self {
            config_path,
            logging_update_event,
        }
    }
    pub fn watch(&self) {
        let config_path = self.config_path.clone();
        let path = self.config_path.parent().unwrap().to_path_buf();
        info!("Watching after {}", path.to_string_lossy());
        let update_logging_config = self.logging_update_event;
        thread::Builder::new()
            .name("Config Watcher".to_string())
            .spawn(move || {
                let (tx, rx) = std::sync::mpsc::channel();

                let mut watcher = match RecommendedWatcher::new(
                    tx,
                    notify::Config::default().with_poll_interval(Duration::from_secs(2)),
                ) {
                    Err(e) => {
                        error!("Error occurred while creating watch {e}");
                        panic!()
                    }
                    Ok(watcher) => watcher,
                };

                match watcher.watch(path.as_ref(), RecursiveMode::NonRecursive) {
                    Err(e) => {
                        error!("Error occurred while watching {e}");
                        panic!()
                    }
                    Ok(watcher) => watcher,
                }

                for res in &rx {
                    log::info!("Got file event {:?}", res);
                    match res {
                        Ok(event) => {
                            if let EventKind::Modify(_) = event.kind {
                                for path in event.paths {
                                    if path == config_path {
                                        ConfigWatcher::update_logging_config(&config_path, update_logging_config);
                                    }
                                }
                            }
                        }
                        Err(e) => error!("watch error: {:?}", e),
                    }
                }

            })
            .expect("Could not spawn Config Watcher");
    }
    pub fn update_logging_config(path: &Path, logging_update_event: fn(config: LoggingConfig)) {
        if path.exists() == false {
            info!("Did not update logging configuration, file {} does not exist", path.to_string_lossy());
            return;
        }
        info!("Updating logging config");
        let logging_config = Config::settings_to_config(
            &config::Config::builder()
                .add_source(config::File::new(&path.to_string_lossy(), FileFormat::Json))
                .build()
                .unwrap(),
        );
        (logging_update_event)(logging_config)
    }
}
