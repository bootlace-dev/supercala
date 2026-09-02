// ============================================================================
// [PROVENANCE: The Art of PostgreSQL Ch. 16: Concurrency Control & Deadlock Elimination]
// [PROVENANCE: GaloyMoney/cala Double-Entry Engine & Transactional Outbox]
// [INVARIANT: Double-Entry Balance Conservation sum(debits) == sum(credits)]
// ============================================================================

use crate::error::SuperCalaError;
use crate::models::{Balance, EntryType, NewTransactionParams};
use rand::seq::SliceRandom;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use tracing::{info, instrument};
use uuid::Uuid;

pub struct Ledger {
    pool: PgPool,
    chaos_random_locks: bool,
}

impl Ledger {
    pub fn new(pool: PgPool, chaos_random_locks: bool) -> Self {
        Self {
            pool,
            chaos_random_locks,
        }
    }

    #[instrument(skip(self, params), fields(journal_id = %params.journal_id))]
    pub async fn post_transaction(
        &self,
        params: NewTransactionParams,
    ) -> Result<Uuid, SuperCalaError> {
        let tx_id = Uuid::new_v4();

        // Step 1: Assert Mathematical Double-Entry Invariance & Positive Units
        if params.entries.is_empty() {
            return Err(SuperCalaError::EmptyTransaction { tx_id });
        }

        let mut totals_by_currency: HashMap<String, (Decimal, Decimal)> = HashMap::new();

        for entry in &params.entries {
            if entry.units <= Decimal::ZERO {
                return Err(SuperCalaError::InvalidEntryUnits {
                    account_id: entry.account_id,
                    units: entry.units,
                });
            }

            let (debits, credits) = totals_by_currency
                .entry(entry.currency.clone())
                .or_insert((Decimal::ZERO, Decimal::ZERO));

            match entry.entry_type {
                EntryType::Debit => *debits += entry.units,
                EntryType::Credit => *credits += entry.units,
            }
        }

        for (_currency, (total_debits, total_credits)) in totals_by_currency {
            if total_debits != total_credits {
                return Err(SuperCalaError::UnbalancedTransaction {
                    tx_id,
                    total_debits,
                    total_credits,
                });
            }
        }

        // Step 2: Begin ACID PostgreSQL Transaction
        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await.map_err(SuperCalaError::from_sqlx)?;

        // Step 3: [PROVENANCE: Art of PG Ch. 16] Lock Ordering Invariant
        let mut unique_account_ids: Vec<Uuid> = params
            .entries
            .iter()
            .map(|e| e.account_id)
            .collect();
        unique_account_ids.sort();
        unique_account_ids.dedup();

        if self.chaos_random_locks {
            // [CHAOS NEGATIVE PROOF]: Randomize lock order to deliberately induce lock-graph cycles
            {
                let mut rng = rand::thread_rng();
                unique_account_ids.shuffle(&mut rng);
            }

            for account_id in &unique_account_ids {
                let _ = sqlx::query_as::<_, Balance>(
                    r#"
                    SELECT account_id, currency, layer, settled_debits, settled_credits,
                           pending_debits, pending_credits, encumbered_debits, encumbered_credits,
                           version, modified_at
                    FROM cala_balances
                    WHERE account_id = $1
                    FOR UPDATE
                    "#,
                )
                .bind(account_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(SuperCalaError::from_sqlx)?;
            }
        } else {
            // [STANDARD INVARIANT]: Deterministic lexicographical ascending sort order
            let _locked_balances = sqlx::query_as::<_, Balance>(
                r#"
                SELECT account_id, currency, layer, settled_debits, settled_credits,
                       pending_debits, pending_credits, encumbered_debits, encumbered_credits,
                       version, modified_at
                FROM cala_balances
                WHERE account_id = ANY($1)
                ORDER BY account_id ASC, currency ASC, layer ASC
                FOR UPDATE
                "#,
            )
            .bind(&unique_account_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(SuperCalaError::from_sqlx)?;
        }

        // Step 4: Record Transaction Header
        sqlx::query(
            r#"
            INSERT INTO cala_transactions (id, journal_id, correlation_id, external_id, description)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(tx_id)
        .bind(params.journal_id)
        .bind(&params.correlation_id)
        .bind(&params.external_id)
        .bind(&params.description)
        .execute(&mut *tx)
        .await
        .map_err(SuperCalaError::from_sqlx)?;

        // Step 5: Append Immutable Entries & Update Balances
        for (idx, entry) in params.entries.iter().enumerate() {
            let entry_id = Uuid::new_v4();
            let entry_type_str = match entry.entry_type {
                EntryType::Debit => "DEBIT",
                EntryType::Credit => "CREDIT",
            };

            // Insert into immutable entries table
            sqlx::query(
                r#"
                INSERT INTO cala_entries (id, transaction_id, journal_id, account_id, entry_type, layer, units, currency, sequence)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(entry_id)
            .bind(tx_id)
            .bind(params.journal_id)
            .bind(entry.account_id)
            .bind(entry_type_str)
            .bind(&entry.layer)
            .bind(entry.units)
            .bind(&entry.currency)
            .bind(idx as i32 + 1)
            .execute(&mut *tx)
            .await
            .map_err(SuperCalaError::from_sqlx)?;

            // Update materialized balance snapshot
            let update_result = match entry.entry_type {
                EntryType::Debit => {
                    sqlx::query(
                        r#"
                        UPDATE cala_balances
                        SET settled_debits = settled_debits + $1,
                            version = version + 1,
                            modified_at = NOW()
                        WHERE account_id = $2 AND currency = $3 AND layer = $4
                        "#,
                    )
                    .bind(entry.units)
                    .bind(entry.account_id)
                    .bind(&entry.currency)
                    .bind(&entry.layer)
                    .execute(&mut *tx)
                    .await
                    .map_err(SuperCalaError::from_sqlx)?
                }
                EntryType::Credit => {
                    sqlx::query(
                        r#"
                        UPDATE cala_balances
                        SET settled_credits = settled_credits + $1,
                            version = version + 1,
                            modified_at = NOW()
                        WHERE account_id = $2 AND currency = $3 AND layer = $4
                        "#,
                    )
                    .bind(entry.units)
                    .bind(entry.account_id)
                    .bind(&entry.currency)
                    .bind(&entry.layer)
                    .execute(&mut *tx)
                    .await
                    .map_err(SuperCalaError::from_sqlx)?
                }
            };

            if update_result.rows_affected() == 0 {
                return Err(SuperCalaError::AccountNotFound(entry.account_id));
            }
        }

        // Step 6: Atomic Outbox Event Persistence
        let outbox_payload = serde_json::json!({
            "transaction_id": tx_id,
            "journal_id": params.journal_id,
            "entry_count": params.entries.len(),
            "recorded_at": chrono::Utc::now()
        });

        sqlx::query(
            r#"
            INSERT INTO cala_outbox (sequence, event_type, payload, status)
            VALUES (nextval('cala_outbox_sequence_seq'), 'TRANSACTION_RECORDED', $1, 'PENDING')
            "#,
        )
        .bind(outbox_payload)
        .execute(&mut *tx)
        .await
        .map_err(SuperCalaError::from_sqlx)?;

        // Step 7: Commit ACID Transaction
        tx.commit().await.map_err(SuperCalaError::from_sqlx)?;

        info!(%tx_id, "Double-entry transaction posted successfully");
        Ok(tx_id)
    }
}
