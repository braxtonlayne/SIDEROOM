# Passkey relying-party spike report (F-14 / D-161 / ENT-AUTH-010 step 1)

Outcome: **a pure-Rust path meets the bar.** ADR-0009 (webauthn-rs with vendored OpenSSL) is not needed and was not written. No exception to `deny.toml` was added or is required.

## What the bar was

`none` attestation only; ES256, EdDSA, RS256; no `openssl` or other native dependency (D-150; `deny.toml` bans `openssl`, `openssl-sys`, `native-tls`); licences inside the `cargo deny` allow-list; strict origin, rpIdHash, flags, counter and challenge checks.

## Options compared

| Option | Pure Rust | Result |
|---|---|---|
| `webauthn-rs` | No (links `openssl`) | Rejected by policy; this is the D-161 fallback. |
| `webauthn_rp` 0.3 (crates.io name `webauthn_rp`, not `webauthn-rp`) | Yes (RustCrypto + own CBOR) | Dependency tree resolves, but it pulls the **`rsa` crate**, which carries the unpatched RUSTSEC-2023-0071 (Marvin). It also brings a large server-state model (and `precis` crates for username handling) that does not map onto our `AuthStore` seam, and is hosted on a single maintainer's own git server. Not adopted. |
| `passkey-types` 0.6 (1Password `passkey-rs`) | Yes | Wire types and COSE/CBOR types for the client/authenticator side; it has no relying-party verification logic (origin, rpIdHash, counter, signature), so a verifier would still be ours. Pulls `coset` and `ml-dsa`, which we do not need. Not adopted. |
| `p256` + `ed25519-dalek` + `rsa` + `ciborium` | Yes | Works, but `rsa` (every version, 0.9.10 and 0.10.0-rc.x) fails `cargo deny` advisories with RUSTSEC-2023-0071. The advisory concerns private-key operations (timing leak of the secret exponent); a relying party only verifies, so it does not apply in practice, but shipping it needs an advisory ignore in `deny.toml`, which is owner-controlled. Not adopted. |
| **Own verifier on RustCrypto (`p256`, `ed25519-dalek`, `crypto-bigint`, `ciborium`)** | Yes | **Built.** RS256 is the small RSASSA-PKCS1-v1_5 public operation on `crypto-bigint` (about 60 lines), so `rsa` is not in the graph at all. |

Licences: p256 and crypto-bigint MIT OR Apache-2.0, ed25519-dalek BSD-3-Clause, ciborium Apache-2.0; all already allowed. `cargo deny check` passes (advisories, bans, licenses, sources) with the feature enabled; `deny.toml` uses `all-features = true`, so the feature is checked even though it is off by default.

Maintenance: p256, ed25519-dalek and crypto-bigint are RustCrypto/dalek projects at current majors (p256 0.14, ed25519-dalek 3.0, crypto-bigint 0.7). `ciborium` 0.2.2 is stable but slow-moving; the CBOR subset needed (definite-length maps, ints, bytes, text) is small enough to replace if it stalls.

## What was built

`crates/sr-auth/src/webauthn.rs` (+ `webauthn/tests.rs`, `webauthn/rsa_fixture.txt`), behind cargo feature `webauthn` (off by default; `cargo test -p sr-auth --features webauthn`).

- `begin_registration` / `begin_authentication`: random 32-byte challenge, 5 minute TTL, JSON for `navigator.credentials.create/get` (resident key required, `attestation: none`, `pubKeyCredParams` -7, -8, -257, `excludeCredentials` / `allowCredentials`; an empty allow list is the discoverable login).
- `ChallengeBook`: in-memory single-use ceremony store (`take` removes the entry), with `purge`.
- `finish_registration`: clientDataJSON type, challenge (constant time), exact origin, crossOrigin, expiry; attestationObject as strict CBOR (no trailing bytes, duplicate keys rejected), `fmt` must be `none` with an empty `attStmt`; authData: rpIdHash, UP, UV when required, BS-needs-BE, AT required, credential id length 1..=1023, COSE_Key parsed from the byte stream, extension block must be a CBOR map covering exactly the remaining bytes, rawId must equal the attested id; COSE key validated (algorithm allow-list, kty/crv consistency, P-256 point on curve, Ed25519 point decode, RSA 2048..=4096 bits, odd exponent of at most 32 bits).
- `finish_authentication`: same client-data and authData checks, AT must be absent, signature over `authData || SHA-256(clientDataJSON)` (ES256 DER, EdDSA via `verify_strict`, RS256 compares the whole recomputed EMSA-PKCS1-v1_5 block so the recovered block is never parsed), BE must not change, counter must strictly increase unless both are 0, user handle must match when both sides have one. Returns the new counter, backup state and UV for the caller to persist.

## What is verified

33 unit tests, all passing, against authenticators simulated in the tests with RustCrypto (P-256 and Ed25519 signing keys; RSA signing done independently on `crypto-bigint` with a fixed throwaway 2048-bit key):

- Round trips for ES256, EdDSA and RS256 (registration then assertions, UV on and off).
- Interop vector: an RS256 signature produced by the OpenSSL command line (`openssl dgst -sha256 -sign`) verifies; an altered message, wrong length, all-zero, s >= n and a flipped bit fail.
- Negatives: wrong origin (suffix, scheme and trailing-slash variants), crossOrigin true, wrong rpIdHash (registration, and a validly signed assertion for another RP id), bad, truncated or empty signature for every algorithm, signature from another algorithm or key, tampered authData or clientDataJSON, replayed challenge, response for another ceremony, expiry, counter regression (equal, lower, restart at 0; 0 to 0 allowed), unlisted algorithms (ES384, ES512, PS256, RS1, unknown, missing), alg versus kty/crv mismatch, invalid P-256 point, short or even RSA modulus, bad RSA exponent, malformed CBOR (garbage, empty, truncated, trailing bytes, non-map, duplicate keys, missing authData), malformed authData (short, truncated, trailing, bad extension block, zero or oversized credential id), `packed` format and a non-empty `none` statement, UP / UV / BS / AT / BE flag cases, credential id and user handle binding, corrupt stored key.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` (and `--all-features` for sr-auth), `cargo test --workspace`, `cargo deny check`.

## Not verified or not done

- **Real authenticators and the CDP virtual authenticator (Q83).** No frontend or Playwright work was in scope, so no browser exercised this. The simulated authenticators follow the spec byte layouts, but a browser run is the remaining proof (especially `credProps` extension output, the ED flag, and Safari / iCloud Keychain BE/BS behaviour).
- **REST ceremony endpoints, store trait and migration 0180-0189: not wired.** The seam is `AuthStore` (`sr-auth::store`), implemented by `PgAuthStore` in `sr-db` and an in-memory store in `sr-api`, plus the partial-session / 2FA path in `sr-api/src/routes/auth/{login,mfa}.rs`. Passkeys touch all of those crates and the login state machine, which is outside this spike's sr-auth-only scope and collides with parallel work. ENT-AUTH-010 is therefore **not ticked**; the inventory line and the doc 03 table were annotated.
- No attestation verification (by design), no `topOrigin`, no Level 3 hints / conditional create / related origins.
- `ChallengeBook` is process-local; step 2 should persist pending ceremonies (Postgres, short TTL) if more than one node serves auth.
- `rsa_fixture.txt` is a throwaway test key and protects nothing.

## Recommendation

1. Accept the pure-Rust verifier as the D-161 outcome: no ADR-0009, no OpenSSL, no `deny.toml` change. The doc 03 table is updated.
2. Proceed with ENT-AUTH-010 step 2 (size L):
   - Migration `0180_passkey_credentials.sql`: `passkey_credentials(id bigint pk snowflake, account_id bigint not null references accounts, credential_id bytea not null unique, cose_key bytea not null, alg smallint not null, sign_count bigint not null default 0, backup_eligible bool not null, backup_state bool not null, aaguid bytea, name text not null, created_at, last_used_at)`, plus `accounts.user_handle bytea` (random 32 bytes, never the account id) for WebAuthn `user.id`.
   - Extend `AuthStore` (or add a `PasskeyStore`) with insert, list, rename, delete, find-by-credential-id and update-counter; implement in `PgAuthStore` and the in-memory store.
   - Routes from Q78: `POST /sr/auth/passkeys/register/{begin,finish}`, `POST /sr/auth/passkeys/login/{begin,finish}`, `GET/PATCH/DELETE /sr/auth/passkeys[/{id}]`; password re-auth on register and delete (as TOTP does); 2FA use through the existing partial session. Persist `PendingCeremony` (add `Serialize`) keyed by a ceremony token.
   - Treat a counter regression as a security event (audit entry, optionally disable that credential) instead of a plain login failure.
   - Run the CDP virtual-authenticator Playwright test (Q83) before un-hiding the passkey buttons.
3. Later hardening: a `cargo fuzz` target for `finish_registration` / `finish_authentication`, since they parse attacker-controlled CBOR.
