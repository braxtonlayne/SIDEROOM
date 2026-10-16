# Handoff: feat/p14-misc (H-10 clashes, LNG-MSG-015/016/019, admin/donations/review audit)

Branch `feat/p14-misc` off `integration/phase14` (with `feat/p14-fu-gw` merged). No migrations, no new routes (so `route_matrix.tsv` is unchanged).

## Done

- **H-10 schema-name clashes (all 17).** Every clashing name reached the served document with one type winning, so the generated client was wrong for the other. Renamed with `#[schema(as = ...)]` (the more widely used type keeps the name):
  `DeveloperApplicationObject`, `DeveloperAuditEntryObject`, `AdminBanBody`, `ReadingBookmark`, `ReadingBookmarkEntry`, `AdminChangeEmailBody`, `MemberCreateInviteBody`, `DirectoryDecisionBody`, `LibraryFollowBody`, `CinemaInstantSearch`, `CinemaMergeBody`, `MediaWorkerPartUrl`, `AdminReauthBody`, `AdminRenameBody`, `LibraryUploadRequest`, `LibraryUploadSlot`, `MessageUploadSlot`, `GuildUserRef`. (`sr-voice-proto` `UserRef` does not derive `ToSchema`.) `KNOWN_CLASHES` is empty. `openapi.json`, `schema.d.ts` and the MSW handlers were regenerated (the diff also carries drift from earlier merged work); the web had no callers of the generated names (it uses hand-written types), typecheck is clean. `routes/library/tests.rs` updated for the new names.
- **LNG-MSG-019** `sr-chat/src/voice_meta.rs`: pure-Rust Ogg Opus walker (no new dependency). Duration = last granule minus pre-skip at 48 kHz; waveform = envelope (1 to 256 bytes) from Opus packet sizes, since symphonia has no Opus decoder. Run at attachment finalize for `audio/ogg` up to 8 MiB; the client's declared duration and waveform now only fill in when the file cannot be read. Tests: unit (5) and `voice_duration_and_waveform_are_measured_from_the_file`.
- **LNG-MSG-015** Suppression flag was built; spoiler handling added: a link preview whose link is written only inside `||...||` gets `sr_spoiler: true` (`extract_preview_links_marked`), client embeds can never set it, the web hides such a preview behind "Spoiler: show link preview".
- **LNG-MSG-016** New types: follow added (12), server support (8 to 11 by level). `POST /channels/{id}/followers` posts the follow notice in the receiving channel. A credited server-level payment posts the support notice in the system channel (honours `SUPPRESS_PREMIUM_SUBSCRIPTIONS`); wired through a new `PaymentEvents::server_supported` and a late-bound `ChatSlot`.
- **ADM-DASH-001, ADM-DASH-003, ADM-PLT-006 (partial)** Setup checklist and System page read the `integrations` list; "Send a test email" button.
- **DON-LB-002** avatars on leaderboards. **DON-UI-006** ticked after audit. **REV-CORE-007** submitter system DM per decision (`ReviewEvents::dm`, `DmSlot`).
- i18n fragment `p14-misc-1.ts`. Inventory ticks and notes in 05, 10, 11, 12, 15, 16.

## Still open (reasons)

- LNG-EXP-004 install route (artwork is owner-supplied; read path is complete). ADM-CNT-001 Packages tab (no package system). ADM-PLT-004/005 (default theme per section, editable integrations: owner decisions). ADM-PLT-006 backups and disk probes. ADM-GRAPH-003 Canvas 2D graph. DON-UI-005 patron badge/ring (needs a member-visible patron flag and toggle). DON-UI-001 public `/support` page.
- 12-review editor items (REV-ED-001..004, REV-RV-002 edit-in-place): large front-end work, not started.
- 15-live-tv TV-ENG-001/003/007, TV-HLS-003/005, TV-ADM-003/004 (guide import and slots; filler media; needs real players).
- 16-watch-parties WP-BE-008, WP-FE-008/010/012 (live-updating card message, webcam bubbles).
- 18-cross-cutting XC-TEST-001 (Garage bucket helper, no Windows build), XC-TEST-005 (needs Playwright against a live stack), XC-CI-005 (optional).

## Tests run

cargo fmt --check; clippy -D warnings (-j 2, --all-targets) on sr-api, sr-chat, sr-markdown, sr-payments, sr-review, sr-library, sr-cinema, sr-media, sr-server; `cargo test` for sr-api (all non-ignored incl. openapi_names, route_matrix), sr-chat, sr-markdown, sr-payments, sr-review, sr-library, sr-cinema, sr-media. Postgres suites not run (no PG code changed). Web: typecheck, eslint, prettier --check, full vitest at --maxWorkers=2 (2035 of 2038 pass; the misses were load timeouts that pass alone, the wasm-dependent channelPerms suite which passes after `pnpm --filter @sideroom/wasm build`, and `vitestVersion` which needs vitest 3 from the pnpm install), build and bundle budget (all within budget).

## Owner questions

- Voice waveform is a packet-size envelope, not decoded PCM amplitude. Acceptable, or add an Opus decoder crate (a download) for true amplitudes?
- Should ADM-PLT-004/005 become editable in the UI, or stay operator config?
