extern crate chrono;

use crate::logger::logging_configuration::logs_dir;
use crate::logger::logging_configuration::PhantomFormatter;
use crate::utils::file_utils::clear_folder;
use spdlog::sink::Sink;
use spdlog::Record;
use spdlog::{formatter::Formatter, ErrorHandler, LevelFilter};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use crate::utils::file_creation_date_util::get_file_creation_date;
use crate::create_dir_if_not_exists;

pub struct PhantomFileSink {
    file_ref: Arc<Mutex<std::fs::File>>,
    formatter: Box<dyn Formatter>,
}

impl PhantomFileSink {
    pub fn new(retention_days: u32) -> Self {
        let logs_dir = logs_dir();
        create_dir_if_not_exists(&logs_dir);
        let file_log_path = logs_dir.join("phantom_agent.log");
        PhantomFileSink::apply_retention_policy(&file_log_path, retention_days);
        let file_ref = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_log_path);
        match file_ref {
            Ok(file_ref) => {
                let file_ref = Arc::new(Mutex::new(file_ref));

                Self {
                    file_ref,
                    formatter: Box::new(PhantomFormatter),
                }
            }
            Err(e) => {
                PhantomFileSink::internal_log(&format!("Failed openning phantom_agent.log file {}", e));
                panic!("Error occurred {}", e)
            }
        }
    }
    fn internal_log(message: &str) {
        use std::io::Write;
        let message = format!("{message}\n");
        let file_log_path = logs_dir().join("/internal.log");
        let mut file_ref = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_log_path)
            .expect("Unable to open file");
        file_ref
            .write_all(message.as_bytes())
            .expect("write failed");
    }

    fn epoch_to_date(secs: u64) -> String {
        use chrono::prelude::DateTime;
        use chrono::Utc;
        use std::time::{Duration, UNIX_EPOCH};
        // Creates a new SystemTime from the specified number of whole seconds
        let d = UNIX_EPOCH + Duration::from_secs(secs);
        // Create DateTime from SystemTime
        let datetime = DateTime::<Utc>::from(d);
        // Formats the combined date and time with the specified format string.
        datetime.format("%Y-%m-%d %H:%M:%S.%f").to_string()
    }

    fn apply_retention_policy(log_path: &Path, retention_days: u32) {
        if !log_path.exists() {
            return;
        }
        let retention_seconds = (retention_days as u64) * 24 * 60 * 60;

        let now_systime = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        let now_from_epoch = now_systime.unwrap().as_secs();
        let retention_limit = now_from_epoch - retention_seconds;
        if let Ok(file_create_time) = get_file_creation_date(log_path) {
            if file_create_time < retention_limit {
                PhantomFileSink::internal_log("Retention applied!");
                PhantomFileSink::internal_log(&format!(
                    "now_from_epoch: {}",
                    PhantomFileSink::epoch_to_date(now_from_epoch)
                ));
                PhantomFileSink::internal_log(&format!(
                    "file_create_time: {}",
                    PhantomFileSink::epoch_to_date(file_create_time)
                ));
                PhantomFileSink::internal_log(&format!(
                    "retention_limit: {}",
                    PhantomFileSink::epoch_to_date(retention_limit)
                ));
                clear_folder(log_path.parent().unwrap()).expect("Failed removing log file");
            }
        }
    }
    fn write(&self, message: &str) {
        use std::io::Write;
        self.file_ref
            .lock()
            .unwrap()
            .write_all(message.as_bytes())
            .expect("write failed");
    }
    fn flush_file(&self) {
        self.file_ref
            .lock()
            .unwrap()
            .sync_all()
            .expect("Failed flush");
    }
}

impl Sink for PhantomFileSink {
    fn log(&self, record: &Record) -> spdlog::Result<()> {
        let mut dest = String::new();
        self.formatter
            .format(record, &mut dest)
            .expect("failed forrmating");
        self.write(&dest);
        Ok(())
    }

    fn flush(&self) -> spdlog::Result<()> {
        self.flush_file();
        Ok(())
    }

    fn level_filter(&self) -> LevelFilter {
        LevelFilter::All
    }

    fn set_level_filter(&self, _level_filter: LevelFilter) {}

    fn set_formatter(&self, _formatter: Box<dyn Formatter>) {}

    fn set_error_handler(&self, _handler: Option<ErrorHandler>) {}
}

#[cfg(test)]
mod tests {
    use crate::logger::phantom_file_logger::PhantomFileSink;
    use std::fs;
    use std::path::Path;
    use std::{thread, time};

    #[test]#[ignore]
    fn test_apply_retention_policy() {
        let test_file = Path::new("sha1_checksum_from_file_test");
        if test_file.exists() {
            fs::remove_file(test_file).expect("Failed to remove old file");
        }

        let data = "The quick brown fox jumps over the lazy dog";
        fs::write(test_file, data).expect("Couldn't write into file!");
        let two_seconds = time::Duration::from_secs(2);

        thread::sleep(two_seconds);
        if test_file.exists() {
            PhantomFileSink::apply_retention_policy(test_file, 0);
            if test_file.exists() {
                unreachable!();
            }
        }
    }

    #[test]#[ignore]
    fn test_apply_retention_policy2() {
        let test_file = Path::new("sha1_checksum_from_file_test");
        if test_file.exists() {
            fs::remove_file(test_file).expect("Failed to remove old file");
        }

        let data = "The quick brown fox jumps over the lazy dog";
        fs::write(test_file, data).expect("Couldn't write into file!");
        if test_file.exists() {
            PhantomFileSink::apply_retention_policy(test_file, 1);
            if test_file.exists() {
                fs::remove_file(test_file).expect("Failed to remove old file");
            } else {
                unreachable!();
            }
        }
    }
}
