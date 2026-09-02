# Contributing to SuperCala

Thank you for your interest in improving high-assurance financial ledger tooling.

## Core Directives

1. **Deterministic Invariants**: Every pull request modifying ledger transactions or locking algorithms must preserve strict balance conservation (`sum(debits) == sum(credits)`) and DAG-ordered lock acquisition (`ORDER BY account_id ASC`).
2. **Zero Deadlocks**: PRs introducing new lock paths must be verified against the 100-worker parallel chaos harness (`cargo run --release -- --workers 100 --tx-per-worker 20 --jitter`).
3. **Machine-First (`llms.txt`)**: When updating architecture, schema migrations, or command flags, update [`llms.txt`](llms.txt) to keep AI agent execution context current.

## Development Workflow

### 1. Prerequisites
* Rust 1.75+ (stable)
* Docker & Docker Compose (for local PostgreSQL 16 testbed)

### 2. Launch Local Testbed
```bash
docker run -d --name supercala-postgres --cap-add=NET_ADMIN \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=supercala \
  -p 5433:5432 postgres:16-alpine \
  -c deadlock_timeout=50ms \
  -c autovacuum_vacuum_scale_factor=0.05
```

### 3. Run Migrations & Verification Suite
```bash
docker exec -i supercala-postgres psql -U postgres -d supercala < migrations/0001_initial_cala_schema.sql
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --test edge_cases_and_limits
```

### 4. Code Formatting
```bash
cargo fmt --check
```

## Pull Request Guidelines
* Keep PRs focused on a single invariant, benchmark, or optimization.
* Include benchmark output showing before/after TPS and retry counts.
