use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use russh::client;
use russh_keys::key;
use tokio::net::lookup_host;

use crate::credentials::Credentials;
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

struct SshHandler;

#[async_trait]
impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _server_public_key: &key::PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct SshTarget {
    host: String,
    port: u16,
    timeout: Duration,
}

impl SshTarget {
    pub fn new(target: &str) -> Result<Self, BruteError> {
        let parts: Vec<&str> = target.split(':').collect();
        let host = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().map_err(|e| BruteError::Connection(format!("Invalid SSH port: {}", e)))?
        } else {
            22
        };

        Ok(Self {
            host,
            port,
            timeout: Duration::from_secs(10),
        })
    }
}

#[async_trait]
impl BruteTarget for SshTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        let config = Arc::new(client::Config::default());

        let addr = format!("{}:{}", self.host, self.port);
        let socket_addrs: Vec<_> = lookup_host(&addr)
            .await
            .map_err(|e| BruteError::Connection(format!("Failed to resolve {}: {}", addr, e)))?
            .collect();
            
        if socket_addrs.is_empty() {
            return Err(BruteError::Connection(format!("No IP addresses found for {}", addr)));
        }
        
        let sh = SshHandler;
        
        let connect_fut = client::connect(config, socket_addrs[0], sh);
        let mut session = match tokio::time::timeout(self.timeout, connect_fut).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Ok(AttemptResult::Blocked(format!("SSH Handshake error: {}", e))),
            Err(_) => return Ok(AttemptResult::Blocked("Connection timeout".to_string())),
        };

        let auth_res = session
            .authenticate_password(&credentials.username, &credentials.password)
            .await
            .map_err(|e| BruteError::Auth(format!("Authentication error: {}", e)))?;

        if auth_res {
            Ok(AttemptResult::Success(credentials.clone()))
        } else {
            Ok(AttemptResult::Failure)
        }
    }
}
