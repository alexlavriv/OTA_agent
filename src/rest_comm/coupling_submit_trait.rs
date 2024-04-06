pub use mockall::*;
use std::path::Path;
use serde::Serialize;
use url::Url;


#[derive(Debug,Serialize,PartialEq)] #[serde(rename_all = "lowercase")]
pub enum NodeOtaProgressStatus {
    Failed,
    Triggered,
    Updating,
    Downloading,
    Installing,
    Updated,
}
#[derive(Debug, Serialize)]
pub struct NodeOtaStatus{
    pub eta: u64,
    pub status: NodeOtaProgressStatus,
    pub message: String
}


impl NodeOtaProgressStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeOtaProgressStatus::Failed => "failed",
            NodeOtaProgressStatus::Triggered => "triggered",
            NodeOtaProgressStatus::Updating => "updating",
            NodeOtaProgressStatus::Updated => "updated",
            NodeOtaProgressStatus::Downloading => "updating",
            NodeOtaProgressStatus::Installing => "updating"
        }
    }
    pub fn from_string(str: &str) -> NodeOtaProgressStatus {
        match str {
            "failed" => NodeOtaProgressStatus::Failed,
            "triggered" => NodeOtaProgressStatus::Triggered,
            "updating" => NodeOtaProgressStatus::Updating,
            "updated" => NodeOtaProgressStatus::Updated,
            _ => NodeOtaProgressStatus::Triggered,
        }
    }
}

#[automock]
pub trait CouplingRestSubmitter {
    fn post_checksums(&self, checksums: serde_json::Value) -> Result<String, String>;
    fn put_ota_status(&self, message: Option<String>, eta: Option<u64>, ota_progress: NodeOtaProgressStatus);
    fn send_file_to_jira(&self, file: &Path, ticket: &str) -> Result<String, String>;
    fn get_url_and_token(&self) -> (Url, String);
    fn check_versions(&self) -> Result<String, String>;
    fn get_node_info(&self) -> Result<String, String>;
    fn get_ota_status(&self) -> NodeOtaProgressStatus;
}
