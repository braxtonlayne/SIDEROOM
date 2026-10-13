# 15 — Threat model

XC-SEC-007. A working document: what we protect, from whom, what stops them, and what is still open. It describes the system as built through phase 4 and names the file that enforces each control, so a reviewer can check the claim. When a control changes, change it here in the same PR.

## 1. Assets

| Asset | Why it matters |
|---|---|
| Account credentials (passwords, TOTP secrets, passkeys, recovery codes, session tokens) | Takeover of a member or of staff |
| Invitations and the account cap | The whole access model is "invitation only"; leaking or forging invites defeats it |
| Message content and attachments | Private community; includes DMs |
| Staff powers and the staff access log | Staff can see and act across the platform; the log must be tamper-evident |
| Payment records and perks (phase 6+) | Money, and the link between payments and accounts |
| E2EE material (voice/video, later DMs) | Keys never reach the server |
| Operator secrets (`server.secret_key`, database, storage and SMTP credentials) | Root of trust for stored TOTP secrets and all data |

## 2. Actors

Anonymous visitor; invited-but-not-yet-member; member; malicious member (spam, harassment, attempts to read what they may not); staff member acting in good faith; compromised or malicious staff account; another site's page driving a visitor's browser; network attacker; compromised operator host; a bot token holder.

## 3. Trust boundaries

Browser to Caddy (TLS) to `sr-server` (loopback or compose network) to Postgres and object storage. The browser also talks to the object store directly for presigned uploads and downloads. Activities (phase 8) run on a separate origin and are never trusted.

## 4. Threats and controls

| # | Threat | Controls (where) | Residual risk / open |
|---|---|---|---|
| T1 | Credential stuffing, brute force | Argon2id password hashing; per-IP and per-account rate limits (`sr-api/src/rate_limit.rs`, login throttles in `sr-auth`); `X-Forwarded-For` only trusted behind a proxy (`trust_forwarded_for`) | Limits are per process and reset on restart. Not tested with more than one server |
| T2 | Stolen session cookie | `HttpOnly`, `Secure` (https public URL), `SameSite=Lax`; tokens stored hashed; sessions listable and revocable; `sr-cli user reset-2fa` revokes all | XSS would bypass cookie flags: see T4 |
| T3 | CSRF | `SameSite=Lax` plus the `Origin`/`Referer` allow-list check on every cookie-authenticated non-safe request (`sr-api/src/router.rs` `origin_check`) plus CSRF tokens for the auth endpoints (`sr-api/src/auth/csrf.rs`). CORS allows only the public origin | Bearer and bot tokens are not cookie-authenticated and skip the check by design |
| T4 | XSS | React escaping; markdown rendered by `sr-markdown` to a typed AST, never to raw HTML; CSP `script-src 'self' 'wasm-unsafe-eval'` with no inline or eval scripts, `object-src 'none'`, `base-uri 'self'` (`apps/web/src/security/csp.ts`, enforced by Caddy; pinned by a test); `/cdn` responses carry `default-src 'none'; sandbox` and `nosniff` | `style-src-attr 'unsafe-inline'` is allowed because React sets `style` attributes; it cannot run script. User-supplied SVG is rejected or served sandboxed (XC-SEC-002 open) |
| T5 | Clickjacking | `frame-ancestors 'none'` on the app shell (Caddy) and on every API response (`sr-api/src/security_headers.rs`) | None known |
| T6 | Invite forgery, enumeration, account-cap races | Invite codes are 10 random characters (about 59 bits) stored as issued so owners can list them; checks are rate limited per IP (10/min), which makes guessing impractical; cap enforced under an advisory lock (`ENT-INV-006`) | Concurrency test ENT-INV-010 still open; codes are not hashed at rest, so a database leak exposes unredeemed invites |
| T7 | Unauthorized access to channels, library items, staff tools | Permissions evaluated server-side on every route; client hiding is never the boundary (AGENTS.md section 5). Permission bits shared with the client from one Rust crate | A generated per-route test matrix (XC-SEC-005) does not exist yet; coverage today is hand-written tests per feature |
| T8 | SSRF via link unfurls, webhooks, RSS, metadata lookups | `sr-unfurl` resolves and rejects private, loopback and link-local addresses and re-checks after redirects; TURN denies relay into private ranges (`coturn/turnserver.prod.conf.example`) | XC-SEC-003 asks for one shared client crate for all outbound fetches; today only unfurl has it |
| T9 | Malicious uploads (polyglots, oversized, decompression bombs, executable content) | Content sniffed against an allowlist per surface; archives validated before extraction with size and entry limits (`sr-library`); media re-encoded by the pipeline; all served with `nosniff` and a sandboxing CSP | RAR support uses the vendored UnRAR (owner decision 1) |
| T10 | Staff abuse | Staff actions need explicit permission bits; every access to private data writes a hash-chained log row (`sr-cli verify-access-log` detects tampering); no Owner password in logs | The log detects tampering but cannot prevent a compromised Owner |
| T11 | Database or backup theft | TOTP secrets encrypted with `server.secret_key`; password and token hashes only; backup directory is mode 700; keep the key apart from dumps (BACKUP.md) | Messages and attachments are not encrypted at rest by the application. Use disk encryption on the host |
| T12 | Network attacker | TLS everywhere via Caddy; HSTS (one year, includeSubDomains); Postgres `sslmode=require` or better if off-host (INSTALL.md section 2) | Preloading HSTS is the operator's choice |
| T13 | Operator secret leakage through logs, errors or the repo | No secrets in code; CI scans nothing yet; `deploy/compose/.env.prod.example` holds placeholders only; services read secrets from env or files | Add a secret scanner (gitleaks) to CI: open |
| T14 | Supply chain | `cargo deny` (licences, advisories, sources) and an npm audit signal in CI; Dependabot weekly; lockfiles committed and `--frozen-lockfile` | Audit is advisory for npm (see ci.yml); no signature verification of dependencies |
| T15 | Service worker serving stale or private data | The worker never intercepts non-GET, cross-origin, `/api`, `/gateway`, `/cdn`, `/internal`, `/oauth`, `Authorization` or `Range` requests; it only stores build output and the public app shell (`apps/web/public/sw.js`, pinned by tests and by a browser test that inspects the caches) | A shared computer keeps cached build files; they contain no user data |
| T16 | E2EE downgrade or key leakage (voice/video) | Keys are generated and kept in the client (`sr-e2ee`); the SFU forwards ciphertext | Out of scope here until phase 5 voice lands; threat model to be extended then |
| T17 | Denial of service | Rate limits, request timeouts (30 s), body size limits, gateway connection cap, bounded job concurrency, compression only on responses | No load test yet (XC-PERF-003); no WAF. A volumetric attack needs the host's or a CDN's help |

## 5. Out of scope

A compromised operator host (root can read everything on it); a member sharing their own screen or content elsewhere; endpoint malware on a member's device; legal process against the operator.

## 6. Review cadence

Re-read this document at each phase boundary and whenever a new outward-facing feature ships (payments, bots and OAuth, activities, federation, public sharing). Anything marked open above is tracked in `docs/inventory/18-cross-cutting.md`.
