use std::time::Duration;
use async_trait::async_trait;
use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials as SmtpCredentials};

use crate::credentials::Credentials;
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

pub struct SmtpTarget {
    host: String,
    port: u16,
}

impl SmtpTarget {
    pub fn new(target: &str) -> Result<Self, BruteError> {
        let parts: Vec<&str> = target.split(':').collect();
        let host = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().map_err(|e| BruteError::Connection(format!("Invalid SMTP port: {}", e)))?
        } else {
            587
        };

        Ok(Self { host, port })
    }
}

#[async_trait]
impl BruteTarget for SmtpTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        let smtp_creds = SmtpCredentials::new(credentials.username.clone(), credentials.password.clone());
        
        let mailer: AsyncSmtpTransport<Tokio1Executor> = match AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host) {
            Ok(builder) => builder
                .port(self.port)
                .timeout(Some(Duration::from_secs(10)))
                .credentials(smtp_creds)
                .build(),
            Err(e) => return Err(BruteError::Connection(format!("Failed to build SMTP transport: {}", e))),
        };

        match mailer.test_connection().await {
            Ok(true) => Ok(AttemptResult::Success(credentials.clone())),
            Ok(false) => Ok(AttemptResult::Failure),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Authentication") || err_str.contains("credentials") || err_str.contains("535") {
                    Ok(AttemptResult::Failure)
                } else {
                    Ok(AttemptResult::Blocked(err_str))
                }
            }
        }
    }
}
