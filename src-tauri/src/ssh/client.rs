use async_trait::async_trait;
use russh::*;
use russh_keys::*;
use std::sync::Arc;

use crate::error::AppError;

pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

pub struct SshClient {
    pub handle: client::Handle<ClientHandler>,
}

struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: Host key verification (M4)
        Ok(true)
    }
}

impl SshClient {
    pub async fn connect(config: SshConfig) -> Result<Self, AppError> {
        let client_config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        };
        let client_config = Arc::new(client_config);

        let handler = ClientHandler;
        let mut session = client::connect(client_config, (config.host.as_str(), config.port), handler)
            .await
            .map_err(|e| AppError::Ssh(format!("Connection failed: {}", e)))?;

        let auth_res = session
            .authenticate_password(config.username, config.password)
            .await
            .map_err(|e| AppError::Ssh(format!("Auth failed: {}", e)))?;

        if !auth_res {
            return Err(AppError::Ssh("Password authentication failed".to_string()));
        }

        Ok(SshClient { handle: session })
    }

    /// Open a dynamic forwarding channel (SOCKS5 proxy)
    pub async fn open_tcp_channel(&self, host: &str, port: u16) -> Result<Channel<client::Msg>, AppError> {
        let channel = self.handle
            .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
            .await
            .map_err(|e| AppError::Ssh(format!("Channel open failed: {}", e)))?;
        Ok(channel)
    }

    pub async fn disconnect(self) -> Result<(), AppError> {
        self.handle
            .disconnect(Disconnect::ByApplication, "User disconnected", "")
            .await
            .map_err(|e| AppError::Ssh(format!("Disconnect failed: {}", e)))?;
        Ok(())
    }
}