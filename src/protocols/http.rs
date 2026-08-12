use std::time::Duration;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use url::Url;

use crate::credentials::Credentials;
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

#[derive(Debug, Clone)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum HttpMode {
    Basic,
    Form {
        user_field: String,
        pass_field: String,
        success_str: Option<String>,
        fail_str: Option<String>,
    },
    Json {
        user_field: String,
        pass_field: String,
        success_str: Option<String>,
        fail_str: Option<String>,
    },
}

pub struct HttpTarget {
    client: Client,
    url: Url,
    method: HttpMethod,
    mode: HttpMode,
}

impl HttpTarget {
    pub fn new(url: &str, method: HttpMethod, mode: HttpMode) -> Result<Self, BruteError> {
        let parsed_url = Url::parse(url)
            .map_err(|e| BruteError::Connection(format!("Invalid URL: {}", e)))?;
        
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(true) // Common for security testing
            .build()
            .map_err(|e| BruteError::Connection(format!("Failed to build client: {}", e)))?;

        Ok(Self {
            client,
            url: parsed_url,
            method,
            mode,
        })
    }
}

#[async_trait]
impl BruteTarget for HttpTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        match &self.mode {
            HttpMode::Basic => {
                let res = self.client.get(self.url.clone())
                    .basic_auth(&credentials.username, Some(&credentials.password))
                    .send()
                    .await
                    .map_err(|e| BruteError::Connection(e.to_string()))?;
                
                if res.status().is_success() {
                    Ok(AttemptResult::Success(credentials.clone()))
                } else if res.status() == StatusCode::UNAUTHORIZED {
                    Ok(AttemptResult::Failure)
                } else {
                    Ok(AttemptResult::Blocked(format!("HTTP status: {}", res.status())))
                }
            }
            HttpMode::Form { user_field, pass_field, success_str, fail_str } => {
                let params = [
                    (user_field.as_str(), credentials.username.as_str()),
                    (pass_field.as_str(), credentials.password.as_str()),
                ];
                
                let req = match self.method {
                    HttpMethod::Get => self.client.get(self.url.clone()).query(&params),
                    HttpMethod::Post => self.client.post(self.url.clone()).form(&params),
                };

                let res = req.send()
                    .await
                    .map_err(|e| BruteError::Connection(e.to_string()))?;

                let status = res.status();
                let body = res.text().await
                    .map_err(|e| BruteError::Protocol(format!("Failed to read body: {}", e)))?;

                // Check indicators
                if let Some(ref s) = success_str {
                    if body.contains(s) {
                        return Ok(AttemptResult::Success(credentials.clone()));
                    }
                }
                if let Some(ref f) = fail_str {
                    if body.contains(f) {
                        return Ok(AttemptResult::Failure);
                    }
                }

                // Default check based on status code if no strings specified
                if success_str.is_none() && fail_str.is_none() {
                    if status.is_success() {
                        Ok(AttemptResult::Success(credentials.clone()))
                    } else {
                        Ok(AttemptResult::Failure)
                    }
                } else {
                    // If strings specified but none matched, let's assume Failure
                    Ok(AttemptResult::Failure)
                }
            }
            HttpMode::Json { user_field, pass_field, success_str, fail_str } => {
                let payload = serde_json::json!({
                    user_field: credentials.username,
                    pass_field: credentials.password,
                });

                let req = match self.method {
                    HttpMethod::Get => {
                        return Err(BruteError::Protocol("GET method is not supported for JSON login mode".to_string()));
                    }
                    HttpMethod::Post => self.client.post(self.url.clone()).json(&payload),
                };

                let res = req.send()
                    .await
                    .map_err(|e| BruteError::Connection(e.to_string()))?;

                let status = res.status();
                let body = res.text().await
                    .map_err(|e| BruteError::Protocol(format!("Failed to read body: {}", e)))?;

                if let Some(ref s) = success_str {
                    if body.contains(s) {
                        return Ok(AttemptResult::Success(credentials.clone()));
                    }
                }
                if let Some(ref f) = fail_str {
                    if body.contains(f) {
                        return Ok(AttemptResult::Failure);
                    }
                }

                if success_str.is_none() && fail_str.is_none() {
                    if status.is_success() {
                        Ok(AttemptResult::Success(credentials.clone()))
                    } else {
                        Ok(AttemptResult::Failure)
                    }
                } else {
                    Ok(AttemptResult::Failure)
                }
            }
        }
    }
}
