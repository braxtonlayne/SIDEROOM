# Handoff: fu-rust (FU-7, FU-3, FU-13, FU-5, FU-8, FU-9, FU-2, FU-18)

Worktree `SIDEROOM_wt/fu-rust`, branch `fix/fu-rust` (on `integration/fu`). Not pushed.

## Done (commit subjects start with the FU id)

| FU | What |
|---|---|
| FU-7 | `audio/webm` voice messages: `sr_chat::voice_meta::analyze_webm_opus` walks the WebM (unknown-size segment/cluster as `MediaRecorder` writes it), needs exactly one `A_OPUS` audio track, no lacing or encryption; duration = sum of Opus packet lengths, envelope from packet sizes. `attachments.rs` turns a sniffed `video/webm` declared `audio/webm` into `audio/webm`; `validate_voice` accepts it; `/cdn` serves it. Recorder: Ogg first, WebM fallback (`voice.ts`, `VoiceRecorder.tsx`, `AudioTool.tsx`). |
| FU-3 | `POST /admin/arcade/coming-soon/{entry_id}/release` (staff + OPERATOR, audited, optional `application_id`): `ArcadeService::release_coming_soon` removes the entry and returns title + watchers in one transaction; each watcher gets one `SystemDm` (link `<public url>/games`). Returns `{watchers, notified}`. `ArcadeDeps` gained `dm` (a late-bound slot, like review) and `public_url`. Admin button "Release {title}". Route matrix + authz matrix rows (the release route has its own seeded entry). |
| FU-13 | `top_frames` on `CrashSummary` (first three card frames as `fn file:line`). The admin list now shows them in a row under each crash. |
| FU-5 | bech32/bech32m verification of `bc1` addresses in `sr-core` `tips.rs` (version/checksum pairing, program length, padding), mirrored in `apps/web/src/arcade/tips.ts`. Base58check NOT done (see below). |
| FU-8 | Setting `invites.card_claimants_retention_days` (0..3650, default 30, 0 = never), job `cards.purge_claimants` (id 1671, every 6 h), migration `2320_invite_card_claimants_purged.sql` (`going_final`, `claimants_purged_at`). The count is written in the same statement that deletes the claim rows; a purged card shows "N went. The list of names is not kept." |
| FU-9 | `{{house_notes}}` in the build template (`scripts/render-legal.mjs`), filled by `web.rs` (`house_notes_html`, `fill_legal`, new `OperatorInfo::house_notes`) with the staff copy escaped as text; same in the Vite dev server (`publicDev.ts`). |
| FU-2 | Postgres test `a_bare_shelf_delisting_never_counts_toward_the_repeat_infringer_counter` (`crates/sr-moderation/tests/storage_takedown.rs`). No code change. |
| FU-18 | `tipQr` on `BlocksEnv` (`TipQr` type), Linktree draws it for `btc`/`lightning`; `ProfileRenderer` passes `QrBox` unless `compact`; `PublicProfile` passes none. profile-blocks imports no encoder; entrance isolation green. |

Also: OpenAPI, api-client and MSW handlers regenerated (`mocks:check` green). Found in the visual check and fixed: the crash frames column pushed the table past the page; the voice recorder's Stop did nothing in development because StrictMode's first-mount cancel stuck (`cancelled.current = false` on mount).

## Not done / limits

- **Base58check** (legacy `1...`/`3...` Bitcoin addresses): `sha2` is not in `[workspace.dependencies]` (each crate pins it, and `sr-core` has none; `sr-core` also builds to wasm), so per the decision that half waits for the owner. Legacy addresses stay shape-only.
- `packages/profile-blocks/src/url.ts` has its own older `bc1` regex (no checksum); not touched, to keep the public `/~handle` bundle small. The server already refuses a bad checksum on save.
- The voice upload could not be completed end to end in the browser: the test stack uses `memory:` storage, which a browser cannot PUT to ("The upload could not be completed"). The recorder, WebM preview and trim UI were checked live; the server side is covered by the unit tests (`voice_meta`, `validate_voice`) but not by a real upload of a Chromium file.
- The release DM is sent through the Staff Server voice like other system DMs; it does not consult the member's DM settings separately (P32-UTL-3) beyond whatever that sender does.

## Routes shot (`C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep-shots\fu-rust`, desktop 1280x800 and phone 375x812, light and dark)

`admin-arcade`, `admin-crash-reports` (before: first shots show the clipped table; after: frames row), `admin-entrance`, `pub-house-notes` (with a staff copy containing markup, shown escaped), `profile-u` (Owner page with Bitcoin QR, Ko-fi, plain link), `lounge-channel`; flows in `flows\` (voice recorder recording/preview, Release result "1 of 1 watchers were told"). The sweep flags that remain are old and expected: 404 for the hero image (none uploaded), a generic 404 in the Lounge channel, and `profile-member` (the Owner cannot see that member's page; the sweep now seeds the tip links on the Owner's page instead). New sweep rows: `admin-arcade`, `admin-entrance`, plus seeds for a Coming Soon watcher and tip links.

## Tests run

cargo: `sr-chat` (389), `sr-core tips`, `sr-admin settings`, `sr-db secret_columns`/`migration_versions`/`cards_repo`, `sr-activities arcade`, `sr-moderation storage_takedown`, `sr-server web_gate` (23) and `cards_pg_tests` (15) with a scratch DB, `route_matrix`, `immutable_rules` (25), `authz_matrix` (11 passed), `cargo fmt --check`, `clippy -D warnings --all-targets` on the touched crates. Web: tsc, eslint and prettier on changed files, vitest for arcade, profilepage, profile-blocks, `vite/publicDev`, admin crash; `check-css-tokens`, `i18n-audit` (0), web build, `check-entrance-isolation`, `check-bundle-budget` all green. The real `sr-server` was run with the built web root and `/house-notes` showed the staff copy escaped.

## Owner questions (defaults taken)

1. Release DM text is one line ("{name} is out. You asked to hear when it shipped: find it in the Arcade, {url}/games"); wording is a placeholder. `application_id`, when given, only names the game from its Arcade candidate record.
2. FU-8 counts a card as finished from `closed_at` (6 h after start) or `canceled_at`, whichever is set. Right?
3. House-notes copy is shown as plain escaped text (markdown marks visible), as FU-9 says. Should it later be rendered as restricted markdown at build/serve time?
4. Base58check: allow `sha2` into `sr-core` (or the workspace dependency table)?
5. The staff copy sits after the intro paragraph, before "The house rules". Move it?
6. WebM voice files are stored as `audio/webm` with extension `.webm`; the Lounge player already plays any audio type the browser supports, but old clients that only look for `.ogg` would not recognise them.
