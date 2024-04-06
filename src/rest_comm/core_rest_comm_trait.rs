pub use mockall::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

// Just ignore everything else
#[derive(Serialize, Deserialize)]
pub struct PortsStruct {
    pub stats_server: i32,
    pub sdk_server: i32,
    // Also sessions which we ignore for now?
}

// Just ignore everything else
#[derive(Serialize, Deserialize)]
pub struct PortsResponse {
    pub success: bool,
    pub ports: PortsStruct,
}

#[automock]
pub trait CoreRestCommTrait {
    fn is_core_has_connected_session(&self) -> bool;
    fn install_snap(&self, snap_path: &Path) -> Result<(), String>;
    fn update_manifest_version(&self, version : &str);

}
