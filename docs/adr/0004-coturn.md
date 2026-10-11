# ADR-0004: coturn for TURN relay

- **Status:** accepted
- **Date:** 2026-10-11

## Context
10–20% of clients behind restrictive NATs need a TURN relay. No maintained, production-grade Rust TURN server was found (Oct 2026).

## Decision
Ship **coturn** with time-limited REST-API credentials (shared secret HMAC) issued by `sr-server`.

## Consequences
One C component. Revisit when a Rust TURN server matures. The `turn` crate from webrtc-rs is a candidate for an embedded relay inside `sr-voice`.
