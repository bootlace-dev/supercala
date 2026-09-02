// ============================================================================
// [PROVENANCE: tbintdb / One-Hoss Shay Limit Testing Lineage]
// [PROVENANCE: GaloyMoney/cala Concurrency Stress Testing]
// [INNOVATION: Decorrelated Jitter Retry & Tri-Pool Telemetry Isolation]
// ============================================================================

use crate::error::SuperCalaError;
use crate::ledger::Ledger;
use crate::models::{EntryType, NewEntry, NewTransactionParams};
use rand::Rng;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};
use uuid::Uuid;

pub struct FuzzerConfig {
    pub num_workers: usize,
    pub tx_per_worker: usize,
    pub enable_jitter: bool,
    pub chaos_random_locks: bool,
    pub max_retries: usize,
}

pub struct FuzzerMetrics {
    pub total_attempted: AtomicU64,
    pub total_committed: AtomicU64,
    pub total_retries: AtomicU64,
    pub total_deadlocks: AtomicU64,
    pub total_lock_timeouts: AtomicU64,
    pub total_occ_conflicts: AtomicU64,
}

impl FuzzerMetrics {
    pub fn new() -> Self {
        Self {
            total_attempted: AtomicU64::new(0),
            total_committed: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
            total_deadlocks: AtomicU64::new(0),
            total_lock_timeouts: AtomicU64::new(0),
            total_occ_conflicts: AtomicU64::new(0),
        }
    }
}

pub struct Fuzzer {
    ledger: Arc<Ledger>,
    config: FuzzerConfig,
    metrics: Arc<FuzzerMetrics>,
}

impl Fuzzer {
    pub fn new(pool: PgPool, config: FuzzerConfig) -> Self {
        let chaos = config.chaos_random_locks;
        Self {
            ledger: Arc::new(Ledger::new(pool, chaos)),
            config,
            metrics: Arc::new(FuzzerMetrics::new()),
        }
    }

    pub async fn run_merchant_rush(
        &self,
        journal_id: Uuid,
        merchant_account_id: Uuid,
        payer_account_ids: Vec<Uuid>,
    ) -> Result<Duration, SuperCalaError> {
        info!(
            workers = self.config.num_workers,
            tx_per_worker = self.config.tx_per_worker,
            jitter = self.config.enable_jitter,
            chaos_random_locks = self.config.chaos_random_locks,
            "Starting SuperCala Merchant Rush Concurrency Stress Test"
        );

        let start_time = Instant::now();
        let mut handles = Vec::with_capacity(self.config.num_workers);

        for worker_id in 0..self.config.num_workers {
            let ledger = Arc::clone(&self.ledger);
            let metrics = Arc::clone(&self.metrics);
            let payers = payer_account_ids.clone();
            let config_jitter = self.config.enable_jitter;
            let max_retries = self.config.max_retries;
            let tx_count = self.config.tx_per_worker;

            let handle = tokio::spawn(async move {
                for _ in 0..tx_count {
                    metrics.total_attempted.fetch_add(1, Ordering::Relaxed);

                    let (payer_id, amount) = {
                        let mut rng = rand::thread_rng();
                        let p = payers[rng.gen_range(0..payers.len())];
                        let a = Decimal::new(rng.gen_range(100..5000), 0);
                        (p, a)
                    };

                    let params = NewTransactionParams {
                        journal_id,
                        correlation_id: Some(format!("worker-{worker_id}")),
                        external_id: Some(Uuid::new_v4().to_string()),
                        description: Some("Merchant POS settlement".to_string()),
                        entries: vec![
                            NewEntry {
                                account_id: payer_id,
                                entry_type: EntryType::Debit,
                                units: amount,
                                currency: "BTC_SATS".to_string(),
                                layer: "SETTLED".to_string(),
                            },
                            NewEntry {
                                account_id: merchant_account_id,
                                entry_type: EntryType::Credit,
                                units: amount,
                                currency: "BTC_SATS".to_string(),
                                layer: "SETTLED".to_string(),
                            },
                        ],
                    };

                    let mut retries = 0;
                    let mut base_backoff_ms = 10u64;

                    loop {
                        match ledger.post_transaction(params.clone()).await {
                            Ok(_) => {
                                metrics.total_committed.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                            Err(e) => {
                                retries += 1;
                                metrics.total_retries.fetch_add(1, Ordering::Relaxed);

                                match &e {
                                    SuperCalaError::DeadlockDetected(_) => {
                                        metrics.total_deadlocks.fetch_add(1, Ordering::Relaxed);
                                    }
                                    SuperCalaError::LockTimeout(_) => {
                                        metrics.total_lock_timeouts.fetch_add(1, Ordering::Relaxed);
                                    }
                                    SuperCalaError::SerializationFailure(_)
                                    | SuperCalaError::OccCollision { .. } => {
                                        metrics.total_occ_conflicts.fetch_add(1, Ordering::Relaxed);
                                    }
                                    SuperCalaError::Sqlx(sqlx::Error::Database(db_err)) => {
                                        match db_err.code().as_deref() {
                                            Some("40P01") => {
                                                metrics.total_deadlocks.fetch_add(1, Ordering::Relaxed);
                                            }
                                            Some("55P03") => {
                                                metrics.total_lock_timeouts.fetch_add(1, Ordering::Relaxed);
                                            }
                                            Some("40001") => {
                                                metrics.total_occ_conflicts.fetch_add(1, Ordering::Relaxed);
                                            }
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }

                                if retries > max_retries {
                                    error!(%worker_id, ?e, "Worker exceeded max retries");
                                    break;
                                }

                                if config_jitter {
                                    let sleep_ms = {
                                        let mut rng = rand::thread_rng();
                                        rng.gen_range(base_backoff_ms..=(base_backoff_ms * 3))
                                    };
                                    base_backoff_ms = (sleep_ms * 2).min(500);
                                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                                } else {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }

        let elapsed = start_time.elapsed();
        let committed = self.metrics.total_committed.load(Ordering::Relaxed);
        let retries = self.metrics.total_retries.load(Ordering::Relaxed);
        let deadlocks = self.metrics.total_deadlocks.load(Ordering::Relaxed);
        let lock_timeouts = self.metrics.total_lock_timeouts.load(Ordering::Relaxed);
        let occ_conflicts = self.metrics.total_occ_conflicts.load(Ordering::Relaxed);
        let tps = (committed as f64) / elapsed.as_secs_f64();

        info!(
            elapsed_sec = elapsed.as_secs_f64(),
            committed = committed,
            retries = retries,
            deadlocks = deadlocks,
            lock_timeouts = lock_timeouts,
            occ_conflicts = occ_conflicts,
            tps = tps,
            "Merchant Rush Completed"
        );

        Ok(elapsed)
    }
}
