use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Serialize, Clone)]
pub struct SshConfigEntry {
    pub host_aliases: Vec<String>,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    pub entries: Vec<SshConfigEntry>,
    pub skipped: Vec<String>,
}

pub fn parse_ssh_config(path: Option<&Path>) -> Result<ParseResult, AppError> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let config_path = path.unwrap_or(&PathBuf::from(&home).join(".ssh").join("config")).to_path_buf();

    if !config_path.exists() {
        return Err(AppError::Tunnel(format!(
            "SSH config not found at {}",
            config_path.display()
        )));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| AppError::Tunnel(format!("Failed to read SSH config: {}", e)))?;

    let mut entries: Vec<SshConfigEntry> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut current_aliases: Vec<String> = Vec::new();
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_port: Option<u16> = None;
    let mut current_identity_file: Option<String> = None;
    let mut in_host_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle Include directive
        if trimmed.to_lowercase().starts_with("include ") {
            let include_path_str = trimmed[8..].trim();
            let expanded = shellexpand::tilde(include_path_str);
            let include_path = PathBuf::from(expanded.as_ref());

            let full_path = if include_path.is_relative() {
                config_path.parent().unwrap_or(Path::new(".")).join(&include_path)
            } else {
                include_path
            };

            if full_path.exists() {
                let include_result = parse_ssh_config(Some(&full_path))?;
                entries.extend(include_result.entries);
                skipped.extend(include_result.skipped);
            }
            continue;
        }

        if trimmed.to_lowercase().starts_with("host ") {
            // Save previous block
            if in_host_block {
                if let Some(hostname) = current_hostname.take() {
                    if current_aliases.iter().any(|a| a.contains('*') || a.contains('?')) {
                        skipped.push(current_aliases[0].clone());
                    } else {
                        entries.push(SshConfigEntry {
                            host_aliases: current_aliases.clone(),
                            hostname,
                            user: current_user.take(),
                            port: current_port.take(),
                            identity_file: current_identity_file.take(),
                        });
                    }
                }
                current_aliases.clear();
            }

            in_host_block = true;
            let hosts_part = &trimmed[5..].trim();
            current_aliases = hosts_part.split_whitespace().map(|s| s.to_string()).collect();
            current_hostname = None;
            current_user = None;
            current_port = None;
            current_identity_file = None;
            continue;
        }

        if !in_host_block {
            continue;
        }

        if let Some(val) = trimmed.strip_prefix("HostName ").or_else(|| trimmed.strip_prefix("hostname ")) {
            current_hostname = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("User ").or_else(|| trimmed.strip_prefix("user ")) {
            current_user = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("Port ").or_else(|| trimmed.strip_prefix("port ")) {
            current_port = val.trim().parse::<u16>().ok();
        } else if let Some(val) = trimmed.strip_prefix("IdentityFile ").or_else(|| trimmed.strip_prefix("identityfile ")) {
            current_identity_file = Some(shellexpand::tilde(val.trim()).to_string());
        }
    }

    // Save last block
    if in_host_block {
        if let Some(hostname) = current_hostname {
            if current_aliases.iter().any(|a| a.contains('*') || a.contains('?')) {
                skipped.push(current_aliases[0].clone());
            } else {
                entries.push(SshConfigEntry {
                    host_aliases: current_aliases.clone(),
                    hostname,
                    user: current_user,
                    port: current_port,
                    identity_file: current_identity_file,
                });
            }
        } else {
            for alias in &current_aliases {
                if !alias.contains('*') && !alias.contains('?') {
                    skipped.push(format!("{} (no HostName)", alias));
                }
            }
        }
    }

    Ok(ParseResult { entries, skipped })
}
