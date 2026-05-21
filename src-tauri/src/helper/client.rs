use crate::helper::{get_status, install, uninstall, HelperStatus};
use crate::xpc::{Connection, Dictionary};

pub struct HelperClient {
    connection: Option<Connection>,
}

impl HelperClient {
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::new("xyz.dvnlabs.xsshtunnel.helper")
            .map_err(|e| format!("Failed to create XPC connection: {}", e))?;
        Ok(Self { connection: Some(conn) })
    }

    pub fn create_tun(&mut self) -> Result<(String, i32), String> {
        let mut dict = Dictionary::create();
        dict.insert("op", "create_tun");

        let reply = self.connection
            .as_ref()
            .ok_or("Not connected")?
            .send_sync_message(&dict)?;

        let name = reply.get_string("tun_name")
            .ok_or("Missing tun_name in reply")?;
        let fd = reply.get_int("tun_fd")
            .ok_or("Missing tun_fd in reply")?;

        Ok((name, fd as i32))
    }

    pub fn add_route(&mut self, tun_name: &str) -> Result<(), String> {
        let mut dict = Dictionary::create();
        dict.insert("op", "add_route");
        dict.insert("tun_name", tun_name);

        let _reply = self.connection
            .as_ref()
            .ok_or("Not connected")?
            .send_sync_message(&dict)?;

        Ok(())
    }

    pub fn cleanup_routes(&mut self, tun_name: &str) -> Result<(), String> {
        let mut dict = Dictionary::create();
        dict.insert("op", "cleanup_routes");
        dict.insert("tun_name", tun_name);

        let _reply = self.connection
            .as_ref()
            .ok_or("Not connected")?
            .send_sync_message(&dict)?;

        Ok(())
    }
}