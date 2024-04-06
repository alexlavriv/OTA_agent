use crate::BashExec;

// Allows seeing the log::info, warn, debug etc outputs in tests
// Level is the max level such as log::LevelFilter::info
pub fn set_logging_for_tests(level: log::LevelFilter) {
    static MY_LOGGER: MyLogger = MyLogger;
    struct MyLogger;
    impl log::Log for MyLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                println!("{} - {}", record.level(), record.args());
            }
        }
        fn flush(&self) {}
    }
    log::set_logger(&MY_LOGGER).unwrap();
    log::set_max_level(level);
}

pub fn size_as_string(size: u64) -> String {
    let size_letters = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut index = 0;
    let mut size = size as f64;
    while size > 1024.0 {
        index += 1;
        size /= 1024.0;
        if index >= size_letters.len() {
            return "Unknown".to_string();
        }
    }
    format!("{:.1} {}", size, size_letters[index])
}

pub fn seconds_as_string(seconds: f64) -> String {
    let minutes = (seconds / 60.0) as u64;
    if minutes > 0 {
        format!("{minutes} minutes")
    } else {
        format!("{seconds:.1} seconds")
    }
}

pub fn hostname() -> String {
    match BashExec::exec("hostname") {
        Ok(hostname) => { hostname.trim().to_string() }
        Err(e) => {
            log::warn!("Attempted to extract hostname but failed ({})", e);
            "DEFAULT".to_string()
        }
    }
}
