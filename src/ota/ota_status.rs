use serde::ser::{Serialize, SerializeStruct, Serializer};
use crate::ota::manifest::ComponentType;


#[derive(Debug, Clone)]
pub struct OTAStatusRestResponse {
    pub ota_status: OTAStatus,
    pub message: String,
    pub manifest_version: String,
}


#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OTAStatus {
    ERROR,
    DOWNLOADING(u64),
    CHECKING,
    INSTALLING(ComponentType),
    UPDATED,
}

impl Serialize for OTAStatusRestResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        let mut state = serializer.serialize_struct("OTAStatusRestResponse", 4)?;
        match self.ota_status.clone() {
            OTAStatus::DOWNLOADING(eta) => {
                state.serialize_field("ota_status", "downloading")?;
                state.serialize_field("eta", &eta)?;
            }
            OTAStatus::INSTALLING(component_type) => {
                state.serialize_field("ota_status", "installing")?;
                state.serialize_field("component_name", &component_type)?;
            }
            _ => {
                state.serialize_field("ota_status", &self.ota_status)?;
            }
        };

        state.serialize_field("message", &self.message)?;
        state.serialize_field("manifest_version", &self.manifest_version)?;
        state.end()
    }
}

#[test]
fn test_serialization()
{
    let response = OTAStatusRestResponse {
        ota_status: OTAStatus::DOWNLOADING(10),
        message: "test".to_string(),
        manifest_version: "1.2.3".to_string(),
    };
    let response = serde_json::to_string(&response).unwrap();
    let expected = r#"{"ota_status":"downloading","eta":10,"message":"test","manifest_version":"1.2.3"}"#;
    assert_eq!(expected, response);

    let response = OTAStatusRestResponse {
        ota_status: OTAStatus::ERROR,
        message: "test".to_string(),
        manifest_version: "1.2.3".to_string(),
    };
    let response = serde_json::to_string(&response).unwrap();
    let expected = r#"{"ota_status":"error","message":"test","manifest_version":"1.2.3"}"#;
    assert_eq!(expected, response);

    let response = OTAStatusRestResponse {
        ota_status: OTAStatus::INSTALLING(ComponentType::autonomy_client),
        message: "test".to_string(),
        manifest_version: "1.2.3".to_string(),
    };
    let response = serde_json::to_string(&response).unwrap();
    let expected = r#"{"ota_status":"installing","component_name":"autonomy_client","message":"test","manifest_version":"1.2.3"}"#;
    assert_eq!(expected, response);
}