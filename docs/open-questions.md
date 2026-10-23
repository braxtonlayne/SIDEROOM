# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md) (the Phase 2 round is the section "Phase 2 answers (2026-10-13)"; its entries are numbered `OQ-n` with the numbers this file used before the sweep). Adopted answers are D-157 to D-183 (Phase 2 round), D-185 to D-216 (Phase 8 to 11 round) , D-217 to D-220 (Phase 12 handoff round, section "Phase 12 handoff questions (answered 2026-10-15)"), D-221 to D-224 (Phase 12 backend handoff round, section "Phase 12 backend handoff questions (answered 2026-10-15)"; neither Phase 12 round raised anything new for the owner) and D-225 to D-233 (the remaining Phase 12 and the Phase 13 handoffs, section "Phase 12 and 13 remaining questions (answered 2026-10-15)"; D-225 and D-226 are owner decisions given in chat, and the round raised nothing new for the owner), D-234 to D-240 (section "Phase 13 developer platform and settings questions (answered 2026-10-15)"; nothing new for the owner) D-241 to D-242 (section "Phase 13 final questions (answered 2026-10-16)") D-243 to D-251 (section "Phase 14 questions (answered 2026-10-16)"; nothing new for the owner) and D-252 to D-257 (section "Phase 14 and 15 remaining questions (answered 2026-10-16)"), D-258 to D-263 (section "Wave 15 questions (answered 2026-10-17)"; nothing new for the owner), D-270 to D-271 (section "Phase 16 uploads questions (answered 2026-10-19)") and **D-287 to D-308 (section "Owner round (answered 2026-10-19, delegated)": every owner item this file carried, items 1 to 12, answered by a delegated proxy on the owner's instruction, plus the questions the Phase 15 and 16 handoffs left for the owner)** in [01-decisions-log.md](01-decisions-log.md), and the implementation work they imply is listed in [dev/answers-followups.md](dev/answers-followups.md), [dev/answers-followups-p12.md](dev/answers-followups-p12.md), [dev/answers-followups-p13.md](dev/answers-followups-p13.md), [dev/answers-followups-p14.md](dev/answers-followups-p14.md), [dev/answers-followups-p15.md](dev/answers-followups-p15.md) and [dev/answers-followups-p16.md](dev/answers-followups-p16.md) (the owner round is M-01 onward there). Paths are relative to the repo root.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

None open. The twelve items this section held were answered on 2026-10-19 under the owner's delegation ("have a fable agent answer all of your questions"); the owner may override any of them by changing the row in the decisions log:

| Was | Decision | Follow-up |
|---|---|---|
| 1. UnRAR licence (D-170) | D-287: RAR reading stays on; UnRAR licence text in `THIRD_PARTY_NOTICES.md` | M-01 |
| 2. P8-DON-4 Esplora and price sources | D-288: self-hosted `electrs` profile; Coinbase, Kraken, CoinGecko, median | M-02 |
| 3. P11-MSG-1 default sticker packs | D-289: one CC0 typographic starter pack generated in code, seeded by `sr-cli` | M-03 |
| 4. P11-SHL-3 avatar decorations | D-290: closed, effects only; decorations only as reviewed packages | M-04 |
| 5. P11-CLI-1 badge glyphs | D-291: Unicode glyphs are final | M-04 |
| 6. P11-USB-2 Discord OAuth app | D-292: reuse the one app; owner registers the second redirect URI | M-05 |
| 7. MOD-4 copyright (legal) | D-293: staff-entered notices, counter-notices, strike review at 3 in 12 months; not legal advice | M-06 |
| 8. MOD-5 minimum age (legal) | D-294: 13 by self-declaration as a setting; reset-and-restrict under 18; suspend under 13; not legal advice | M-06 |
| 9. MOD-6 data-subject deadline (legal) | D-295: 30 days as a setting; D-124 retention stated; operator country shown; not legal advice | M-06 |
| 10. P13-REV-3 ingest roots | D-296: `/srv/sideroom/ingest`, as H-48 built | none |
| 11. P15-RST-2 JASSUB (legal, download) | D-297: ship `jassub@2.5.16` with `rvfc-polyfill` replaced by an empty module; no GPL in the bundle; not legal advice | M-08 |
| 12. P16-UPL-3 `/support` copy (money wording) | D-298: final copy written; exact strings in M-18 | M-18 |

The same round answered the handoff questions that were waiting on the owner: LNG-CMP-001 (D-299, no Lexical), integrations policy live (D-300), the package-card Approve button (D-301), unsaved profile pictures (D-302), no weekly template on manual channels (D-303), the watch-party card switch (D-304), no user installs (D-305), the SDK compatibility workflow (D-306), the visual baselines dispatch (D-307) and the ordering of the human actions (D-308).

Earlier confirmations (age gate, font, door image, token calibration) are recorded in [open-question-answers.md](open-question-answers.md).

## Owner and integrator actions, human actions and legal questions

These are not decisions, so they no longer live in this file:

- Every pending owner or integrator action (accounts, host, GitHub, downloads waiting for a yes, development machines), ordered, each with what it unblocks: [dev/owner-checklist.md](dev/owner-checklist.md). The order of the human actions is D-308.
- The open legal questions for a lawyer: [legal/lawyer-brief.md](legal/lawyer-brief.md). Not legal advice.
- The ordered steps from an empty server to the first invites: [../deploy/GO-LIVE.md](../deploy/GO-LIVE.md).
