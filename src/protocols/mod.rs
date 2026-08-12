use async_trait::async_trait;
use crate::credentials::Credentials;
use crate::error::BruteError;

pub mod http;
pub mod ssh;
pub mod ftp;
pub mod smtp;
pub mod sql;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptResult {
    /// Authentication succeeded
    Success(Credentials),
    /// Authentication failed
    Failure,
    /// Connection was blocked or throttled (e.g., rate limit, IP ban)
    Blocked(String),
}

#[async_trait]
pub trait BruteTarget: Send + Sync {
    /// Attempts authentication with the given credentials.
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError>;
}
