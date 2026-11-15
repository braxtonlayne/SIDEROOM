# Wave 78 handoff: tests (SEC-AUD-308, inventory notes D-808)

## Done
- **SEC-AUD-308** (commit "SEC-AUD-308: composerActivity waits longer..."). Test fault, not product. `composerActivity.test.tsx` alone: run 1 FAILED ("Unable to find role=button name 'Add to message'"; the DOM dump shows only the loading `role=status aria-busy` placeholder, so the Lounge chunk had not rendered yet), runs 2 and 3 passed. The failed run was the cold transform-cache one (tests 6.05 s, so the 5 s `asyncUtilTimeout` of `src/test/setup.ts:86` ran out); warm runs take 5.4-5.6 s in total, so it sits near the edge. The product renders the button (composerGaps passes 14/14). CI runs it: `pnpm -r --if-present test` in `web.yml`, vite.config.ts `test` has no include/exclude/skip list; CI is green because its first render is faster or warm. Last changes: the test by e91e7b87 (XC-VIS-003 G5); `Composer.tsx` by 14179151 (wave 77 Popover anchor) and earlier. Fix: the first `findByRole('button', {name:'Add to message'})` takes `{ timeout: 20000 }` (testTimeout is 30 s); assertions unchanged, nothing skipped. After the fix: 3 of 3 passes (14.1 s, 13.9 s, 13.3 s total); `composerGaps.test.tsx` alone 14/14 (untouched). I did not reproduce the failure again after the fix (it was a cold-cache failure; no way to force one), so "fails without the fix" is the one cold run seen before the change.
- The optional two-file run beside composerGaps under the lock: dropped (I did not take the lock; no need, the cause is the first render).
- Typecheck (`tsc --noEmit`), eslint, prettier on the file: clean.

## Item 2: inventory notes (15 open rows, checked against the tree on 2026-11-15)
| ID | verdict | waits on |
|---|---|---|
| LNG-SSET-054 | note is true (`attachments.rs:139-144` has the spoiler path, D-224 comment) | decision D-224 (built when an operator asks) |
| VOX-CLI-002 | true (`apps/web/src/voice/vad.ts` exists; no ML model/dependency) | download of a model |
| DON-PAY-009 | true (`crates/sr-payments/tests/esplora_live.rs` and the doc exist) | decision D-257 |
| TV-HLS-005 | true (`crates/sr-tv/tests/hls_sync.rs` exists; `e2e/playwright.config.ts` lists firefox and webkit projects, but no Live TV spec is tagged for WebKit that I found by grep, shallow check) | WebKit/Firefox install |
| EXT-PKG-002 | true (`crates/sr-theme/src/validate.rs` and `crates/sr-theme-wasm` exist; no lightningcss) | owner's Phase 6 call / lightningcss download |
| XC-CI-005 | true | decision D-261 |
| RDY-VOX-002 | true (voice-soak.yml and voice-soak.spec.ts exist) | owner's ten-member run / dispatch |
| RDY-DOC-003 | true (13 `runbook:` annotations in `deploy/monitoring/alerts.yml`; RUNBOOK.md:123 still says "walk pending"; `scripts/check-ops-files.mjs` exists) | a walk on a running prod-smoke stack |
| **RDY-PAY-001** | **OUT OF DATE.** Replacement note: `_(built: `crates/sr-payments/tests/esplora_live.rs`; `docs/dev/payments-live-test.md`, which holds the read-only Esplora test and, since wave 76, the end-to-end walk (7 steps, "What to record", "How to undo it") and an empty "Recorded runs" table; the invoice watcher is `crates/sr-server/src/payments_jobs.rs`; left: the owner's run with a funded signet wallet and the first row in the table, 2026-11-15)_` | owner's funded signet wallet |
| RDY-DEV-003 | true (sdk-compat.yml pair discord.js/serenity; `e2e/sdk-compat` has no discord.py or twilight; doc lists them unrun; run 40318281178 in doc) | owner's yes to downloads |
| DEP-FU-001 | true (Cargo.lock tantivy 0.26.2; three crates "0.26"; the three RUSTSEC ignores in `deny.toml`) | download; on/after 2026-11-17 |
| SEC-ENT-004 | true (19 workflow files; dispatch-only soak) | owner's soak dispatch |
| MUS-JB-001 | true as far as checked (voice-soak.spec.ts: grep "music" finds only verdict helpers, no music step) | Firefox/WebKit runs, soak dispatch |
| OPS-DEP-001 | true (date 2026-11-17 not come; lock still 0.26.2) | date, then download |
| SEC-AUD-196 | true (a release asset claim; nothing to check in tree) | first release |
| README.md LNG-MSG-014 | format example, not a row | |

No row should be ticked. "won't build" habit: closed-by-decision rows elsewhere are ticked (`[x]`, e.g. LNG-EXP-012 "won't build (D-231: shim only)", LNG-GLD-029). DON-PAY-009 and XC-CI-005 say "won't build (D-257 / D-261)" but are left `[ ]`: they differ from that habit. Proposal for the commander to decide: tick them like LNG-EXP-012, or leave (the plan says leave).

### Payments walk against the code (docs/dev/payments-live-test.md)
Checked and matching: `SR_PAYMENTS_ESPLORA_URL`, `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS`, `SR_PAYMENTS_RATE_SOURCES` (payments_jobs.rs, preflight), `GET /donations/config` and `/donations/me` (routes/donations/member.rs), `payments.watch` every 15 s (`WATCH_INTERVAL`), invoice expiry 60 min default (`invoice_expiry_minutes`), `SR_PAYMENT_UPDATE` (InvoiceModal.tsx), `MANAGE_DONATIONS`, `payment_audit` table, Admin > Donations sections "Invoices", "Entitlements", "Pricing and perks". Not verified: the "I've paid" 10 s poll figure (no literal found in `support/InvoiceModal.tsx` by grep) and the Admin "Wallet" section label (not found under that name by a quick grep). No sentence proven stale; those two are for a reader of the code.

## Tails
1. `composerGaps.test.tsx`: left alone (it passed 14/14 alone; its first-wait relies on the same 5 s default; the cause shown by item 1 is the cold first render, so a one-line timeout on its first `findByRole`s could be added next wave if it times out in CI; not done: plan says the same figure only if item 1 shows cause, and I did not see it fail).
2. `packages/ui` prettier: `.prettierignore` lists `packages`, so the root `prettier --check .` (the `lint` script) never looks there; `prettier --check "packages/ui/src/**"` therefore reports "all matched files" (ignored). With `--ignore-path .gitignore`, 75 files in `packages/ui/src` fail the root config. The packages' `lint` scripts are `eslint src` only; CI (`web.yml`: `pnpm -r --if-present lint`) checks no prettier on them. So it is a gap by the ignore file (packages deliberately excluded, never formatted), not a second config (no package-level prettier config exists). No reformatting done.

## Other
- Proposed inventory row: `- [x] **SEC-AUD-308** [TEST] composerActivity.test.tsx no longer fails on a cold Lounge first render: the first findByRole waits up to 20 s (the 5 s asyncUtilTimeout was the cause). Handoff fix-p78-tests.md.`
- Changelog: none (no member-visible change).
- Owner questions: tick DON-PAY-009 / XC-CI-005 like other "won't build" rows? Should `packages` be prettier-formatted, or stay ignored?
- Stashes: I did not look at or touch any stash. No stack, database, ports or binaries folder was used, nothing to clean up. No lock taken.
