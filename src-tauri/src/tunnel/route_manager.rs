use std::process::Command;
use crate::error::AppError;

pub struct RouteManager;

impl RouteManager {
    /// Injects a default route through the TUN device.
    /// This requires root privileges and will fail otherwise.
    pub fn inject_default_route(tun_name: &str) -> Result<(), AppError> {
        // Add default route through TUN (split into two /1 routes)
        let status = Command::new("route")
            .args(["add", "-net", "0.0.0.0/1", "-interface", tun_name])
            .status()
            .map_err(|e| AppError::Route(format!("Failed to add route: {}", e)))?;

        if !status.success() {
            return Err(AppError::Route("Failed to add default route".to_string()));
        }

        let status = Command::new("route")
            .args(["add", "-net", "128.0.0.0/1", "-interface", tun_name])
            .status()
            .map_err(|e| AppError::Route(format!("Failed to add route: {}", e)))?;

        if !status.success() {
            // Attempt cleanup
            let _ = Command::new("route")
                .args(["delete", "-net", "0.0.0.0/1"])
                .status();
            return Err(AppError::Route("Failed to add default route (128)".to_string()));
        }

        Ok(())
    }

    pub fn cleanup_routes(_tun_name: &str) -> Result<(), AppError> {
        // Remove the two default routes we added
        let _ = Command::new("route")
            .args(["delete", "-net", "0.0.0.0/1"])
            .status();

        let _ = Command::new("route")
            .args(["delete", "-net", "128.0.0.0/1"])
            .status();

        Ok(())
    }
}