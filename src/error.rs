use std::io;

#[derive(Debug, thiserror::Error)]
pub enum BruteError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("SQL database error: {0}")]
    Database(String),

    #[error("Generic error: {0}")]
    Generic(String),
}
