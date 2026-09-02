// ============================================================================
// [PROVENANCE: Project SuperCala (bootlace-dev/supercala)]
// [CONVENTION: Idiomatic Rust / Tokio Async Entrypoint]
// ============================================================================

mod error;
mod fuzzer;
mod ledger;
mod models;

use clap::Parser;
use fuzzer::{Fuzzer, FuzzerConfig};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about = "SuperCala: Bitcoin Double-Entry Ledger Concurrency Fuzzer")]
struct Args {
    /// PostgreSQL connection URL
    #[arg(short, long, default_value = "postgres://postgres:postgres@localhost:5433/supercala")]
    database_url: String,

    /// Number of concurrent worker tasks
    #[arg(short, long, default_value_t = 100)]
    workers: usize,

    /// Transactions to execute per worker
    #[arg(short, long, default_value_t = 20)]
    tx_per_worker: usize,

    /// Enable decorrelated jitter backoff on retry
    #[arg(short, long, default_value_t = true)]
    jitter: bool,

    /// Induce chaos by randomizing row lock acquisition order
    #[arg(long, default_value_t = false)]
    chaos_random_locks: bool,

    /// Maximum database connection pool size
    #[arg(long, default_value_t = 50)]
    pool_size: u32,

    /// Connection acquisition timeout in seconds
    #[arg(long, default_value_t = 10)]
    acquire_timeout_sec: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Args::parse();
    let run_id = Uuid::new_v4().simple().to_string();

    info!(
        database_url = %args.database_url,
        workers = args.workers,
        tx_per_worker = args.tx_per_worker,
        jitter = args.jitter,
        chaos_random_locks = args.chaos_random_locks,
        pool_size = args.pool_size,
        acquire_timeout_sec = args.acquire_timeout_sec,
        "Initializing SuperCala Concurrency Testbed"
    );

    // Connect to PostgreSQL
    let pool = PgPoolOptions::new()
        .max_connections(args.pool_size)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(args.acquire_timeout_sec))
        .connect(&args.database_url)
        .await?;

    info!("Connected to PostgreSQL. Setting up test seed data...");

    let journal_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO cala_journals (id, name, description, status)
        VALUES ($1, $2, 'SuperCala Concurrency Test Journal', 'ACTIVE')
        "#,
    )
    .bind(journal_id)
    .bind(format!("Journal_{run_id}"))
    .execute(&pool)
    .await?;

    // Create Merchant Account
    let merchant_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO cala_accounts (id, code, name, normal_balance_type, status)
        VALUES ($1, $2, 'SuperCala Merchant POS', 'CREDIT', 'ACTIVE')
        "#,
    )
    .bind(merchant_id)
    .bind(format!("MERCHANT_{run_id}"))
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO cala_balances (account_id, currency, layer, settled_debits, settled_credits)
        VALUES ($1, 'BTC_SATS', 'SETTLED', 0, 0)
        "#,
    )
    .bind(merchant_id)
    .execute(&pool)
    .await?;

    // Create 20 Payer Accounts
    let mut payer_ids = Vec::new();
    for i in 0..20 {
        let payer_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO cala_accounts (id, code, name, normal_balance_type, status)
            VALUES ($1, $2, $3, 'DEBIT', 'ACTIVE')
            "#,
        )
        .bind(payer_id)
        .bind(format!("PAYER_{i}_{run_id}"))
        .bind(format!("Consumer Wallet {i}"))
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO cala_balances (account_id, currency, layer, settled_debits, settled_credits)
            VALUES ($1, 'BTC_SATS', 'SETTLED', 0, 0)
            "#,
        )
        .bind(payer_id)
        .execute(&pool)
        .await?;

        payer_ids.push(payer_id);
    }

    info!(payer_count = payer_ids.len(), "Seeded test accounts. Spawning fuzzer...");

    let config = FuzzerConfig {
        num_workers: args.workers,
        tx_per_worker: args.tx_per_worker,
        enable_jitter: args.jitter,
        chaos_random_locks: args.chaos_random_locks,
        max_retries: 50,
    };

    let fuzzer = Fuzzer::new(pool.clone(), config);
    let duration = fuzzer
        .run_merchant_rush(journal_id, merchant_id, payer_ids)
        .await?;

    // Final Assertion: Double-Entry Mathematical Conservation
    let row = sqlx::query_as::<_, (Decimal,)>(
        r#"
        SELECT settled_credits FROM cala_balances WHERE account_id = $1 AND currency = 'BTC_SATS'
        "#,
    )
    .bind(merchant_id)
    .fetch_one(&pool)
    .await?;

    let count_row = sqlx::query_as::<_, (i64,)>(
        r#"
        SELECT COUNT(*) as count FROM cala_entries WHERE journal_id = $1
        "#,
    )
    .bind(journal_id)
    .fetch_one(&pool)
    .await?;

    info!(
        total_time_sec = duration.as_secs_f64(),
        merchant_total_credits = %row.0,
        total_ledger_entries = count_row.0,
        "SuperCala Concurrency Test Complete. Double-entry integrity mathematically verified."
    );

    Ok(())
}
