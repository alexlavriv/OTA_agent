use crate::ota::ota_error::OTAError;
use crate::utils::file_utils;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs, ops};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;
use std::fs::read_dir;
use version_compare::Version;
use crate::utils::color::Coloralex;
use crate::ota::version_table::VersionTable;
use super::hardcoded_manifest::get_hardcoded_manifest;

pub const WINDOWS_PHANTOM_AGENT_PATH: &str = "phantom_agent.exe";
pub const WINDOWS_SERVICE_TRIGGER_PATH: &str = "phantom_agent.flag";

pub const META_SERVER_NAME: &str = "meta_server";
pub const HASH_MANIFEST_PATH: &str = "hash_manifest";
pub const PREVIOUS_INSTALL_PATH: &str = "previous";
pub const FUTURE_VERSION_PATH: &str = "future_version";
pub const DOWNLOAD_DIR: &str = "download";

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ComponentType {
    core,
    sim_gps_info,
    phantom_agent,
    phantom_launcher,
    translator,
    vapp,
    stream_manager,
    sdk_demo,
    oden_player,
    oden_streamer,
    oden_plugin,
    oden_webview,
    autonomy_client,
    log2jira,
}

pub fn current_agent_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

impl FromStr for ComponentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "core" => ComponentType::core,
            "sim_gps_info" => ComponentType::sim_gps_info,
            "phantom_agent" => ComponentType::phantom_agent,
            "phantom_launcher" => ComponentType::phantom_launcher,
            "translator" => ComponentType::translator,
            "vapp" => ComponentType::vapp,
            "stream_manager" => ComponentType::stream_manager,
            "sdk_demo" => ComponentType::sdk_demo,
            "oden_player" => ComponentType::oden_player,
            "oden_streamer" => ComponentType::oden_streamer,
            "oden_plugin" => ComponentType::oden_plugin,
            "oden_webview" => ComponentType::oden_webview,
            "autonomy_client" => ComponentType::autonomy_client,
            "log2jira" => ComponentType::log2jira,
            _ => {
                return Err("No matching snap component".to_string());
            }
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Component {
    pub component: String,
    pub checksum: String,
    #[serde(default)]
    pub updated: bool,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing, default)]
    pub link: Option<Url>,
    #[serde(default)]
    pub target_path: Option<PathBuf>,
    #[serde(default)]
    pub version: String,
    #[serde(skip_serializing, default)]
    pub token: Option<String>,
    #[serde(default)]
    pub package_type: String,
    #[serde(default)]
    pub previous_install_path: Option<PathBuf>,
    #[serde(default)]
    pub processes: Vec<String>,
}

impl Component {
    pub fn empty() -> Self {
        Component {
            component: "".to_string(),
            checksum: "".to_string(),
            updated: false,
            path: None,
            link: None,
            target_path: None,
            version: "".to_string(),
            token: None,
            package_type: "".to_string(),
            previous_install_path: None,
            processes: vec![],
        }
    }

    pub fn currently_installed(&self) -> bool { !self.checksum.is_empty() }
    pub fn should_install(&self) -> bool { !self.updated && self.path.is_some() }
    pub fn should_uninstall(&self) -> bool { !self.updated && self.path.is_none() }
    pub fn uninstall_information(&self) -> (bool, PathBuf) {
        if let Some(prev_path) = self.previous_install_path.clone() {
            if let Ok(files) = read_dir(&prev_path) {
                if files.count() == 1 {
                    if let Ok(mut files) = read_dir(&prev_path) {
                        let file = files.next().expect("Failed to get file").expect("Failed to get file");
                        return (true, file.path());
                    }
                }
            }
        }
        (false, Default::default())
    }
}

impl ops::Add<Component> for Component {
    type Output = Component;

    fn add(self, second: Component) -> Component {
        Component {
            token: second.token,
            link: second.link,
            path: second.path,
            version: second.version,
            target_path: {
                if second.target_path.is_some()
                    && !second
                    .target_path
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .is_empty()
                {
                    second.target_path
                } else {
                    self.target_path
                }
            },
            checksum: second.checksum,
            updated: second.updated,
            processes: {
                if !second.processes.is_empty() {
                    second.processes
                } else {
                    self.processes
                }
            },
            previous_install_path: {
                if self.previous_install_path.is_none() { // Only for tests, probably!
                    second.previous_install_path
                } else {
                    self.previous_install_path
                }
            },
            ..self
        }
    }
}

pub struct HashManifest {
    pub components: HashMap<String, HashMap<ComponentType, String>>,
    pub hash_path: PathBuf,
    pub future_version_path: PathBuf,
    pub read_function: fn(path: &Path) -> Result<String, String>,
    pub write_function: fn(path: &Path, content: &str) -> Result<(), String>,
}

impl HashManifest {
    pub fn new(
        hash_path: PathBuf,
        read_function: fn(path: &Path) -> Result<String, String>,
        write_function: fn(path: &Path, content: &str) -> Result<(), String>,
    ) -> Result<Self, String> {
        let default_json = String::from(r#"{}"#);
        let json_content = read_function(&hash_path).unwrap_or_else(|e| {
            log::warn!("Could not read hash file ({}), using default", e);
            default_json.clone()
        });

        let components = match Manifest::parse_hash_file(&json_content) {
            Ok(components) => components,
            Err(e) => {
                log::error!(
                    "Failed to parse hash file {}: {}, resetting to default",
                    hash_path.to_str().unwrap(),
                    e
                );
                Manifest::parse_hash_file(&default_json).expect("Couldn't parse default value (should be impossible!)")
            }
        };

        let future_version_path = match hash_path.parent() {
            None => PathBuf::from(format!("./{FUTURE_VERSION_PATH}")),
            Some(parent) => parent.join(FUTURE_VERSION_PATH),
        };

        Ok(Self {
            components,
            hash_path,
            read_function,
            write_function,
            future_version_path,
        })
    }

    pub fn standardize(self, server_name: String, operator: bool) -> Self { // Enforce backwards compatibility
        let full_server_name = full_server_name(&server_name, operator);
        let mut new_hash = HashMap::new();
        for (name, server) in self.components {
            if name == server_name {
                log::info!("Old server pattern detected, updating {} to {}", name.clone(), full_server_name);
                new_hash.insert(full_server_name.clone(), server.clone());
            } else {
                new_hash.insert(name.clone(), server.clone());
            }
        }
        Self {
            components: new_hash,
            ..
            self
        }
    }

    pub fn update_components(self, meta_components: HashMap<ComponentType, String>, components: HashMap<ComponentType, String>,
                             server_name: String, operator: bool) -> Self {
        let full_server_name = full_server_name(&server_name, operator);
        let mut new_hash = self.components.clone();
        if new_hash.contains_key(META_SERVER_NAME) {
            let mut updated_meta = new_hash[META_SERVER_NAME].clone();
            for (component_type, checksum) in meta_components {
                updated_meta.insert(component_type, checksum);
            }
            new_hash.insert(META_SERVER_NAME.to_string(), updated_meta);
        } else {
            new_hash.insert(META_SERVER_NAME.to_string(), meta_components);
        }
        new_hash.insert(full_server_name, components);
        //self.components.borrow_mut()[&server_name] = components;
        Self {
            components: new_hash,
            ..
            self
        }
    }

    pub fn write_to_file(&self) -> Result<(), String> {
        let json_str = serde_json::to_string_pretty(&self.components).unwrap();
        let result = (self.write_function)(&self.hash_path, &json_str);
        match &result {
            Ok(_) => {
                log::info!(
                    "Successfully wrote hash manifest into {}",
                    self.hash_path.to_str().unwrap()
                );
            }
            Err(e) => {
                log::warn!(
                    "Error writing hash manifest into {}: {}",
                    self.hash_path.to_str().unwrap(),
                    e
                );
            }
        }
        result
    }

    pub fn update_version_file(&self, version: String) -> Result<(), String> {
        let result = (file_utils::string_to_file)(&self.future_version_path, &version);
        match &result {
            Ok(_) => {
                log::info!("Successfully wrote version {} into version file", version);
            }
            Err(e) => {
                log::error!("Error writing version {} into version file: {}", version, e);
            }
        }
        result
    }

    pub fn verify_version(&self, my_version_str: String) {
        let zero_version = Version::from("").unwrap();
        let my_version = Version::from(&my_version_str).unwrap();
        let mut file_version_str = "".to_string();
        match (file_utils::file_to_string)(&self.future_version_path) {
            Ok(version) => { file_version_str = version; }
            Err(e) => { log::warn!( "Could not read future version file ({}), updating to {}", e, my_version); }
        };

        let mut file_version = Version::from(&file_version_str).unwrap();

        if file_version == zero_version || file_version < my_version { // Proper simver compare
            if !(file_version == zero_version) {
                log::warn!("According to version file we should be {} but our version is {} which is higher! Updating version file...",
                    file_version,
                    my_version
                );
            }
            file_version = match self.update_version_file(my_version.to_string()) {
                Ok(_) => my_version.clone(),
                Err(_) => {
                    log::error!("Failed to update version file!");
                    panic!("Failed to update version file, version tracking impossible!");
                }
            }
        }

        if file_version > my_version {
            let error_msg = format!(
                "According to version file {} we should be {} but our version is {}!",
                self.future_version_path.to_string_lossy(),
                file_version,
                my_version
            );
            log::error!("{error_msg}");
            panic!("{error_msg}");
        }

        log::info!("Successfully verified version as {}", my_version);
    }
}

#[cfg(windows)]
pub fn get_launcher_version() -> String {
    let default = "0.0.0".to_string();
    let ver = match crate::BashExec::exec_arg("PhantomLauncher.exe", &["--version"]) {
        Ok(res) => { res.trim().to_string() }
        Err(e) => {
            log::info!("Cannot extract launcher version [{}], reverting to [{}]", e, default);
            return default;
        }
    };
    if ver.len() < 3 || ver.len() > 20 {
        // Sanity check
        log::info!("Invalid launcher version [{}], reverting to [{}]", ver, default);
        return default;
    }
    log::info!("Launcher version is [{}]", ver);
    ver
}

#[cfg(windows)]
pub fn get_log2jira_version() -> String {
    use crate::ota::msi_installer::MsiInstaller;
    let mut current_version = "0.0.0".to_string();
    let ids = MsiInstaller::get_registry_ids("log2jira");
    for (_, version) in ids {
        if Version::from(&current_version) < Version::from(&version) {
            current_version = version;
        }
    }
    log::info!("Log2jira version is [{}]", current_version);
    current_version
}

pub fn full_server_name(server_name: &str, operator: bool) -> String {
    let server_name = if let Some(stripped) = server_name.strip_prefix("http://") { stripped } else { server_name };
    let server_name = if let Some(stripped) = server_name.strip_prefix("https://") { stripped } else { server_name };
    let server_name = if let Some(stripped) = server_name.strip_suffix('/') { stripped } else { server_name };
    match operator {
        true => "O_".to_string() + server_name,
        false => "V_".to_string() + server_name
    }
}

pub struct Manifest {
    pub components: HashMap<ComponentType, Component>,
    pub hash_manifest: HashManifest,
    pub previous_install_path: PathBuf,
    pub server_name: String,
    pub version: String,
    pub operator: bool,
}

impl Manifest {
    pub fn new(
        operator: bool,
        hash_path: PathBuf,
        previous_install_path: PathBuf,
        server_name: String,
        read_function: fn(path: &Path) -> Result<String, String>,
        write_function: fn(path: &Path, content: &str) -> Result<(), String>,
    ) -> Result<Self, String> {
        let hash_manifest = match HashManifest::new(hash_path.clone(), read_function, write_function) {
            Ok(hash_manifest) => {
                hash_manifest.standardize(server_name.clone(), operator)
            }
            Err(e) => {
                log::error!(
                    "Failed to read hash manifest file {}: {}",
                    hash_path.to_str().unwrap(),
                    e
                );
                panic!("{}", e);
            }
        };

        let full_server_name = full_server_name(&server_name, operator);

        let components = match Manifest::parse_json_file(&get_hardcoded_manifest(operator)) {
            Ok(components) => {
                let components: HashMap<ComponentType, Component> = components
                    .into_iter()
                    .map(|(component_type, prev_component)| {
                        let which_server = match component_type {
                            ComponentType::phantom_agent |
                            ComponentType::phantom_launcher |
                            ComponentType::log2jira => { META_SERVER_NAME }
                            _ => { &full_server_name }
                        };
                        let version = match component_type {
                            ComponentType::phantom_agent => { // Current agent version from env!
                                current_agent_version()
                            }
                            ComponentType::phantom_launcher => {
                                #[cfg(windows)] { get_launcher_version() }
                                #[cfg(unix)] { prev_component.version.clone() }
                            }
                            ComponentType::log2jira => {
                                #[cfg(windows)] { get_log2jira_version() }
                                #[cfg(unix)] { prev_component.version.clone() }
                            }
                            _ => { prev_component.version.clone() }
                        };

                        let previous_install_path = {
                            if previous_install_path == PathBuf::default() { None } else { Some(previous_install_path.join(which_server).join(&prev_component.component)) }
                        };

                        if hash_manifest.components.contains_key(which_server) &&
                            hash_manifest.components[which_server].contains_key(&component_type) {
                            (
                                component_type,
                                Component {
                                    checksum: hash_manifest.components[which_server][&component_type].clone(),
                                    previous_install_path,
                                    updated: true,
                                    version,
                                    ..prev_component
                                },
                            )
                        } else {
                            (
                                component_type,
                                Component {
                                    previous_install_path,
                                    updated: true,
                                    version,
                                    ..prev_component
                                },
                            )
                        }
                    })
                    .collect();
                components
            }
            Err(e) => {
                log::error!("Failed to parse hard-coded manifest: {}", e);
                panic!("{}", e);
            }
        };
        Ok(Self {
            version: "not_supported".to_string(),
            components,
            hash_manifest,
            previous_install_path,
            server_name,
            operator,
        })
    }

    pub fn prepare_for_server_purge(self) -> Manifest {
        let full_server_name = full_server_name(&self.server_name, self.operator);
        if !self.hash_manifest.components.contains_key(&full_server_name) {
            log::error!("Server purge requested, but hash manifest doesn't contain ({})", full_server_name);
            return self;
        }

        let mut components = HashMap::new();
        let mut hash_components = self.hash_manifest.components.clone();
        let mut components_to_uninstall: Vec<ComponentType> = Vec::new();

        for (component_type, checksum) in hash_components.get(&full_server_name).unwrap() {
            if !checksum.is_empty() {
                components_to_uninstall.push(component_type.clone());
            }
        }

        for (component_type, component) in self.components.clone() {
            if components_to_uninstall.contains(&component_type) {  // Preparing components for uninstall
                components.insert(component_type, Component { updated: false, path: None, checksum: String::default(), ..component });
            }
            else {
                components.insert(component_type, Component { updated: true, ..component });
            }
        }

        hash_components.insert(full_server_name, HashMap::new());   // Purging checksum data
        Manifest { components, hash_manifest: HashManifest { components: hash_components, ..self.hash_manifest }, ..self }
    }

    // Partially update the manifest with the new data from json
    pub fn update_with_json(self, json: &str) -> Result<Self, String> {
        // Separate cases - 1. we need to update agent and 2. we need to update everything else
        #[derive(Deserialize)]
        struct ServerManifestModel {
            version: String,
            #[serde(rename(deserialize = "missingComponents"))]
            missing_components: Vec<Component>,
        }
        // Backwards compatability support for 1.27
        let parsed_json = if json.contains("missingComponents") {
            let server_manifest_model: ServerManifestModel = serde_json::from_str(json).map_err(|error| error.to_string())?;
            server_manifest_model
        } else {
            // 1.27 contains list of components only
            let missing_components: Vec<Component> = serde_json::from_str(json).map_err(|error| error.to_string())?;
            ServerManifestModel { version: "not_supported".to_string(), missing_components }
        };
        let log_version_string = format!("Backend manifest is: {}", parsed_json.version);
        log::info!("{}", log_version_string.yellow(true));

        if parsed_json.version == "local" && parsed_json.missing_components.is_empty() {
            log::info!("Got 'local' version, keeping the machine as is.");
            return Ok(Self { version: parsed_json.version, ..self });
        }

        if let Some(new_agent) = parsed_json.missing_components
            .iter()
            .find(|component| component.component == "phantom_agent")
        {
            // Technically shouldn't be a use case, but just in case we don't have agent in hardcoded manifest
            if let Some((_, old_agent)) = self.components.iter().find(|(old_agent_type, _old_agent)| {
                **old_agent_type == ComponentType::phantom_agent
            }) {
                let old_agent_version = Version::from(&old_agent.version).unwrap();
                let new_agent_version = Version::from(&new_agent.version).unwrap();
                if old_agent_version < new_agent_version {
                    // Proper simver compare
                    log::info!("update_with_json: Agent is being updated alone ({} to {})", old_agent_version.clone(), new_agent_version.clone());
                    let components = self
                        .components
                        .into_iter()
                        .map(|(component_type, prev_component)| {
                            if component_type == ComponentType::phantom_agent {
                                (component_type, prev_component + new_agent.clone())
                            } else {
                                (component_type, prev_component)
                            }
                        })
                        .collect();
                    return Ok(Self { components, ..self });
                } else {
                    log::info!("Not updating agent - cloud version is {}, but my version is {}", new_agent.version, old_agent.version);
                }
            }
        }

        let components = self
            .components
            .into_iter()
            .map(|(component_type, prev_component)| {
                let found = parsed_json.missing_components
                    .iter()
                    .find(|component| component.component == prev_component.component);
                if let Some(new_component) = found {
                    let should_update = match component_type {
                        ComponentType::phantom_agent => { false }
                        ComponentType::phantom_launcher => {
                            Version::from(&prev_component.version) < Version::from(&new_component.version)
                        }
                        ComponentType::log2jira => {
                            #[cfg(windows)] { Version::from(&prev_component.version) < Version::from(&new_component.version) }
                            #[cfg(unix)] { new_component.checksum != prev_component.checksum }

                        }
                        // In the new setup cloud gives us ALL the components, so the check to update depends on the CHECKSUM
                        _ => { new_component.checksum != prev_component.checksum }
                    };
                    if should_update {
                        let updated = if prev_component.currently_installed() { "updated" } else { "added" };
                        log::info!("update_with_json: {} is being {}", new_component.component, updated);
                        let new_component = new_component.clone();
                        return (component_type, prev_component + new_component);
                    }
                } else if prev_component.currently_installed() {
                    // We have a component, but we did NOT find it in the server list, that means we should remove it
                    log::info!("update_with_json: {} is being removed", prev_component.component);
                    let new_component = Component {
                        updated: false,
                        link: None,
                        path: None,
                        token: None,
                        ..
                        prev_component
                    };
                    return (component_type, new_component);
                }
                let check = if prev_component.currently_installed() ||
                    component_type == ComponentType::phantom_agent { "*" } else { "" };
                log::info!("update_with_json: not updating {} {}", prev_component.component, check);
                (component_type, prev_component)
            })
            .collect();
        Ok(Self { version: parsed_json.version, components, ..self })
    }

    pub fn update_components_paths(
        self,
        paths: HashMap<ComponentType, PathBuf>,
    ) -> Result<Self, OTAError> {
        let components = self
            .components
            .into_iter()
            .map(|(prev_component_type, prev_component)| {
                if paths.contains_key(&prev_component_type) {
                    (
                        prev_component_type,
                        Component {
                            path: Some(paths.get(&prev_component_type).unwrap().clone()),
                            ..prev_component
                        },
                    )
                } else {
                    (prev_component_type, prev_component)
                }
            })
            .collect();
        Ok(Self { components, ..self })
    }

    pub fn is_fully_installed(&self) -> bool {
        for component in self.components.values() {
            if !component.updated {
                return false;
            }
        }
        true
    }

    pub fn write_to_file(self) -> Result<Self, String> {
        let mut meta_components = HashMap::new();
        let mut components = HashMap::new();
        for (component_type, component) in &self.components {
            if component.updated { // Not updating hashmap if the component wasn't installed!
                match component_type {
                    ComponentType::phantom_agent |
                    ComponentType::phantom_launcher |
                    ComponentType::log2jira => { meta_components.insert(*component_type, component.checksum.clone()); }
                    _ => { components.insert(*component_type, component.checksum.clone()); }
                }
            }
        }
        let hash_manifest = self.hash_manifest.update_components(meta_components, components, self.server_name.clone(), self.operator);
        hash_manifest
            .write_to_file()
            .expect("Failed to write to file");

        VersionTable::new().update_version_file(&self.server_name, &self.version);
        Ok(Self {
            hash_manifest,
            ..self
        })
    }


    pub fn parse_json_array(json: &str) -> Result<Vec<Component>, String> {
        let components = serde_json::from_str(json);
        match components {
            Ok(components) => Ok(components),
            Err(e) => Err(e.to_string()),
        }
    }

    fn parse_json_file(json: &str) -> Result<HashMap<ComponentType, Component>, String> {
        let components = serde_json::from_str(json);
        match components {
            Ok(components) => Ok(components),
            Err(e) => Err(e.to_string()),
        }
    }

    fn parse_hash_file(json: &str) -> Result<HashMap<String, HashMap<ComponentType, String>>, String> {
        let components = serde_json::from_str(json);
        match components {
            Ok(components) => Ok(components),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn update_single_component(self, new_component: &Component) -> Result<Self, String> {
        let components = self
            .components
            .into_iter()
            .map(|(component_type, my_component)| {
                if my_component.component == new_component.component {
                    log::info!(
                        "update_single_component: {} is being updated",
                        new_component.component
                    );
                    let new_component = new_component.clone();
                    (component_type, my_component + new_component)
                } else {
                    (component_type, my_component)
                }
            })
            .collect();
        Ok(Self { components, ..self })
    }

    pub fn standardize_prev_dir(&self) {
        let old_install_path = self.previous_install_path.join(self.server_name.clone());
        let full_server_name = full_server_name(&self.server_name, self.operator);
        let new_install_path = self.previous_install_path.join(full_server_name);
        if old_install_path.exists() {
            if !new_install_path.exists() {
                log::info!("Old previous directory detected, moving {} to {}", old_install_path.to_string_lossy(), new_install_path.to_string_lossy());
                if let Err(e) = fs::rename(old_install_path, new_install_path) {
                    log::warn!("Error moving directory: {}", e);
                }
            } else {
                log::info!("Found leftover previous dir {}, removing... ({} already exists)", old_install_path.to_string_lossy(), new_install_path.to_string_lossy());
                if let Err(e) = fs::remove_dir_all(old_install_path) {
                    log::error!("Error removing directory: {}", e);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn display_component_actions(&self) {
        log::info!("--------------------------------------------------");
        for (_, component) in &self.components {
            if component.should_install() {
                log::info!("{}", format!("WILL INSTALL {} - Installed: {}, Checksum: [{}]", component.component, component.updated, component.checksum).green(true));
            } else if component.should_uninstall() {
                log::info!("{}", format!("WILL UNINSTALL {} - Installed: {}, Checksum: [{}]", component.component, component.updated, component.checksum).red(true));
            } else {
                log::info!("{}",  format!("WILL IGNORE {} - Installed: {}, Checksum: [{}]", component.component, component.updated, component.checksum).yellow(true));
            }
        }
        log::info!("--------------------------------------------------");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::file_utils;
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::ota::hardcoded_manifest::hardcoded_manifest_size;
    use std::fs;
    use std::path::Path;
    use crate::utils::file_utils::string_to_file;

    fn read_function(_path: &Path) -> Result<String, String> {
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
    fn update_from_json() {
        let write_function = |_: &Path, _: &str| Ok(());
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            Default::default(),
            read_function,
            write_function,
        )
            .unwrap();
        assert_eq!(manifest.components.len(), hardcoded_manifest_size(true));
        let core_component = manifest.components.get(&ComponentType::core).unwrap();
        assert_eq!(core_component.component, String::from("core"));
        assert_eq!(core_component.updated, true);

        let server_manifest_json = r#"
        [
  {
    "token": "eyJ2ZXIiOiIyIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYiLCJraWQiOiJIT0JEU0RTaWN0TWhYUlBJck95VG9zb1RFUlg0UVZKTGtweUZLNnVJengwIn0.eyJzdWIiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqXC91c2Vyc1wvdmVoaWNsZSIsInNjcCI6Im1lbWJlci1vZi1ncm91cHM6UGhhbnRvbS5CaW5hcnkuUk8iLCJhdWQiOiJqZnJ0QDAxZWE3MHQ1aHdneDZlMXgwMWs4MDYxOHZqIiwiaXNzIjoiamZydEAwMWVhNzB0NWh3Z3g2ZTF4MDFrODA2MTh2alwvdXNlcnNcL2JhY2tvZmZpY2UuYWdlbnQiLCJleHAiOjE2NDYwNDE0MjMsImlhdCI6MTY0NjAzNzgyMywianRpIjoiOWNhZWEwYTYtMzI1NS00Nzk0LThiNDQtYTM4ZTU3ZjVjMTcxIn0.R9F770-xVLNxvx9ospd9XIQN2WzfGd7VUgfc80DSsNCpwJVD6FFI0lkPdvslH9V4Eu_eg0TkKBJCWihGxjGE5k8ttyo5dFH3ce7kY6r-soV8xYqau1fTA0TC722x_6P4H9A0GQOYioGZZFawYQx6P4m4JeELXkyPXzPOZbTm7NhR7RqetjlMvF2L39lem56byGrUUXFqB3Uerk9iLpwhuuoJiY_yuOrrUZk2urSkurBUoc-oRM8iTl0MpPZe3ROgce3ZaBmVV-qGqk51isb8GF3klJRdjnLKs6DemWH2jOJZTfSOJhIoaY0dj1e82Y58BYZbGz71kgJ4FxuURMrbOA",
    "_id": "621c78f3fd12780012c795da",
    "component": "core",
    "version": "0.1.2",
    "link": "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap",
    "checksum": "new checksum",
    "arch": "AMD64"
  }
]
        "#;
        let updated_manifest = manifest.update_with_json(server_manifest_json).unwrap();
        let core_component = updated_manifest
            .components
            .get(&ComponentType::core)
            .unwrap();
        assert_eq!(core_component.updated, false);
        assert_eq!(core_component.checksum, String::from("new checksum"));
        assert_eq!(core_component.link, Some(Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/0.1.2/amd64/phantom-agent_0.1.2_amd64.snap").unwrap()));
    }

    #[test]
    fn update_from_hash() {
        let write_function = |_: &Path, str: &str| {
            println!("WRITE: {}", str);
            Ok(())
        };
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            "test_server".to_string(),
            read_function,
            write_function,
        )
            .unwrap();
        assert_eq!(manifest.components.len(), hardcoded_manifest_size(true));
        assert_eq!(
            manifest
                .components
                .get(&ComponentType::core)
                .unwrap()
                .component,
            String::from("core")
        );
        assert_eq!(
            // Checking that we updated the manifest with the provided hash_manifest checksums
            manifest
                .components
                .get(&ComponentType::core)
                .unwrap()
                .checksum,
            String::from("core checksum")
        );
        let manifest = manifest.write_to_file().unwrap();
        println!("MANIFEST IS: [{:?}]", manifest.components);
        #[cfg(unix)]
        {
            let manifest = Manifest::new(
                false,
                PathBuf::from("./hash_manifest.json"),
                PathBuf::from("./previous"),
                Default::default(),
                read_function,
                write_function,
            )
                .unwrap();
            assert_eq!(manifest.components.len(), hardcoded_manifest_size(false));
        }
    }

    #[test]
    fn make_component_from_json() {
        let json = r#"
        {
    "component": "phantom_agent",
    "checksum": "",
    "installed": false,
    "version":"1.2.3",
    "path": null,
    "link": null
  }
        "#;
        let component: Component = serde_json::from_str(&json).unwrap();
        assert_eq!(component.link, None);
        assert_eq!(component.version, "1.2.3".to_string());
    }

    #[test]
    fn component_enums_sanity() {
        let write_function = |_: &Path, _: &str| Ok(());
        let operator_manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            Default::default(),
            read_function,
            write_function,
        )
            .unwrap();
        for (component_type, component) in operator_manifest.components {
            assert_eq!(
                component_type,
                ComponentType::from_str(&component.component).unwrap()
            );
        }
        #[cfg(unix)]
        {
            let vehicle_manifest = Manifest::new(
                false,
                PathBuf::from("./hash_manifest.json"),
                PathBuf::from("./previous"),
                Default::default(),
                read_function,
                write_function,
            )
                .unwrap();
            for (component_type, component) in vehicle_manifest.components {
                assert_eq!(
                    component_type,
                    ComponentType::from_str(&component.component).unwrap()
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn version_file_sanity() {
        set_logging_for_tests(log::LevelFilter::Info);
        log::info!("Our real version is {}", current_agent_version());
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("version_file_sanity_dir"));
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to remove dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        let manifest = Manifest::new(
            true,
            test_dir.join("hash_manifest.json"),
            test_dir.join("previous"),
            Default::default(),
            file_utils::file_to_string,
            file_utils::string_to_file,
        )
            .unwrap();
        manifest.hash_manifest.verify_version("TEST_VERSION1".to_string()); // Initializing to TEST_VERSION1
        manifest.hash_manifest.verify_version("TEST_VERSION1".to_string()); // Verifying as TEST_VERSION1 (expecting success)
        manifest.hash_manifest.verify_version("TEST_VERSION2".to_string()); // TEST_VERSION2 is HIGHER than TEST_VERSION1, so we expect it to PASS and update to TEST_VERSION2
        let result = std::panic::catch_unwind(|| manifest.hash_manifest.verify_version("TEST_VERSION1".to_string()));
        assert!(result.is_err()); // This time we are TEST_VERSION1 but what we should be was updated to TEST_VERSION2 so it should fail!
        fs::remove_dir_all(test_dir).expect("Failed to cleanup!");
    }

    #[test]
    #[ignore]
    fn empty_manifest_check() {
        set_logging_for_tests(log::LevelFilter::Info);
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("empty_manifest_check"));
        if !test_dir.exists() {
            fs::create_dir(&test_dir).expect("Failed to create dir!");
        }
        let manifest = Manifest::new(
            true,
            test_dir.join("hash_manifest.json"),
            test_dir.join("previous"),
            Default::default(),
            file_utils::file_to_string,
            file_utils::string_to_file,
        ).unwrap();
        log::info!("MANIFEST IS: <<<{:?}>>>", manifest.components);
        fs::remove_dir_all(test_dir).expect("Failed to cleanup!");
    }

    #[cfg(windows)]
    #[test]
    #[ignore]
    fn launcher_version() {
        set_logging_for_tests(log::LevelFilter::Info);
        let version = get_launcher_version();
        log::info!("Version is [{}]", version);
    }

    pub fn verify_version(first: String, second: String) -> bool {
        let first_version = Version::from(&first).unwrap();
        let second_version = Version::from(&second).unwrap();
        if first_version < second_version {
            log::info!("COMPARE: [{}] < [{}]", first_version, second_version);
        }
        if first_version == second_version {
            log::info!("COMPARE: [{}] == [{}]", first_version, second_version);
        }
        if first_version > second_version {
            log::info!("COMPARE: [{}] > [{}]", first_version, second_version);
        }
        first_version < second_version
    }

    #[test]
    #[ignore]
    fn versions_compare_sanity() {
        set_logging_for_tests(log::LevelFilter::Info);
        assert!(verify_version("".to_string(), "1.0.0".to_string()));
        assert!(verify_version("".to_string(), "2.0.0".to_string()));
        assert!(!verify_version("1.0.0".to_string(), "1.0.0".to_string()));
        assert!(verify_version("1.0.0".to_string(), "2.0.0".to_string()));
        assert!(!verify_version("2.0.0".to_string(), "1.0.0".to_string()));
        assert!(!verify_version("".to_string(), "0.0.0".to_string()));
        assert!(!verify_version("1".to_string(), "1.a".to_string()));
        assert!(verify_version("1".to_string(), "a.1".to_string()));
        assert!(verify_version("2.0.1".to_string(), "2.0.2".to_string()));
        assert!(!verify_version("2.0.20".to_string(), "2.0.2".to_string()));
        assert!(!verify_version("1.10.1".to_string(), "1.9.1".to_string()));
        assert!(verify_version("b.1.2".to_string(), "1.3.0".to_string()));
        assert!(verify_version("b.1.2".to_string(), "1.2.0".to_string()));
        assert!(!verify_version("1.2".to_string(), "1.2.0".to_string()));
        assert!(verify_version("b.1.2".to_string(), "1.2".to_string()));
        assert!(!verify_version("b.1.2".to_string(), "1.1.0".to_string()));
        assert!(verify_version("a.0.20".to_string(), "b.0.20".to_string()));
        assert!(!verify_version("1.1.1".to_string(), "1.1.1.extra".to_string()));
        assert!(verify_version("2.27.11-JT4.5".to_string(), "2.27.11-JT4.6".to_string()));
    }

    #[test]
    #[ignore]
    fn manifests_with_display() {
        set_logging_for_tests(log::LevelFilter::Info);
        let write_function = |_: &Path, str: &str| {
            println!("WRITE: {}", str);
            Ok(())
        };
        let manifest = Manifest::new(
            true,
            PathBuf::from("./hash_manifest.json"),
            PathBuf::from("./previous"),
            "test_server".to_string(),
            read_function,
            write_function,
        ).unwrap();
        manifest.display_component_actions();

        let server_manifest_json = r#"
        [
  {
    "token": "token",
    "_id": "621c78f3fd12780012c795da",
    "component": "oden_plugin",
    "version": "0.1.2",
    "link": "https://oden_plugin_link",
    "checksum": "plugin checksum",
    "path": "plugin path",
    "arch": "WIN"
  },
  {
    "token": "token",
    "_id": "621c78f3fd12780012c795da",
    "component": "oden_player",
    "version": "0.1.2",
    "link": "https://oden_player_link",
    "checksum": "player checksum",
    "path": "token path",
    "arch": "WIN"
  },
  {
    "token": "token",
    "_id": "621c78f3fd12780012c795da",
    "component": "oden_webview",
    "version": "0.1.2",
    "link": "https://oden_webview_link",
    "checksum": "oden webview NEW checksum",
    "path": "webview path",
    "arch": "WIN"
  }
]
        "#;
        let updated_manifest = manifest.update_with_json(server_manifest_json).unwrap();
        updated_manifest.display_component_actions();
    }

    #[test]
    fn directory_move_sanity() {
        let top = std::env::current_dir().unwrap();
        let test_dir = top.join(Path::new("directory_move_check"));
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to remove dir!");
        }
        fs::create_dir(&test_dir).expect("Failed to create dir!");
        let moving_dir1 = test_dir.join("moving1");
        let moving_dir2 = test_dir.join("moving2");
        fs::create_dir(&moving_dir1).expect("Failed to create dir!");
        let moving_file1 = moving_dir1.join("file");
        let moving_file2 = moving_dir2.join("file");
        string_to_file(&moving_file1, "FILEFILEFILE").expect("Failed to write file!");
        if let Err(e) = fs::rename(&moving_dir1, &moving_dir2) {
            log::warn!("Error moving directory: {}", e);
        }
        assert!(moving_dir2.exists());
        assert!(moving_file2.exists());
        fs::remove_dir_all(test_dir).expect("Cleanup failed!");
    }

    #[test]
    fn test_full_server_name() {
        assert_eq!(full_server_name("http://example.com", true), "O_example.com");
        assert_eq!(full_server_name("https://example.com", false), "V_example.com");
        assert_eq!(full_server_name("example.com", false), "V_example.com");
        assert_eq!(full_server_name("example.com/", true), "O_example.com");
        assert_eq!(full_server_name("https://example.com/", true), "O_example.com");
    }

    #[test]
    #[ignore]
    #[cfg(windows)]
    fn test_log2jira_version() {
        let version = get_log2jira_version();
        println!("Log2jira version is {version}");
    }
}
