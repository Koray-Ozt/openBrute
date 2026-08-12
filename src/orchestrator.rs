use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
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
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
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

        // Calculate rate limiting delay if configured
        let delay_between_attempts = self.config.rate_limit_per_sec.map(|limit| {
            Duration::from_secs_f64(1.0 / limit as f64)
        });

        for credentials in source {
            // Check if we should stop on success
            if self.config.stop_on_success && stop_flag.load(Ordering::Relaxed) {
                break;
            }

            // Acquire concurrency permit
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let target = Arc::clone(&self.target);
            let tx_clone = tx.clone();
            let failures_clone = Arc::clone(&failures);
            let blocked_clone = Arc::clone(&blocked);
            let total_clone = Arc::clone(&total);

            tokio::spawn(async move {
                // Keep permit alive during attempt
                let _permit = permit;
                
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
            });

            if let Some(delay) = delay_between_attempts {
                sleep(delay).await;
            }
        }

        // Close sender channel so manager knows no more results are coming
        drop(tx);

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
