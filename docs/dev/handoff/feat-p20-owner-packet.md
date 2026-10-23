# Handoff: feat/p20-owner-packet (wave 20, docs only)

Branch `feat/p20-owner-packet` on `integration/phase20`. Docs only: no builds, servers or downloads. Nothing pushed.

## Resume here

Nothing is mid-work. Next: the integrator merges this with `ops-smoke` and `preflight`; then `install.sh` gets its `sr-cli preflight` call (the GO-LIVE steps 4 and 6 name `sr-cli preflight` and `--alert-test`, which land on the preflight branch).

## Done

- RDY-OWN-001 `docs/legal/lawyer-brief.md`: marked "not legal advice; questions for a lawyer" at the top; preamble about the instance; Q1 to Q19, one per listed decision (D-336 split into its letters (a) to (h)); D-351 appears once, as "considered, no question". Each question quotes the decision id and points at its own lawyer note instead of restating it. `open-questions.md` no longer repeats the list.
- RDY-OWN-002 `docs/dev/owner-checklist.md`: six groups with checkboxes and an "Unblocks" on each row. The two action sections of `open-questions.md` are replaced by a link to it, the brief and GO-LIVE.
- RDY-OWN-003 `deploy/GO-LIVE.md`: nine steps with command and "done when"; linked from INSTALL.md, README and the owner checklist; `scripts/check-ops-files.mjs` now checks that the file exists, has its last step and that INSTALL.md links it (the script is outside the Split table; the roadmap acceptance asks for it, so the edit is six lines).
- RDY-DOC-001: README Status rewritten; doc 12 section 5 (and the `sr-cli restore` and `upgrade` sentences it touched); docs/README (inventory delivered, owner links); `THIRD_PARTY_NOTICES.md` "To complete" note (the web copy `apps/web/public/THIRD_PARTY_NOTICES.md` refreshed with `check-notices-copy.mjs --write`, it must be byte-identical).

## Findings worth knowing

- **VAPID:** the owner checklist of old said "generate the VAPID key pair on the host". The server creates and seals it in the database on first start (`crates/sr-server/src/app.rs`, `load_or_create_vapid`); the real action is to keep `server.secret_key` backed up. The row says so.
- Doc 12 section 5 said `sr-cli restore --from <snapshot>`; the real form is `sr-cli restore <dump> [--force]`. Section 6 said `sr-cli upgrade` restarts the service; it checks, backs up and migrates only. Both corrected.
- The "Setup Wizard" is the setup checklist on the Admin dashboard (ADM-DASH-003), not a separate wizard.
- Notices note: which component the NTP term belongs to is not identified; the Zlib holder is stated as expected, not confirmed.

## Phase 20 answers (D-353 to D-357)

The five open questions of this wave (licence wording, `prod-smoke.yml` triggers, the preflight permission, preflight row text, insecure TLS) were answered under the owner's delegation: decisions D-353 to D-357 in `docs/01-decisions-log.md`, answers P20-1 to P20-5 in `docs/open-question-answers.md`, follow-ups Q-01 to Q-05 under "Phase 20 round" in `docs/dev/answers-followups-p16.md`. The README License section and the "Repository layout" heading were changed on this branch (D-353); `Cargo.toml` and the `package.json` fields are the integrator's (Q-01).

## Run

`node scripts/check-doc-links.mjs` ok; `node scripts/check-ops-files.mjs` ok; `node scripts/check-notices-copy.mjs` identical; prettier check ok on the changed files.

## Open questions for the owner

1. ~~License section of the README ("To be decided"): update after D-309's clarification?~~ Answered: D-353.
2. Is `prod-smoke.yml` meant to be named in the owner checklist as a dispatch row only after ops-smoke merges (it is, worded "once wave 20 is merged")?
