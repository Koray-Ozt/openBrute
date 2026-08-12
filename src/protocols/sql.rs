use std::time::Duration;
use async_trait::async_trait;
use sqlx::{Connection, MySqlConnection, PgConnection};
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;

use crate::credentials::Credentials;
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

pub enum SqlDriver {
    MySql,
    Postgres,
}

pub struct SqlTarget {
    driver: SqlDriver,
    host: String,
    port: u16,
    database: Option<String>,
}

impl SqlTarget {
    pub fn new(target: &str) -> Result<Self, BruteError> {
        let parsed = url::Url::parse(target)
            .map_err(|e| BruteError::Connection(format!("Invalid URL or target format: {}", e)))?;
            
        let driver = match parsed.scheme() {
            "mysql" => SqlDriver::MySql,
            "postgres" | "postgresql" => SqlDriver::Postgres,
            other => return Err(BruteError::Protocol(format!("Unsupported database driver: {}", other))),
        };

        let host = parsed.host_str()
            .ok_or_else(|| BruteError::Connection("Missing host in database URL".to_string()))?
            .to_string();

        let port = parsed.port().unwrap_or_else(|| match driver {
            SqlDriver::MySql => 3306,
            SqlDriver::Postgres => 5432,
        });

        let database = parsed.path().trim_start_matches('/').to_string();
        let database = if database.is_empty() { None } else { Some(database) };

        Ok(Self {
            driver,
            host,
            port,
            database,
        })
    }
}

#[async_trait]
impl BruteTarget for SqlTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        let db_name = self.database.as_deref().unwrap_or("");
        
        match self.driver {
            SqlDriver::MySql => {
                let mut options = MySqlConnectOptions::new()
                    .host(&self.host)
                    .port(self.port)
                    .username(&credentials.username)
                    .password(&credentials.password);
                
                if !db_name.is_empty() {
                    options = options.database(db_name);
                }

                let connect_fut = MySqlConnection::connect_with(&options);
                match tokio::time::timeout(Duration::from_secs(10), connect_fut).await {
                    Ok(Ok(mut conn)) => {
                        let _ = conn.ping().await;
                        Ok(AttemptResult::Success(credentials.clone()))
                    }
                    Ok(Err(e)) => {
                        let err_str = e.to_string();
                        if err_str.contains("Access denied") || err_str.contains("1045") {
                            Ok(AttemptResult::Failure)
                        } else {
                            Ok(AttemptResult::Blocked(err_str))
                        }
                    }
                    Err(_) => Ok(AttemptResult::Blocked("Connection timeout".to_string())),
                }
            }
            SqlDriver::Postgres => {
                let mut options = PgConnectOptions::new()
                    .host(&self.host)
                    .port(self.port)
                    .username(&credentials.username)
                    .password(&credentials.password);

                if !db_name.is_empty() {
                    options = options.database(db_name);
                }

                let connect_fut = PgConnection::connect_with(&options);
                match tokio::time::timeout(Duration::from_secs(10), connect_fut).await {
                    Ok(Ok(mut conn)) => {
                        let _ = conn.ping().await;
                        Ok(AttemptResult::Success(credentials.clone()))
                    }
                    Ok(Err(e)) => {
                        let err_str = e.to_string();
                        if err_str.contains("password authentication failed") || err_str.contains("28P01") {
                            Ok(AttemptResult::Failure)
                        } else {
                            Ok(AttemptResult::Blocked(err_str))
                        }
                    }
                    Err(_) => Ok(AttemptResult::Blocked("Connection timeout".to_string())),
                }
            }
        }
    }
}
