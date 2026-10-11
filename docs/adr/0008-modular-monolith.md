# ADR-0008: Modular monolith with an EventBus seam

- **Status:** accepted
- **Date:** 2026-10-11

## Decision
A single `sr-server` binary composed of crates. Cross-module communication goes through the `EventBus` trait (in-process now; Postgres NOTIFY or NATS later). No Redis: caches are in-process (`moka`).

## Consequences
Simple ops at the 20k-account scale. Horizontal scaling requires implementing a distributed `EventBus` plus sticky gateway sessions, which is planned but not needed now.
