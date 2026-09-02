# SuperCala Architecture & Invariant Specification

## 1. System Topology

```
┌─────────────────────────────────────────────────────────────┐
│                    Async Tokio Runtime                      │
│                                                             │
│  ┌─────────────────┐ ┌─────────────────┐ ┌────────────────┐ │
│  │ Worker Task #1  │ │ Worker Task #2  │ │ Worker Task #N │ │
│  └────────┬────────┘ └────────┬────────┘ └───────┬────────┘ │
│           │                   │                  │          │
│           ▼                   ▼                  ▼          │
│  ┌────────────────────────────────────────────────────────┐ │
│  │               Decorrelated Jitter Backoff              │ │
│  │        sleep = rng(base..base*3); base = min(500)      │ │
│  └────────────────────────────┬───────────────────────────┘ │
└───────────────────────────────┼─────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────┐
│               sqlx::Pool<Postgres> (50 Conns)               │
│                                                             │
│  1. BEGIN Transaction                                       │
│  2. Canonical Lexicographical Lock Acquisition:             │
│     SELECT id, version FROM cala_balances                   │
│     WHERE account_id IN ($1, $2)                            │
│     ORDER BY account_id ASC FOR UPDATE                      │
│  3. Balance Update with Row Assertion:                      │
│     UPDATE cala_balances SET settled_credits = ...          │
│     ASSERT rows_affected() > 0                              │
│  4. Immutable Journal Entry Insert                          │
│  5. Transactional Outbox Persistence (Dedicated Sequence)   │
│  6. COMMIT                                                  │
└─────────────────────────────────────────────────────────────┘
```

## 2. Invariant Proofs

### Invariant 1: Deadlock Elimination via Directed Acyclic Graphs (DAG)
* By sorting distinct account UUIDs lexicographically (`ORDER BY account_id ASC`), all transactions acquire locks in the exact same global total order.
* Circular lock wait cycles ($T_1 \to L_A \to L_B$ while $T_2 \to L_B \to L_A$) are mathematically impossible, reducing deadlocks (`40P01`) to $O(1) = 0$.

### Invariant 2: Connection Pool Starvation Defense
* High worker concurrency (300 workers) on limited pooled connections (50 max) leads to connection hold-time inflation.
* SuperCala enforces:
  1. Pre-transaction validation in memory (rejecting empty or non-positive unit legs before acquiring a database connection).
  2. Randomized exponential jitter backoff to desynchronize retrying workers and eliminate thundering-herd convoys.

### Invariant 3: Precision & Arithmetic Bounds
* Storage uses PostgreSQL `NUMERIC(38, 18)` and `rust_decimal::Decimal`.
* Micro-dust 1 SAT ($0.00000001\text{ BTC}$) and 21M BTC ($2.1 \times 10^{15}\text{ SATS}$) are asserted without truncation or loss of significance.
