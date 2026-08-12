use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use axum::{
    routing::{get, post},
    Router,
    response::{Html, IntoResponse, sse::{Event, KeepAlive, Sse}},
    Json,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::credentials::{CredentialSource, Credentials, WordlistMode};
use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::protocols::http::{HttpTarget, HttpMode as TargetHttpMode, HttpMethod as TargetHttpMethod};
use crate::protocols::ssh::SshTarget;
use crate::protocols::ftp::FtpTarget;
use crate::protocols::smtp::SmtpTarget;
use crate::protocols::sql::SqlTarget;
use crate::protocols::{AttemptResult, BruteTarget};
use crate::error::BruteError;

#[derive(Clone, Serialize, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum WebEvent {
    #[serde(rename = "log")]
    Log { message: String },
    #[serde(rename = "stats")]
    Stats { total: usize, success: usize, blocked: usize },
    #[serde(rename = "finish")]
    Finish,
}

static LOG_SENDER: OnceLock<broadcast::Sender<WebEvent>> = OnceLock::new();

fn get_log_sender() -> &'static broadcast::Sender<WebEvent> {
    LOG_SENDER.get_or_init(|| {
        let (tx, _) = broadcast::channel(100);
        tx
    })
}

pub fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/start", post(start_attack))
        .route("/api/stream", get(stream_logs))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct WebConfig {
    pub protocol: String,
    pub target: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub usernames_list: Option<String>,
    pub passwords_list: Option<String>,
    pub combos_list: Option<String>,
    pub input_mode: String,
    pub concurrency: usize,
    pub rate_limit: Option<usize>,
    pub http_mode: Option<String>,
    pub http_method: Option<String>,
    pub user_field: Option<String>,
    pub pass_field: Option<String>,
    pub success_str: Option<String>,
    pub fail_str: Option<String>,
}

struct BroadcastTarget {
    inner: Arc<dyn BruteTarget>,
    tx: broadcast::Sender<WebEvent>,
    total: Arc<AtomicUsize>,
    success: Arc<AtomicUsize>,
    blocked: Arc<AtomicUsize>,
}

#[async_trait]
impl BruteTarget for BroadcastTarget {
    async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
        let res = self.inner.attempt(credentials).await;
        
        let total = self.total.fetch_add(1, Ordering::Relaxed) + 1;
        let mut success = self.success.load(Ordering::Relaxed);
        let mut blocked = self.blocked.load(Ordering::Relaxed);

        match &res {
            Ok(AttemptResult::Success(_)) => {
                success = self.success.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = self.tx.send(WebEvent::Log {
                    message: format!("SUCCESS: Found valid credentials: user={}, pass={}", credentials.username, credentials.password),
                });
            }
            Ok(AttemptResult::Failure) => {
                let _ = self.tx.send(WebEvent::Log {
                    message: format!("FAILURE: Attempt failed: user={}, pass={}", credentials.username, credentials.password),
                });
            }
            Ok(AttemptResult::Blocked(reason)) => {
                blocked = self.blocked.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = self.tx.send(WebEvent::Log {
                    message: format!("BLOCKED: user={}, pass={}. Reason: {}", credentials.username, credentials.password, reason),
                });
            }
            Err(err) => {
                let _ = self.tx.send(WebEvent::Log {
                    message: format!("Error during attempt for {}: {:?}", credentials.username, err),
                });
            }
        }

        let _ = self.tx.send(WebEvent::Stats { total, success, blocked });
        res
    }
}

async fn start_attack(Json(payload): Json<WebConfig>) -> impl IntoResponse {
    let tx = get_log_sender().clone();
    
    let source = match payload.input_mode.as_str() {
        "manual" => {
            let u = payload.username.clone().unwrap_or_default();
            let p = payload.password.clone().unwrap_or_default();
            CredentialSource::from_lists(vec![u], vec![p], WordlistMode::Cartesian)
        }
        "lists" => {
            let u_content = payload.usernames_list.clone().unwrap_or_default();
            let p_content = payload.passwords_list.clone().unwrap_or_default();
            let usernames = parse_lines(&u_content);
            let passwords = parse_lines(&p_content);
            CredentialSource::from_lists(usernames, passwords, WordlistMode::Cartesian)
        }
        "combo" => {
            let c_content = payload.combos_list.clone().unwrap_or_default();
            let combos = parse_combos(&c_content);
            CredentialSource::from_combos(combos)
        }
        _ => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };

    let inner_target: Arc<dyn BruteTarget> = match payload.protocol.as_str() {
        "http" => {
            let http_method = match payload.http_method.as_deref() {
                Some("get") => TargetHttpMethod::Get,
                _ => TargetHttpMethod::Post,
            };
            let http_mode = match payload.http_mode.as_deref() {
                Some("form") => TargetHttpMode::Form {
                    user_field: payload.user_field.clone().unwrap_or_else(|| "username".to_string()),
                    pass_field: payload.pass_field.clone().unwrap_or_else(|| "password".to_string()),
                    success_str: payload.success_str.clone(),
                    fail_str: payload.fail_str.clone(),
                },
                Some("json") => TargetHttpMode::Json {
                    user_field: payload.user_field.clone().unwrap_or_else(|| "username".to_string()),
                    pass_field: payload.pass_field.clone().unwrap_or_else(|| "password".to_string()),
                    success_str: payload.success_str.clone(),
                    fail_str: payload.fail_str.clone(),
                },
                _ => TargetHttpMode::Basic,
            };
            match HttpTarget::new(&payload.target, http_method, http_mode) {
                Ok(t) => Arc::new(t),
                Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }
        "ssh" => match SshTarget::new(&payload.target) {
            Ok(t) => Arc::new(t),
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        "ftp" => match FtpTarget::new(&payload.target) {
            Ok(t) => Arc::new(t),
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        "smtp" => match SmtpTarget::new(&payload.target) {
            Ok(t) => Arc::new(t),
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        "sql" => match SqlTarget::new(&payload.target) {
            Ok(t) => Arc::new(t),
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        _ => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };

    let broadcast_target = Arc::new(BroadcastTarget {
        inner: inner_target,
        tx: tx.clone(),
        total: Arc::new(AtomicUsize::new(0)),
        success: Arc::new(AtomicUsize::new(0)),
        blocked: Arc::new(AtomicUsize::new(0)),
    });

    let config = OrchestratorConfig {
        concurrency: payload.concurrency,
        rate_limit_per_sec: payload.rate_limit,
        stop_on_success: true,
    };

    let orchestrator = Orchestrator::new(config, broadcast_target);

    tokio::spawn(async move {
        let _ = tx.send(WebEvent::Log {
            message: format!("Web Attack initialized against: {}", payload.target),
        });
        
        let _ = orchestrator.run(source).await;
        
        let _ = tx.send(WebEvent::Finish);
    });

    axum::http::StatusCode::OK.into_response()
}

async fn stream_logs() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = get_log_sender().subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|res| res.ok())
        .map(|event| {
            let json = serde_json::to_string(&event).unwrap();
            Event::default().data(json)
        })
        .map(Ok);

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn parse_lines(content: &str) -> Vec<String> {
    content.lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect()
}

fn parse_combos(content: &str) -> Vec<Credentials> {
    let mut result = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(pos) = trimmed.find(':') {
            let username = trimmed[..pos].trim().to_string();
            let password = trimmed[pos + 1..].trim().to_string();
            result.push(Credentials { username, password });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_web_routes() {
        let app = app();

        let response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let payload = WebConfig {
            protocol: "http".to_string(),
            target: "http://example.com".to_string(),
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            usernames_list: None,
            passwords_list: None,
            combos_list: None,
            input_mode: "manual".to_string(),
            concurrency: 5,
            rate_limit: None,
            http_mode: None,
            http_method: None,
            user_field: None,
            pass_field: None,
            success_str: None,
            fail_str: None,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/start")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
