// ============================================================================
// [PROVENANCE: GaloyMoney/cala Error Architecture]
// [CONVENTION: Idiomatic Rust Error Handling via `thiserror`]
// ============================================================================

use rust_decimal::Decimal;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum SuperCalaError {
    #[error("PostgreSQL database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Unbalanced double-entry transaction '{tx_id}': total debits ({total_debits}) != total credits ({total_credits})")]
    UnbalancedTransaction {
        tx_id: Uuid,
        total_debits: Decimal,
        total_credits: Decimal,
    },

    #[error("Invalid entry units for account '{account_id}': units must be positive (got {units})")]
    InvalidEntryUnits {
        account_id: Uuid,
        units: Decimal,
    },

    #[error("Empty transaction '{tx_id}': transaction must contain at least one balanced entry pair")]
    EmptyTransaction {
        tx_id: Uuid,
    },

    #[allow(dead_code)]
    #[error("Insufficient funds in account '{account_id}': required {required}, available {available}")]
    InsufficientBalance {
        account_id: Uuid,
        required: Decimal,
        available: Decimal,
    },

    #[error("Account balance snapshot for account '{0}' not found")]
    AccountNotFound(Uuid),

    #[allow(dead_code)]
    #[error("Optimistic Concurrency Control (OCC) collision on entity '{entity_id}' at sequence {attempted_seq}")]
    OccCollision {
        entity_id: Uuid,
        attempted_seq: i32,
    },

    #[error("PostgreSQL lock wait timeout (SQLSTATE 55P03) on entity '{0}'")]
    LockTimeout(Uuid),

    #[error("Deadlock detected during multi-account transfer (SQLSTATE 40P01): {0}")]
    DeadlockDetected(String),

    #[error("Serialization failure (SQLSTATE 40001): {0}")]
    SerializationFailure(String),
}

impl SuperCalaError {
    pub fn from_sqlx(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db_err) = err {
            match db_err.code().as_deref() {
                Some("40P01") => SuperCalaError::DeadlockDetected(db_err.message().to_string()),
                Some("55P03") => SuperCalaError::LockTimeout(Uuid::nil()),
                Some("40001") => SuperCalaError::SerializationFailure(db_err.message().to_string()),
                _ => SuperCalaError::Sqlx(err),
            }
        } else {
            SuperCalaError::Sqlx(err)
        }
    }
}
