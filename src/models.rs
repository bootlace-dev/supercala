// ============================================================================
// [PROVENANCE: GaloyMoney/cala Upstream Data Types & Models]
// [PROVENANCE: The Art of PostgreSQL Ch. 3 & 15: Strong Typing & Domain Models]
// ============================================================================

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "UPPERCASE")]
pub enum EntryType {
    Debit,
    Credit,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "UPPERCASE")]
pub enum BalanceType {
    Debit,
    Credit,
}

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub normal_balance_type: String,
    pub status: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Balance {
    pub account_id: Uuid,
    pub currency: String,
    pub layer: String,
    pub settled_debits: Decimal,
    pub settled_credits: Decimal,
    pub pending_debits: Decimal,
    pub pending_credits: Decimal,
    pub encumbered_debits: Decimal,
    pub encumbered_credits: Decimal,
    pub version: i32,
    pub modified_at: DateTime<Utc>,
}

impl Balance {
    // Net settled balance calculation derived mathematically
    #[allow(dead_code)]
    pub fn net_settled_balance(&self, normal_type: BalanceType) -> Decimal {
        match normal_type {
            BalanceType::Debit => self.settled_debits - self.settled_credits,
            BalanceType::Credit => self.settled_credits - self.settled_debits,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEntry {
    pub account_id: Uuid,
    pub entry_type: EntryType,
    pub units: Decimal,
    pub currency: String,
    pub layer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTransactionParams {
    pub journal_id: Uuid,
    pub correlation_id: Option<String>,
    pub external_id: Option<String>,
    pub description: Option<String>,
    pub entries: Vec<NewEntry>,
}
