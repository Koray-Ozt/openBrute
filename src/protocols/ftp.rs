use std::time::Duration;
use async_trait::async_trait;
use suppaftp::AsyncFtpStream;

use crate::credentials::Credentials;
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

pub struct FtpTarget {
    host: String,
    port: u16,
}

impl FtpTarget {
    pub fn new(target: &str) -> Result<Self, BruteError> {
        let parts: Vec<&str> = target.split(':').collect();
        let host = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().map_err(|e| BruteError::Connection(format!("Invalid FTP port: {}", e)))?
        } else {
            21
        };

        Ok(Self { host, port })
    }
}

#[async_trait]
impl BruteTarget for FtpTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        let addr = format!("{}:{}", self.host, self.port);
        let connect_fut = AsyncFtpStream::connect(&addr);
        
        let mut ftp_stream = match tokio::time::timeout(Duration::from_secs(10), connect_fut).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => return Ok(AttemptResult::Blocked(format!("FTP Connect error: {}", e))),
            Err(_) => return Ok(AttemptResult::Blocked("Connection timeout".to_string())),
        };

        match ftp_stream.login(&credentials.username, &credentials.password).await {
            Ok(_) => {
                let _ = ftp_stream.quit().await;
                Ok(AttemptResult::Success(credentials.clone()))
            }
            Err(_) => Ok(AttemptResult::Failure),
        }
    }
}
