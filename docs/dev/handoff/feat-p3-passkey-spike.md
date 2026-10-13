# Handoff: feat/p3-passkey-spike (COMPLETE, spike)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p3-passkey-spike, base integration/phase2. Local commits only, nothing pushed.

## Result
F-14 / D-161: a pure-Rust passkey relying party is viable. `crates/sr-auth/src/webauthn.rs` (feature `webauthn`, off by default) does `none`-attestation registration and assertion for ES256, EdDSA and RS256 on p256, ed25519-dalek, crypto-bigint and ciborium. No openssl, no deny.toml change, no ADR-0009. Full comparison and verification: `docs/dev/passkey-spike-report.md`.

## Verified
33 unit tests (positive per algorithm, OpenSSL-signed RS256 interop vector, about 25 negative cases). fmt, clippy -D warnings, workspace tests and cargo deny pass.

## Not done
- REST endpoints, store trait and migration 0180-0189 are deliberately not wired (touches sr-db, sr-api and the login state machine; outside the sr-auth scope). Proposed schema and routes are in the report.
- CDP virtual authenticator / real-browser run (Q83) not done; no frontend work.
- ENT-AUTH-010 left unticked (annotated). The F-14 box in docs/dev/answers-followups.md is left for the owner to tick on merge.

## Decisions worth a look
- The `rsa` crate is avoided on purpose: every release has the unpatched RUSTSEC-2023-0071 and would need a deny.toml advisory ignore. RS256 verification is hand-written on crypto-bigint (public operation only; compares the full recomputed EMSA block).
- Cargo.lock gained the p256 / ed25519-dalek 3 / crypto-bigint / ciborium trees (feature-gated in sr-auth).
- doc 03 section 2: the "2FA / passkeys" row was split into 2FA and Passkeys rows.
