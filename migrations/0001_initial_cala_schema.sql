-- ============================================================================
-- [PROVENANCE: GaloyMoney/cala Upstream DDL Architecture]
-- [PROVENANCE: The Art of PostgreSQL Ch. 3 & 15: State Immutability & Double-Entry Invariance]
-- ============================================================================

-- 1. Accounts Table
CREATE TABLE IF NOT EXISTS cala_accounts (
    id UUID PRIMARY KEY,
    code VARCHAR(128) NOT NULL UNIQUE,
    name VARCHAR(256) NOT NULL,
    description TEXT,
    normal_balance_type VARCHAR(16) NOT NULL CHECK (normal_balance_type IN ('DEBIT', 'CREDIT')),
    status VARCHAR(32) NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'LOCKED', 'CLOSED')),
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. Account Events Table (Optimistic Concurrency Control Event Store)
CREATE TABLE IF NOT EXISTS cala_account_events (
    id UUID NOT NULL REFERENCES cala_accounts(id) ON DELETE RESTRICT,
    sequence INT NOT NULL,
    event_type VARCHAR(64) NOT NULL,
    event JSONB NOT NULL,
    context JSONB DEFAULT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sequence)
);

-- 3. Journals Table (Ledger Boundary)
CREATE TABLE IF NOT EXISTS cala_journals (
    id UUID PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    description TEXT,
    status VARCHAR(32) NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'CLOSED')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. Transactions Table
CREATE TABLE IF NOT EXISTS cala_transactions (
    id UUID PRIMARY KEY,
    journal_id UUID NOT NULL REFERENCES cala_journals(id) ON DELETE RESTRICT,
    correlation_id VARCHAR(128),
    external_id VARCHAR(128) UNIQUE,
    description TEXT,
    metadata JSONB,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 5. Entries Table (The Immutable Double-Entry Sub-Ledger)
CREATE TABLE IF NOT EXISTS cala_entries (
    id UUID PRIMARY KEY,
    transaction_id UUID NOT NULL REFERENCES cala_transactions(id) ON DELETE RESTRICT,
    journal_id UUID NOT NULL REFERENCES cala_journals(id) ON DELETE RESTRICT,
    account_id UUID NOT NULL REFERENCES cala_accounts(id) ON DELETE RESTRICT,
    entry_type VARCHAR(16) NOT NULL CHECK (entry_type IN ('DEBIT', 'CREDIT')),
    layer VARCHAR(32) NOT NULL DEFAULT 'SETTLED' CHECK (layer IN ('SETTLED', 'PENDING', 'ENCUMBERED')),
    units NUMERIC(38, 18) NOT NULL CHECK (units > 0),
    currency VARCHAR(16) NOT NULL,
    sequence INT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_cala_entry_tx_seq UNIQUE (transaction_id, sequence)
);

-- 6. Balances Table (Materialized Snapshot for Fast Reads & Deterministic Row Locking)
CREATE TABLE IF NOT EXISTS cala_balances (
    account_id UUID NOT NULL REFERENCES cala_accounts(id) ON DELETE RESTRICT,
    currency VARCHAR(16) NOT NULL,
    layer VARCHAR(32) NOT NULL DEFAULT 'SETTLED' CHECK (layer IN ('SETTLED', 'PENDING', 'ENCUMBERED')),
    settled_debits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (settled_debits >= 0),
    settled_credits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (settled_credits >= 0),
    pending_debits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (pending_debits >= 0),
    pending_credits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (pending_credits >= 0),
    encumbered_debits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (encumbered_debits >= 0),
    encumbered_credits NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (encumbered_credits >= 0),
    version INT NOT NULL DEFAULT 1,
    modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (account_id, currency, layer)
);

-- 7. Persistent Transactional Outbox (PostgreSQL-as-Message-Queue)
CREATE SEQUENCE IF NOT EXISTS cala_outbox_sequence_seq;

CREATE TABLE IF NOT EXISTS cala_outbox (
    id BIGSERIAL PRIMARY KEY,
    sequence BIGINT NOT NULL UNIQUE DEFAULT nextval('cala_outbox_sequence_seq'),
    event_type VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED')),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for Fast Financial Replay and Worker Consumption
CREATE INDEX IF NOT EXISTS idx_cala_entries_account_currency ON cala_entries(account_id, currency, recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_cala_entries_tx ON cala_entries(transaction_id);
CREATE INDEX IF NOT EXISTS idx_cala_entries_journal ON cala_entries(journal_id);
CREATE INDEX IF NOT EXISTS idx_cala_outbox_unprocessed ON cala_outbox(id ASC) WHERE status = 'PENDING';

