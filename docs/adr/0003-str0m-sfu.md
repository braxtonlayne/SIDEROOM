# ADR-0003: A custom SFU built on str0m

- **Status:** accepted
- **Date:** 2026-10-11

## Context
Voice/video/screen share needs an SFU with simulcast, congestion control, and E2EE pass-through. D-150 prefers Rust.

## Decision
Build `sr-voice` on **str0m** (sans-IO Rust WebRTC). We own the room logic, layer selection, sender allowance, and egress governor. **webrtc-rs** is the documented fallback if str0m blocks progress.

## Alternatives considered
- **LiveKit (Go):** most mature and fastest to ship, but not Rust. It remains the emergency fallback, behind the same signaling abstraction (`sr-voice-proto`), and would need an ADR amendment.
- **mediasoup (C++ worker, Rust bindings):** mature, but the worker is C++.

## Consequences
More engineering in Phase 2. str0m lacks TURN and a jitter buffer. An SFU doesn't need a jitter buffer, and TURN is covered by ADR-0004. Load testing is part of the Phase 2 exit gate.
