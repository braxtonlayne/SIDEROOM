# Handoff: feat/p15-rest-b (agent rest-b)

Base: integration/phase15 (= main at c65792cf). Inventory items 01, 02, 04 to 08, 18 and J-21.

## firstpaint.test.tsx investigation
`apps/web/src/i18n/firstpaint.test.tsx` passes on the untouched base in a fresh worktree: 12 of 12, both with the wasm
package built (`pnpm --filter @sideroom/wasm build`) and with `packages/wasm/dist` moved away (the test never needs
the wasm; the MSW "unhandled request" lines for `*_bg.wasm` and `/api/v1/*` are noise, not failures). Conclusion: not a
bug in the test or in the entrance routes (no non-core key is used by them). The slowest cases take about 5 s
(`/` 3 s and the non-entrance hold 5 s on this machine); the config timeout is 30 s, so the reported failure fits an
environment problem in the other worktree: a stale or incomplete `node_modules` (the worktree needs
`pnpm install --offline --frozen-lockfile`), or a heavily loaded machine (use `--maxWorkers=2`). No code change.

## Done (ticked in the inventory, with notes)
- J-21 doc rows: doc 05 op 8 row, spec `lounge-voice-video.md` section 5 "Wire details as built", LNG-GLD-029 won't build (D-251); the other rows were already in the docs. Box ticked.
- LNG-GLD-029 closed as won't build (D-251).
- LNG-USET-051: spoilers "on hover for mine", sticker suggestions switch (`StickerSuggestions`), Markdown preview (`MarkdownPreview`).
- LNG-USET-050: Advanced cards: hardware acceleration hint, QoS high packet priority (applied to the mic sender at join), E2EE info and "Show privacy codes" (honoured by the call shield).
- LNG-USET-049: soundboard volume in the Sounds card; incoming ring was already built.
- LNG-USET-035: Cinema audio language and quality cap (the player can apply them), Library page preload count.
- VOX-UI-001, VOX-UI-005: Activities button in the voice panel / call bar (stale boxes).
- LNG-SSET-053: stale; auto-lockdown exists as `raid_auto_pause_invites` (J-02).
- ENT-AUTH-011: link from User Settings, unlink with password, PKCE S256 on all flows; migration 0990 (`external_identities.unlinked_at`).
- VOX-UI-003: per-tile E2EE shield (box stays open for per-tile connection quality).

## Still open (reason)
- FND-DB-009 library samples with CC0 media: needs downloaded media (ask before downloading).
- ENT-UI-026, XC-TEST-005: need Playwright against a live stack (not allowed in this pass).
- LNG-CHN-010 stream preview, LNG-CHN-017 per-channel module switches, LNG-SSET-050/051, LNG-USET-011/036/039: EXT phase or no backend (notes already in the inventory).
- LNG-CMP-007 resumable multipart (> 64 MB): needs multipart endpoints in the media API; large. LNG-CMP-001 Lexical composer: scheduled, D-174.
- LNG-EXP-004 default sticker packs: owner-supplied artwork.
- VOX-E2E-003: owned by the E2EE browser agent. VOX-UI-016 mobile bottom sheet: not started.
- XC-TEST-001 Garage bucket helper (no Windows build, live-S3 suite covers it) and XC-CI-005 preview deployments (optional): left open.

## Decisions
- Unlinking Discord keeps the `external_identities` row (soft unlink via `unlinked_at`): the identity is never freed for another account (D-030) and registration still finds the Person (no ban-evasion path). The same account can relink it.
- QoS is off by default (Discord's default); it only sets sender `priority` and `networkPriority` to `high` at join.
- "On hover for mine" is a third `spoilers` value, `mine`.
- `settings.discord.*`, `settings.advanced.*` keys live in the new fragment `p15-rest-b-1.ts`.

## Tests run
- web: tsc, eslint (touched dirs), prettier --check, vitest for `src/settings/user`, `src/lounge/voice`, `src/voice`, `src/activities`, `src/cinema/player`, `src/library`, firstpaint.
- rust: `cargo test -j 2 -p sr-api --lib routes::auth::tests::discord`, `sr-db --test p15_rest_b -- --include-ignored` on a scratch database.

## Open owner questions
- Should XC-TEST-001 (Garage bucket helper) and XC-CI-005 be closed as won't build?
- Should Discord relink to a different account ever be allowed after an unlink (support request), and by whom?
