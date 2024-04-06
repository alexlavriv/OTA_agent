

#[cfg(unix)]
#[allow(dead_code)]
pub fn hardcoded_manifest_size(operator: bool) -> usize {
    if operator { 7 } else { 11 }
}

#[cfg(unix)]
pub fn get_hardcoded_manifest(operator: bool) -> String {
    if operator {
        // OPERATOR manifest
        String::from(
            r#"
{
  "core":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"core",
    "version":"1.0.0",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "phantom_agent": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_agent",
    "version":"1.0.0",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "phantom_launcher": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_launcher",
    "version":"1.0.0",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "oden_player": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_player",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"deb",
    "processes":["Phantom Client"]
  },
  "oden_plugin": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_plugin",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"/opt/phantom-client",
    "processes":["Phantom Client"]
  },
  "oden_webview": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_webview",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"/opt/phantom-client",
    "processes":["Phantom Client"]
  },
  "log2jira": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"log2jira",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap",
    "processes":[]
  }
}
            "#,
        )
    } else {
        // VEHICLE manifest
        String::from(
            r#"
{
  "core":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"core",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "sim_gps_info":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"sim_gps_info",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "phantom_agent": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_agent",
    "version":"1.0.0",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "translator": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"translator",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "vapp": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"vapp",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "stream_manager": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"stream_manager",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "sdk_demo": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"sdk_demo",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap",
    "processes":["phantom-sdk-demo"]
  },
  "oden_streamer": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_streamer",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"deb",
    "processes":["phantom-streamer","phantom-streame"]
  },
  "oden_plugin": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_plugin",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"/opt/phantom-streamer",
    "processes":["phantom-streamer","phantom-streame"]
  },
  "autonomy_client":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"autonomy_client",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "log2jira": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"log2jira",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap",
    "processes":[]
  }
}
            "#,
        )
    }
}

#[cfg(windows)]
#[allow(dead_code)]
pub fn hardcoded_manifest_size(operator: bool) -> usize {
    if operator { 7 } else { 11 }
}

#[cfg(windows)]
pub fn get_hardcoded_manifest(operator: bool) -> String {
    if operator {
        // OPERATOR manifest
        String::from(
            r#"
{
  "core":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"core",
    "version":"1.0.0",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "phantom_agent": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_agent",
    "version":"1.0.0",
    "path":"C:/Program Files/phantom_agent/bin/download/phantom_agent.exe",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "phantom_launcher": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_launcher",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"msi",
    "processes":["PhantomLauncher.exe"]
  },
  "oden_player": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_player",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"msi",
    "processes":["PhantomClient.exe", "cef_child_process.exe"]
  },
  "oden_plugin": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_plugin",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"C:/Program Files/Phantom Client",
    "processes":["PhantomClient.exe", "cef_child_process.exe"]
  },
  "oden_webview": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_webview",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"C:/Program Files/Phantom Client",
    "processes":["PhantomClient.exe", "cef_child_process.exe"]
  },
  "log2jira": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"log2jira",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"msi",
    "processes":[]
  }
}
            "#,
        )
    } else {
        String::from(
            r#"
{
  "core":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"core",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
   "phantom_agent": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"phantom_agent",
    "version":"1.0.0",
    "path":"C:/Program Files/phantom_agent/bin/download/windows_service.exe",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "sim_gps_info":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"sim_gps_info",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "translator": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"translator",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "vapp": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"vapp",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "stream_manager": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"stream_manager",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "sdk_demo": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"sdk_demo",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap",
    "processes":["phantom-sdk-demo"]
  },
  "oden_streamer": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_streamer",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"msi",
    "processes":["phantom-streamer","phantom-streame"]
  },
  "oden_plugin": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"oden_plugin",
    "version":"1.0.0",
    "link":null,
    "checksum":"",
    "installed":true,
    "package_type":"tar",
    "target_path":"/opt/phantom-streamer",
    "processes":["phantom-streamer","phantom-streame"]
  },
  "autonomy_client":{
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"autonomy_client",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"snap"
  },
  "log2jira": {
    "token":"token",
    "_id":"6208c6e48cff329d56642878",
    "component":"log2jira",
    "version":"1.0.0",
    "link":"https://something.jfrog.io/ui/packages",
    "checksum":"",
    "installed":true,
    "package_type":"msi",
    "processes":[]
  }
}
            "#,
        )
    }
}
