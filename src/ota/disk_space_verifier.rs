use crate::{ota::manifest::Manifest, rest_request::RestServer};
use log;
use sysinfo::{DiskExt, RefreshKind, System, SystemExt};
use url::Url;

pub struct DiskSpaceVerifier {
    pub(crate) get_remote_file_size: fn(link: &Url, auth: Option<String>) -> Result<u64, String>,
    pub(crate) disk_space: u64,
}

impl DiskSpaceVerifier {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            get_remote_file_size: RestServer::get_file_size,
            disk_space: DiskSpaceVerifier::get_disk_space_field()?,
        })
    }
    fn get_disk_space_field() -> Result<u64, String> {
        let mut system_adapter =
            System::new_with_specifics(RefreshKind::new().with_disks().with_disks_list());
        system_adapter.refresh_all();

        match system_adapter.disks().iter().next() {
            None => Err("Failed getting disk space".to_string()),
            Some(disk_data) => Ok(disk_data.available_space()),
        }
    }

    fn bytes_to_megabytes(bytes: u64) -> u64 {
        bytes / 2_u64.pow(20)
    }
    pub fn verify(&self, manifest: &Manifest) -> Result<bool, String> {
        const MIN_DISK_SPACE: u64 = DiskSpaceVerifier::get_min_disk_space();
        let required_space = MIN_DISK_SPACE + self.get_components_total_size(manifest)?;
        let available_space = self.disk_space;
        log::info!(
            "Required space for downloading components is {}MB\n Available space is {}MB",
            Self::bytes_to_megabytes(required_space),
            Self::bytes_to_megabytes(available_space)
        );

        Ok(available_space > required_space)
    }

    const fn get_min_disk_space() -> u64 {
        // min space is 100mb;
        const MIN_DISK_SPACE_MB: u64 = 100;
        const BASE: u64 = 2;
        MIN_DISK_SPACE_MB * BASE.pow(20)
    }

    fn get_components_total_size(&self, manifest: &Manifest) -> Result<u64, String> {
        manifest
            .components
            .iter()
            .try_fold(0, |acc, (_, component)| {
                if component.updated {
                    return Ok(acc);
                }
                match (&component.link, &component.token) {
                    (Some(link), Some(token)) => {
                        let token = format!("Bearer {token}");
                        let file_size = (self.get_remote_file_size)(link, Some(token))?;
                        log::info!(
                            "The size of {} is {}MB",
                            component.component,
                            Self::bytes_to_megabytes(file_size)
                        );
                        Ok(acc + file_size)
                    }
                    (None, None) => {
                        Ok(acc) // Probably uninstall
                    }
                    _ => Err("Can't sum components size, no link or token".to_string()),
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::ota::disk_space_verifier::DiskSpaceVerifier;
    use crate::ota::manifest::Manifest;
    use std::path::{Path, PathBuf};

    fn read_function(_path: &Path) -> Result<String, String> {
        Ok(String::from(
            r#"
        {
            "core":"core checksum",
            "sim_gps_info":"asdf"
        }"#,
        ))
    }

    #[cfg(unix)]
    fn get_manifest() -> Manifest {
        let write_function = |_: &Path, str: &str| {
            println!("WRITE: {}", str);
            Ok(())
        };
        let manifest = Manifest::new(
            false,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            Default::default(),
            read_function,
            write_function,
        )
        .unwrap();
        let server_manifest_json = r#"
{
   "version":"1.27_JT4.6-rc.6",
   "missingComponents":[
      {
         "token":"eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
         "_id":"621c78f3fd12780012c795da",
         "component":"core",
         "version":"0.1.2",
         "link":"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
         "checksum":"new checksum",
         "arch":"AMD64"
      },
      {
         "token":"eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
         "_id":"621c78f3fd12780012c795da",
         "component":"sim_gps_info",
         "version":"0.1.2",
         "link":"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
         "checksum":"new checksum",
         "arch":"AMD64"
      }
   ]
}
        "#;
        manifest.update_with_json(server_manifest_json).unwrap()
    }
    #[cfg(windows)]
    fn get_manifest() -> Manifest {
        let write_function = |_: &Path, str: &str| {
            println!("WRITE: {}", str);
            Ok(())
        };
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            Default::default(),
            Default::default(),
            read_function,
            write_function,
        )
        .unwrap();
        let server_manifest_json = r#"
        [
  {
    "token": "eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
    "_id": "621c78f3fd12780012c795da",
    "component": "core",
    "version": "0.1.2",
    "link": "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
    "checksum": "new checksum",
    "arch": "WIN"
  },
  {
    "token": "eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
    "_id": "621c78f3fd12780012c795da",
    "component": "oden_player",
    "version": "0.6.0",
    "link": "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
    "checksum": "new checksum",
    "arch": "WIN"
  }

]
        "#;
        manifest.update_with_json(server_manifest_json).unwrap()
    }

    #[test]
    fn basic_failure_test() {
        let disk_space_verifier = DiskSpaceVerifier {
            get_remote_file_size: |_, _| Ok(20),
            disk_space: 20,
        };
        let manifest = get_manifest();
        assert_eq!(disk_space_verifier.verify(&manifest), Ok(false))
    }

    #[test]
    fn basic_success_test() {
        let base: u64 = 2;
        let min_space = 100 * base.pow(20);
        let disk_space = min_space + 40 + 1;
        let disk_space_verifier = DiskSpaceVerifier {
            get_remote_file_size: |_, _| Ok(20),
            disk_space,
        };
        let manifest = get_manifest();
        assert_eq!(disk_space_verifier.verify(&manifest), Ok(true))
    }

    #[test]
    fn size_test() {
        let disk_space_verifier = DiskSpaceVerifier {
            get_remote_file_size: |_, _| Ok(20),
            disk_space: 10,
        };
        let manifest = get_manifest();
        assert_eq!(
            disk_space_verifier.get_components_total_size(&manifest),
            Ok(40)
        )
    }
    #[test]
    #[ignore]
    fn disk_space_verifier_real() {
        let disk_space_verifier = DiskSpaceVerifier::new().unwrap();
        let manifest = get_manifest();
        assert_eq!(
            disk_space_verifier.get_components_total_size(&manifest),
            Ok(40)
        )
    }

    #[test]
    fn remote_error_test() {
        let disk_space_verifier = DiskSpaceVerifier {
            get_remote_file_size: |_, _| Err("File not found".to_string()),
            disk_space: 10,
        };
        let manifest = get_manifest();
        assert_eq!(
            disk_space_verifier.get_components_total_size(&manifest),
            Err("File not found".to_string())
        )
    }
}
