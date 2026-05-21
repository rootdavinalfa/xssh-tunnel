use crate::helper::{get_status, install, uninstall, HelperStatus};

pub struct HelperClient;

impl HelperClient {
    pub fn status() -> Result<HelperStatus, String> {
        get_status()
    }

    pub fn install(bundle_path: &str) -> Result<(), String> {
        install(bundle_path)
    }

    pub fn uninstall() -> Result<(), String> {
        uninstall()
    }
}