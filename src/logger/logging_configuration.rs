use std::path::PathBuf;

extern crate chrono;
use thread_id;
use std::sync::Arc;
use spdlog::{ErrorHandler, LevelFilter};
use std::fmt::Write;
use std::time::Duration;
use chrono::offset::Utc;
use chrono::DateTime;
use spdlog::sink::{Sink, StdStream, StdStreamSink};
use crate::logger::phantom_file_logger::PhantomFileSink;

use spdlog::{
    formatter::{FmtExtraInfo, Formatter},
    prelude::*,
    Record, StringBuf,
};
use crate::config::{LoggingConfig, LogLevel};

pub fn logs_dir() -> PathBuf {
    PathBuf::from("../log")
}

#[derive(Clone, Default)]
pub struct PhantomFormatter;

impl Formatter for PhantomFormatter {
    fn format(&self, record: &Record, dest: &mut StringBuf) -> spdlog::Result<FmtExtraInfo> {
        let (source_file, module_path) = match record.source_location() {
            None => ("", ""),
            Some(location) => (location.file_name(), location.module_path()),
        };
        let datetime: DateTime<Utc> = record.time().into();
        write!(dest, "{}", datetime.format("%Y-%m-%dT%T%.3fZ"))
            .map_err(spdlog::Error::FormatRecord)?;
        let style_range_begin: usize = dest.len();
        write!(dest, " [{}] ", &record.level().as_str().to_ascii_uppercase()).map_err(spdlog::Error::FormatRecord)?;
        write!(dest, "[{}] ", thread_id::get()).map_err(spdlog::Error::FormatRecord)?;
        let style_range_end: usize = dest.len();
        writeln!(dest, "({}::{}) {}", module_path, source_file, record.payload(), ).map_err(spdlog::Error::FormatRecord)?;
        Ok(FmtExtraInfo::builder()
            .style_range(style_range_begin..style_range_end)
            .build())
    }

    fn clone_box(&self) -> Box<dyn Formatter> {
        Box::new(self.clone())
    }
}

pub fn log_configure() {
    spdlog::init_log_crate_proxy()
        .expect("users should only call `init_log_crate_proxy` function once");
    let config = LoggingConfig {
        default_level: LogLevel::Trace,
        loggers: vec![],
        retention: 14,
    };
    configure_logging(config)
}

pub fn configure_logging(config: LoggingConfig) {
    // Building a custom formatter.
    let new_formatter: Box<PhantomFormatter> = Box::default();

    let p_sink = Arc::new(PhantomSink::new(config));
    p_sink.set_formatter(new_formatter);
    let logger: Arc<Logger> = match Logger::builder().sink(p_sink).build() {
        Ok(logger) => Arc::new(logger),
        Err(error) => panic!("Error occurred while creating stdout logging sink: {error}"),
    };
    logger.set_flush_period(Some(Duration::from_secs(1)));

    let proxy: &'static spdlog::LogCrateProxy = spdlog::log_crate_proxy();
    log::set_max_level(log::LevelFilter::Trace);
    logger.set_level_filter(LevelFilter::All);
    proxy.swap_logger(Some(logger));

    log::trace!("LogLevel verification: 0");
    log::debug!("LogLevel verification: 1");
    log::info!("LogLevel verification: 2");
    log::warn!("LogLevel verification: 3");
    log::error!("LogLevel verification: 4");
}

struct PhantomSink {
    std_sink: StdStreamSink,
    file_sink: PhantomFileSink,
    config: LoggingConfig,
}

impl PhantomSink {
    fn new(config: LoggingConfig) -> PhantomSink {
        let std_sink = match StdStreamSink::builder()
            .std_stream(StdStream::Stdout)
            .build()
        {
            Ok(std_sink) => std_sink,
            Err(error) => panic!("Error occurred while creating stdout logging sink: {error}"),
        };
        //   let file_log_path = ;
        let file_sink = PhantomFileSink::new(config.retention);
        Self {
            std_sink,
            config,
            file_sink,
        }
    }
    fn should_log(&self, module: &str, source_path: &str, record_level: spdlog::Level) -> bool {
        let source_path_with_file = format!("{module}::{source_path}");
        match self
            .config
            .loggers
            .iter()
            .find(|logger| logger.component == module || logger.component == source_path_with_file)
        {
            None => PhantomSink::compare(record_level, self.config.default_level.clone()),
            Some(component_logger) => {
                PhantomSink::compare(record_level, component_logger.level.clone())
            }
        }
    }
    pub const fn spd_level_to_u16(spd_level: Level) -> u16 {
        match spd_level {
            Level::Critical => 5,
            Level::Error => 4,
            Level::Warn => 3,
            Level::Info => 2,
            Level::Debug => 1,
            Level::Trace => 0,
        }
    }
    // config_level is level that from phantom config
    // level is spd level
    pub fn compare(level: Level, config_level: LogLevel) -> bool {
        if config_level == LogLevel::None {
            return false;
        }
        let level_num: u16 = PhantomSink::spd_level_to_u16(level);
        let config_level_num: u16 = config_level as u16;
        config_level_num <= level_num
    }
}

impl Sink for PhantomSink {
    fn log(&self, record: &Record) -> spdlog::Result<()> {
        let (source_file, module_path) = match record.source_location() {
            None => ("", ""),
            Some(location) => (location.file_name(), location.module_path()),
        };
        if self.should_log(module_path, source_file, record.level()) {
            let std_result = self.std_sink.log(record);
            let file_result = self.file_sink.log(record);
            match (std_result, file_result) {
                (Ok(_), Ok(_)) => Ok(()),
                (Ok(_), Err(_)) => Ok(()),
                (Err(_), Ok(_)) => Ok(()),
                (Err(_std_error), Err(file_error)) => Err(file_error),
            }
        } else {
            Ok(())
        }
    }

    fn flush(&self) -> spdlog::Result<()> {
        let std_result = self.std_sink.flush();
        let file_result = self.file_sink.flush();
        match (std_result, file_result) {
            (Ok(_), Ok(_)) => Ok(()),
            (Ok(_), Err(_)) => Ok(()),
            (Err(_), Ok(_)) => Ok(()),
            (Err(std_error), Err(file_error)) => {
                let message = format!(
                    "Failed flushing all sinks std_error: {std_error} file_error: {file_error}"
                );
                println!("{message}");
                Err(file_error)
            }
        }
    }

    fn level_filter(&self) -> LevelFilter {
        self.std_sink.level_filter()
    }

    fn set_level_filter(&self, level_filter: LevelFilter) {
        self.std_sink.set_level_filter(level_filter);
        self.file_sink.set_level_filter(level_filter);
    }

    fn set_formatter(&self, formatter: Box<dyn Formatter>) {
        self.std_sink.set_formatter(formatter.clone_box());
        self.file_sink.set_formatter(formatter)
    }

    fn set_error_handler(&self, handler: Option<ErrorHandler>) {
        self.std_sink.set_error_handler(handler);
        self.file_sink.set_error_handler(handler);
    }
}
