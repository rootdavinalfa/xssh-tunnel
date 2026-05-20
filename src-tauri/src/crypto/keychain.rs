use security_framework::passwords::{get_generic_password, set_generic_password};

use crate::error::AppError;
use super::generate_master_key;

const SERVICE_NAME: &str = "xyz.dvnlabs.xsshtunnel";
const ACCOUNT_NAME: &str = "master_key";

pub fn get_or_create_master_key() -> Result<[u8; 32], AppError> {
    // Try to retrieve existing key
    match get_generic_password(SERVICE_NAME, ACCOUNT_NAME) {
        Ok(key_bytes) => {
            if key_bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                Ok(key)
            } else {
                Err(AppError::Tunnel("Invalid master key length in keychain".to_string()))
            }
        }
        Err(_) => {
            // Generate new key and store it
            let key = generate_master_key()?;
            set_generic_password(SERVICE_NAME, ACCOUNT_NAME, &key)
                .map_err(|e| AppError::Tunnel(format!("Failed to store master key: {}", e)))?;
            Ok(key)
        }
    }
}