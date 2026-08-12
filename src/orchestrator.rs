use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::credentials::{CredentialSource, Credentials};
use crate::error::BruteError;
use crate::protocols::{AttemptResult, BruteTarget};

pub struct OrchestratorConfig {
    pub concurrency: usize,
    pub rate_limit_per_sec: Option<usize>,
    pub stop_on_success: bool,
}

pub struct Orchestrator {
    config: OrchestratorConfig,
    target: Arc<dyn BruteTarget>,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub total_attempts: usize,
    pub successes: Vec<Credentials>,
    pub failures: usize,
    pub blocked: usize,
}

impl Orchestrator {
    pub fn new(config: OrchestratorConfig, target: Arc<dyn BruteTarget>) -> Self {
        Self { config, target }
    }

    pub async fn run(&self, source: CredentialSource) -> Result<Report, BruteError> {
        let (tx, mut rx) = mpsc::channel::<AttemptResult>(self.config.concurrency * 2);
        
        let successes = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let failures = Arc::new(AtomicUsize::new(0));
        let blocked = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));

        let stop_flag = Arc::new(AtomicBool::new(false));

        // Spawn a logger/orchestrator manager that processes results
        let successes_clone = Arc::clone(&successes);
        let stop_flag_clone = Arc::clone(&stop_flag);
        
        let manager_handle = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    AttemptResult::Success(creds) => {
                        info!("SUCCESS: Found valid credentials: user={}, pass={}", creds.username, creds.password);
                        let mut guard = successes_clone.lock().await;
                        guard.push(creds);
                        stop_flag_clone.store(true, Ordering::Relaxed);
                    }
                    AttemptResult::Failure => {
                        // Normally we don't log every single failure unless verbosity is high
                    }
                    AttemptResult::Blocked(reason) => {
                        warn!("BLOCKED: {}", reason);
                    }
                }
            }
        });

        // Distribute rate limit dynamically among workers
        let delay_between_attempts = self.config.rate_limit_per_sec.map(|limit| {
            Duration::from_secs_f64((self.config.concurrency as f64) / limit as f64)
        });

        let source_iter = Arc::new(std::sync::Mutex::new(source));
        let mut workers = Vec::new();

        for _ in 0..self.config.concurrency {
            let source_iter = Arc::clone(&source_iter);
            let target = Arc::clone(&self.target);
            let tx_clone = tx.clone();
            let failures_clone = Arc::clone(&failures);
            let blocked_clone = Arc::clone(&blocked);
            let total_clone = Arc::clone(&total);
            let stop_flag_clone = Arc::clone(&stop_flag);
            let stop_on_success = self.config.stop_on_success;
            let delay = delay_between_attempts;

            let worker = tokio::spawn(async move {
                loop {
                    if stop_on_success && stop_flag_clone.load(Ordering::Relaxed) {
                        break;
                    }

                    let credentials = {
                        let mut guard = source_iter.lock().unwrap();
                        guard.next()
                    };

                    let credentials = match credentials {
                        Some(c) => c,
                        None => break,
                    };

                    total_clone.fetch_add(1, Ordering::Relaxed);
                    match target.attempt(&credentials).await {
                        Ok(AttemptResult::Success(creds)) => {
                            let _ = tx_clone.send(AttemptResult::Success(creds)).await;
                        }
                        Ok(AttemptResult::Failure) => {
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                            let _ = tx_clone.send(AttemptResult::Failure).await;
                        }
                        Ok(AttemptResult::Blocked(reason)) => {
                            blocked_clone.fetch_add(1, Ordering::Relaxed);
                            let _ = tx_clone.send(AttemptResult::Blocked(reason)).await;
                        }
                        Err(err) => {
                            error!("Error during attempt for {}: {:?}", credentials.username, err);
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                            let _ = tx_clone.send(AttemptResult::Failure).await;
                        }
                    }

                    if let Some(d) = delay {
                        sleep(d).await;
                    }
                }
            });
            workers.push(worker);
        }

        // Close sender channel so manager knows no more results are coming
        drop(tx);

        // Wait for all workers to finish
        for worker in workers {
            let _ = worker.await;
        }

        // Wait for manager to finish logging everything
        let _ = manager_handle.await;

        let final_successes = successes.lock().await.clone();
        Ok(Report {
            total_attempts: total.load(Ordering::Relaxed),
            successes: final_successes,
            failures: failures.load(Ordering::Relaxed),
            blocked: blocked.load(Ordering::Relaxed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockTarget {
        success_user: String,
        success_pass: String,
    }

    #[async_trait]
    impl BruteTarget for MockTarget {
        async fn attempt(&self, credentials: &Credentials) -> Result<AttemptResult, BruteError> {
            if credentials.username == self.success_user && credentials.password == self.success_pass {
                Ok(AttemptResult::Success(credentials.clone()))
            } else {
                Ok(AttemptResult::Failure)
            }
        }
    }

    #[tokio::test]
    async fn test_orchestrator_run() {
        let target = Arc::new(MockTarget {
            success_user: "admin".to_string(),
            success_pass: "secret".to_string(),
        });
        
        let config = OrchestratorConfig {
            concurrency: 2,
            rate_limit_per_sec: None,
            stop_on_success: true,
        };

        let orchestrator = Orchestrator::new(config, target);
        let source = CredentialSource::from_lists(
            vec!["user1".to_string(), "admin".to_string(), "user2".to_string(), "user3".to_string(), "user4".to_string()],
            vec!["wrong1".to_string(), "wrong2".to_string(), "secret".to_string(), "wrong3".to_string()],
            crate::credentials::WordlistMode::Cartesian,
        );

        let report = orchestrator.run(source).await.unwrap();
        assert_eq!(report.successes.len(), 1);
        assert_eq!(report.successes[0].username, "admin");
        assert_eq!(report.successes[0].password, "secret");

        // With stop_on_success: true, total attempts must be limited and not run all 20 Cartesian combinations
        assert!(report.total_attempts < 20);
    }
}
