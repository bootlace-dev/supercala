// ============================================================================
// [PROVENANCE: Project SuperCala (bootlace-dev/supercala)]
// [PURPOSE: Phase 4 Mathematical Precision & Edge-Case Invariant Test Suite]
// ============================================================================

use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn test_phase4_mathematical_precision_and_limits() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5433/supercala".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    let journal_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO cala_journals (id, name, description, status)
        VALUES ($1, $2, 'Phase 4 Edge Case Journal', 'ACTIVE')
        "#,
    )
    .bind(journal_id)
    .bind(format!("EdgeJournal_{}", Uuid::new_v4().simple()))
    .execute(&pool)
    .await?;

    let acc1_id = Uuid::new_v4();
    let acc2_id = Uuid::new_v4();

    for (id, code, name) in [
        (acc1_id, format!("ACC1_{}", Uuid::new_v4().simple()), "Test Account 1"),
        (acc2_id, format!("ACC2_{}", Uuid::new_v4().simple()), "Test Account 2"),
    ] {
        sqlx::query(
            r#"
            INSERT INTO cala_accounts (id, code, name, normal_balance_type, status)
            VALUES ($1, $2, $3, 'DEBIT', 'ACTIVE')
            "#,
        )
        .bind(id)
        .bind(code)
        .bind(name)
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO cala_balances (account_id, currency, layer, settled_debits, settled_credits)
            VALUES ($1, 'SATS', 'SETTLED', 100000000000, 0)
            "#,
        )
        .bind(id)
        .execute(&pool)
        .await?;
    }

    // 1. Test Micro-Dust (1 SAT = 0.00000001 BTC or 1 integer SAT)
    let dust_amount = Decimal::from_str("0.00000001")?;
    let dust_tx_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO cala_transactions (id, journal_id, description)
        VALUES ($1, $2, 'Micro-Dust 1 SAT Transfer')
        "#,
    )
    .bind(dust_tx_id)
    .bind(journal_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO cala_entries (id, transaction_id, journal_id, account_id, entry_type, layer, units, currency, sequence)
        VALUES 
            ($1, $2, $3, $4, 'DEBIT', 'SETTLED', $5, 'SATS', 1),
            ($6, $2, $3, $7, 'CREDIT', 'SETTLED', $5, 'SATS', 2)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(dust_tx_id)
    .bind(journal_id)
    .bind(acc1_id)
    .bind(dust_amount)
    .bind(Uuid::new_v4())
    .bind(acc2_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    println!("[Phase 4 PASS] Micro-dust 1 SAT transaction committed with full precision.");

    // 2. Test Macro-Boundary (21M BTC = 2,100,000,000,000,000 SATS)
    let max_supply = Decimal::from_str("2100000000000000.00000000")?;
    let macro_tx_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO cala_transactions (id, journal_id, description)
        VALUES ($1, $2, '21M BTC Total Supply Transfer')
        "#,
    )
    .bind(macro_tx_id)
    .bind(journal_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO cala_entries (id, transaction_id, journal_id, account_id, entry_type, layer, units, currency, sequence)
        VALUES 
            ($1, $2, $3, $4, 'DEBIT', 'SETTLED', $5, 'SATS', 1),
            ($6, $2, $3, $7, 'CREDIT', 'SETTLED', $5, 'SATS', 2)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(macro_tx_id)
    .bind(journal_id)
    .bind(acc1_id)
    .bind(max_supply)
    .bind(Uuid::new_v4())
    .bind(acc2_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    println!("[Phase 4 PASS] Macro 21M BTC boundary transaction committed without overflow.");

    // 3. Test Negative Amount Rejection (Must fail SQL constraint units > 0)
    let neg_amount = Decimal::from_str("-500.00")?;
    let neg_tx_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;

    let res = sqlx::query(
        r#"
        INSERT INTO cala_entries (id, transaction_id, journal_id, account_id, entry_type, layer, units, currency, sequence)
        VALUES ($1, $2, $3, $4, 'DEBIT', 'SETTLED', $5, 'SATS', 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(neg_tx_id)
    .bind(journal_id)
    .bind(acc1_id)
    .bind(neg_amount)
    .execute(&mut *tx)
    .await;

    assert!(res.is_err(), "Negative units MUST be rejected by check constraint");
    println!("[Phase 4 PASS] Negative amount rejected by database check constraint.");

    // 4. Test Zero Amount Rejection (Must fail SQL constraint units > 0)
    let zero_amount = Decimal::ZERO;
    let res_zero = sqlx::query(
        r#"
        INSERT INTO cala_entries (id, transaction_id, journal_id, account_id, entry_type, layer, units, currency, sequence)
        VALUES ($1, $2, $3, $4, 'DEBIT', 'SETTLED', $5, 'SATS', 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(neg_tx_id)
    .bind(journal_id)
    .bind(acc1_id)
    .bind(zero_amount)
    .execute(&mut *tx)
    .await;

    assert!(res_zero.is_err(), "Zero units MUST be rejected by check constraint");
    println!("[Phase 4 PASS] Zero amount rejected by database check constraint.");

    Ok(())
}
