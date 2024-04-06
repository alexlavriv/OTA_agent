use std::path::Path;

pub fn get_file_creation_date(path: &Path) -> Result<u64, String> {
    file_create_util::get_file_created_time(path)
}

#[cfg(windows)]
mod file_create_util {
    use std::fs;
    use std::path::Path;
    use std::time::SystemTime;
    use crate::utils::file_utils::generate_file_error;
    pub fn get_file_created_time(path: &Path) -> Result<u64, String> {
        if path.exists() {
            match fs::metadata(path) {
                Ok(metadata) => match metadata.created() {
                    Ok(date) => Ok(date
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()),
                    Err(error) => Err(format!(
                        "Error occurred while getting created time: {error}"
                    )),
                },
                Err(error) => Err(format!("Error occurred while getting metadata: {error}")),
            }
        } else {
            Err(generate_file_error(path, "does not exist"))
        }
    }
}
#[cfg(not(windows))]
mod file_create_util {
    use std::fs;
    use std::path::Path;
    use chrono::{DateTime, NaiveDateTime};
    use std::time::SystemTime;
    use std::os::unix::fs::MetadataExt;
    use chrono::Utc;
    use crate::BashExec;


    pub fn get_file_created_time(path: &Path) -> Result<u64, String> {
        if !path.exists() {
            return Err(format!("File does not exists {}", path.to_string_lossy()));
        }
        let metadata = fs::metadata(path).unwrap();
        let file_inode = metadata.ino();
        let device = get_file_device(path)?;
        let output = BashExec::exec_arg(&format!("debugfs {device}"), &["-R", &format!("stat <{file_inode}>")])?;
        get_crtime_parse_debugfs(&output)
    }

    fn get_file_device(path: &Path) -> Result<String, String> {
        if !path.exists() {
            return Err(format!("File does not exists {}", path.to_string_lossy()));
        }
        //stat -c %i "${target}"
        let device = BashExec::exec(&format!("df  --output=source {}", path.to_string_lossy()))?;
        let device = device.lines().last().unwrap();
        Ok(device.to_string())
    }

    fn get_crtime_parse_debugfs(input: &str) -> Result<u64, String> {
        let result = match input.lines().find(|line| line.trim().starts_with("crtime")) {
            Some(result) => result,
            None => return Err("crtime not found".to_string())
        };
        let result = match result.split("--").last() {
            Some(result) => result.trim(),
            None => return Err(" -- delimeter not found".to_string())
        };

        let date_only = match NaiveDateTime::parse_from_str(result, "%a %b %d %H:%M:%S %Y") {
            Ok(date_only) => date_only,
            Err(error) => return Err(format!("Failed parsing {:?} error: {}", result, error))
        };
        let datetime_again: DateTime<Utc> = DateTime::from_naive_utc_and_offset(date_only, Utc);
        let sys_time: SystemTime = datetime_again.into();

        match sys_time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(time) => Ok(time.as_secs()),
            Err(error) => Err(format!("Error calculation epoch {}", error))
        }
    }
}