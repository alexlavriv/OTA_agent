use super::bash_exec::BashExec;

// NOTE: In Linux we want to ensure that the timedatectl service is ON
#[cfg(unix)]
pub fn check_ntp_service_status() -> bool {
    match BashExec::exec_arg_log("timedatectl", &[], false) {
        Ok(stdout) => match stdout.contains("NTP service: active") {
            true => {
                log::trace!("NTP service is up and running");
                true
            }
            false => {
                log::warn!("NTP service is inactive, will attempt to reinstate");
                false
            }
        },
        Err(stderr) => {
            log::error!("Could not run timedatectl: {stderr}");
            log::error!("please contact system administrator");
            false
        }
    }
}

#[cfg(unix)]
pub fn set_ntp_service_status() -> bool {
    // Stop any other services here
    BashExec::exec_arg_log("service ntp stop", &[], false).ok();
    match BashExec::exec_arg_log("timedatectl set-ntp true", &[], false) {
        Ok(_) => {
            log::trace!("timedatectl NTP service active");
            true
        }
        Err(stderr) => {
            log::error!("Could not set timedatectl service to primary: {stderr}");
            false
        }
    }
}

// NOTE: On Windows we want to *disable* the system service
#[cfg(windows)]
pub fn check_ntp_service_status() -> bool {
    match BashExec::exec_cmd("w32tm /query /status") {
        Ok(_) => {
            log::trace!("Windows Time Service is up and running");
            false
        }
        Err(stderr) => {
            log::error!("Windows Time Service is not running: {stderr}");
            true
        }
    }
}

#[cfg(windows)]
pub fn set_ntp_service_status() -> bool {
    BashExec::exec_cmd_write_info_log("w32tm /unregister", false).ok();
    match BashExec::exec_cmd_write_info_log("net stop w32time", false) {
        Ok(_) => {
            log::trace!("Disabled Windows Time Service");
            true
        }
        Err(stderr) => {
            log::error!("Could not disable Windows Time Service: {stderr}");
            false
        }
    }
}

#[cfg(windows)]
pub mod windows_ntp_service {
    use crate::utils::verify_ntp::{check_ntp_service_status, set_ntp_service_status};
    use chrono::prelude::*;
    use ntp::formats::timestamp::{TimestampFormat, EPOCH_DELTA};
    use std::{
        sync::{atomic::{AtomicBool, Ordering}, Arc},
        thread::{sleep, JoinHandle},
        time::{Duration, SystemTime},
        env,
    };
    use windows::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::SetLocalTime};

    struct NTPResponse {
        pub server_address: String,
        pub server_rtt: Duration,
        pub server_clock: Duration,
    }

    fn timestamp_to_duration(timestamp: &TimestampFormat) -> Duration {
        const NTP_SCALE: f64 = 4294967295.0_f64;
        Duration::new(
            (timestamp.sec as i64 - EPOCH_DELTA) as u64,
            (timestamp.frac as f64 / NTP_SCALE * 1e9) as u32,
        )
    }

    pub struct NTPService {
        ntp_thread: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
    }

    impl NTPService {
        pub fn new(interval_secs: u32, servers: Vec<String>) -> Self {
            let kill_switch = Arc::new(AtomicBool::new(false));
            Self {
                ntp_thread: Some((
                    kill_switch.clone(),
                    std::thread::Builder::new()
                        .name("NTP Thread".to_string())
                        .spawn(move || {
                            log::info!("Started NTP service thread");
                            while !kill_switch.load(Ordering::Acquire) {
                                // Verify the windows service is off
                                if !check_ntp_service_status() {
                                    set_ntp_service_status();
                                }

                                // Run our sync service
                                if let Err(e) = Self::run_ntp_service(&servers) {
                                    log::error!("Failed syncing system time: {e}");
                                }

                                // Sleep the designated time
                                sleep(Duration::from_secs(interval_secs as u64));
                            }
                        })
                        .expect("Could not launch NTP thread"),
                )),
            }
        }

        fn request_ntp_from_server(server: &str) -> Result<NTPResponse, String> {
            // So we have an accurate time of when the request was made
            let time_now = SystemTime::now();
            let response = ntp::request(server)
                .map_err(|e| format!("{server} - Could not perform NTP request: {e}"))?;

            // Delta between when the server received the request, and when it sent a response back
            let server_delay = timestamp_to_duration(&response.transmit_time).checked_sub(
                timestamp_to_duration(&response.recv_time)).ok_or_else(|| "Overflow delay".to_string())?;

            // Round trip time is the entire request time, minus the delta inside the server
            let rtt = time_now
                .elapsed()
                .map_err(|e| format!("Could not count time since request: {e}"))?
                .checked_sub(server_delay).ok_or_else(|| "Overflow rtt".to_string())?;

            // We offset the server time with the estimated one-way request time
            let estimated_server_utc_time =
                timestamp_to_duration(&response.transmit_time) + (rtt / 2);
            let local_utc_offset_in_seconds = Local::now().offset().local_minus_utc();
            let estimated_local_time = match local_utc_offset_in_seconds >= 0 {
                true => {
                    estimated_server_utc_time
                        + Duration::from_secs(local_utc_offset_in_seconds as u64)
                }
                false => {
                    estimated_server_utc_time.checked_sub(Duration::from_secs(local_utc_offset_in_seconds.unsigned_abs() as u64))
                        .ok_or_else(|| "Overflow local".to_string())?
                }
            };

            // Add time zone offset
            Ok(NTPResponse {
                server_address: server.to_string(),
                server_rtt: rtt,
                server_clock: estimated_local_time,
            })
        }

        pub fn run_ntp_service_default() -> Result<(), String> {
            let ntp_servers = env::var("NTP_SERVERS")
                .unwrap_or_else(|_| "0.pool.ntp.org:123,1.pool.ntp.org:123,2.pool.ntp.org:123,3.pool.ntp.org:123".to_string())
                .split(',')
                .map(|element| element.to_string())
                .collect::<Vec<String>>();

            Self::run_ntp_service(&ntp_servers)
        }

        pub fn run_ntp_service(servers: &[String]) -> Result<(), String> {
            let mut rtt_max = 80;
            const ATTEMPTS: i32 = 10;
            for attempt_number in 0..ATTEMPTS {
                let mut received_durations = servers
                    .iter()
                    // Get an NTP response for each server, only keep the ones that succeeded
                    .filter_map(|server| {
                        if let Ok(ntp_response) = Self::request_ntp_from_server(server) {
                            return Some(ntp_response);
                        }
                        None
                    })
                    // We keep a vector and not a hashmap because vectors can be sorted
                    .collect::<Vec<NTPResponse>>();

                // If no results match the predicate, try again in a bit
                if received_durations.is_empty() {
                    log::warn!("Could not get any NTP request with less than 80ms rtt, attempt {attempt_number} out of {ATTEMPTS}", );
                    continue;
                }

                received_durations.sort_by(|ntp_response_a, ntp_response_b| {
                    ntp_response_a.server_rtt.cmp(&ntp_response_b.server_rtt)
                });

                // Get the first one, which in the sorted vector is the one with the smallest rtt
                if let Some(ntp_response) = received_durations.first() {
                    if let Ok(local_time) = time::OffsetDateTime::from_unix_timestamp_nanos(
                        ntp_response.server_clock.as_nanos() as i128,
                    ) {
                        let sys_time = SYSTEMTIME {
                            wYear: local_time.year() as u16,
                            wMonth: local_time.month() as u16,
                            wDayOfWeek: local_time.weekday() as u16,
                            wDay: local_time.day() as u16,
                            wHour: local_time.hour() as u16,
                            wMinute: local_time.minute() as u16,
                            wSecond: local_time.second() as u16,
                            wMilliseconds: local_time.millisecond(),
                        };

                        unsafe {
                            if !SetLocalTime(&sys_time as *const SYSTEMTIME).as_bool() {
                                return Err(
                                    "Failed setting local time, Ensure we have administrator privileges"
                                        .to_string(),
                                );
                            }
                        }

                        log::info!(
                            "Synced local time to server '{}': {:0>2}/{:0>2}/{} {:0>2}:{:0>2}:{:0>2}.{:0>3}, Server round-trip-time: {}ms",
                            ntp_response.server_address,
                            local_time.day(),
                            local_time.month() as u8,
                            local_time.year(),
                            local_time.hour(),
                            local_time.minute(),
                            local_time.second(),
                            local_time.millisecond(),
                            ntp_response.server_rtt.as_millis()
                        );
                        return Ok(());
                    }
                }

                log::warn!(
                    "No request matched the {rtt_max} rtt threshold, trying again in 2 seconds"
                );
                sleep(Duration::from_secs(1));
                rtt_max += 10;
            }

            Err(format!(
                "All requests timed out, max accepted rtt: {rtt_max}"
            ))
        }
    }

    impl Drop for NTPService {
        fn drop(&mut self) {
            if let Some((kill_switch, join_handle)) = self.ntp_thread.take() {
                kill_switch.store(true, Ordering::Release);
                join_handle.join().ok();
            }
        }
    }
}
