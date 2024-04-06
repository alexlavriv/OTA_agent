use crate::auth::license_manager_trait::AuthError;
use base64::{engine::general_purpose, Engine as _};
use hmacsha1;
use sha2::{Digest, Sha256};

#[allow(dead_code)]
pub struct FileAuth {
    pub name: String,
    pub server: String,
    pub b64_key: String,
    pub license_id: String,
}
#[allow(dead_code)]
impl FileAuth {
    /// Returns the value in seconds.
    pub fn get_challenge_response(&self, challenge_b64: &str) -> Result<String, AuthError> {
        // Currently we support only 20bytes keys
        if let Ok(decoded_challnge) = general_purpose::STANDARD.decode(challenge_b64) {
            // create a Sha256 object
            let mut hasher = Sha256::new();
            // write input message
            hasher.update(decoded_challnge);
            // read hash digest and consume hasher
            let hash_of_challenge = hasher.finalize();

            if let Ok(decoded_key) = general_purpose::STANDARD.decode(&self.b64_key) {
                let digest = hmacsha1::hmac_sha1(&decoded_key, &hash_of_challenge);
                // from hex to bytes
                //  let digest_bytes = hex::decode(digest).unwrap().as_bytes();
                let digest_encoded = general_purpose::STANDARD.encode(digest);
                Ok(digest_encoded)
            } else {
                Err(AuthError::DecodingError(String::from(
                    "Failed decoding key",
                )))
            }
        } else {
            Err(AuthError::DecodingError(String::from(
                "Failed decoding challenge",
            )))
        }
    }
    // key encoded to base64
    fn new(name: &str, server: &str, key: &str, license_id: &str) -> FileAuth {
        FileAuth {
            name: String::from(name),
            server: String::from(server),
            b64_key: String::from(key),
            license_id: String::from(license_id),
        }
    }
}
