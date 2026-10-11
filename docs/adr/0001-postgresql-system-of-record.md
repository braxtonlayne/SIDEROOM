# ADR-0001: PostgreSQL as the system of record

- **Status:** accepted
- **Date:** 2026-10-11

## Context
D-150 asks for Rust implementations where good ones exist. The system of record needs ACID transactions (invite redemption under the account cap, payments, permission changes), mature backups and point-in-time recovery, partitioning for messages, and compile-time-checked queries.

## Decision
Use PostgreSQL 17 through `sqlx` (Rust). All access goes through `sr-db` repositories.

## Alternatives considered
- **SurrealDB (Rust, v3.x):** active, but its operational track record for a long-lived system of record, plus backup/PITR tooling, is weaker. `sqlx`-style compile-time checking is unavailable.
- **TiKV (Rust):** a distributed KV store. Massive overkill at this scale, and it would need a SQL layer (TiDB, written in Go).
- **ScyllaDB/Cassandra for messages (as Discord does):** unnecessary below ~billions of messages. Postgres partitioning covers 20k accounts comfortably.

## Consequences
One non-Rust infrastructure component. Messages are range-partitioned monthly. If a mature Rust SQL database emerges, `sr-db` isolates the migration surface.
