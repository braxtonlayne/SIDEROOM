# Handoff: feat/p13-h-a (H-01, H-02, H-03, H-14, H-15, H-16, H-17, H-18, H-43)

Base: integration/phase13. Migrations used: 0870, 0871, 0873, 0879 (0872 and 0874 belong to feat/p13-h-b; 0875 exists on another branch).

## Built
- **H-01** `tv.reminder_lead_minutes` settings key (0 to 120, default 5) read when `?lead_minutes=` is absent; `GET /tv/reminders` (self, session only) lists pending reminders joined to the guide; web "My reminders" list (`RemindersCard.tsx`) and Live TV settings lead.
- **H-02** Empty watch party rooms (migration 0870): media columns nullable, state `idle`, first joiner becomes host, `WATCH_PARTY_UPDATE` carries `media: null`, `change_media` from idle goes to paused; room preset no longer needs a title. Spec updated.
- **H-03** Legacy download fallback removed; migration 0871 drops `cinema_download_grants` behind a guard (`p13_h_a` test covers with and without grants).
- **H-14** Creator is Interested on create (D-228); bots mark nobody. Lounge tests now expect `user_count` 1 and the start notice to go to the creator and Carol.
- **H-15** `Occupancy::Known | Unknown`; events end for emptiness only on Known; join-to-create keeps the room on Unknown; `jobs.reap` at startup and hourly (30 minute threshold).
- **H-16** `reading_progress.cfi` (migration 0873, max 1024), accepted and returned by the progress route.
- **H-17** `library.share_reading` (`off|friends|everyone`) drives the Reading presence audience; web Activity Privacy control. Fixed in this pass: `friends_reading` now also hides readers the viewer blocked (either direction), matching the event audience.
- **H-18** Library export idle sweep (7 days, `last_read_at`) and expired `reading_presence` sweep (migration 0879).
- **H-43** `PUT /admin/library/chapters/{chapter_id}/pages/{index}` (`crates/sr-library/src/page_replace.rs`), keeps position and spread flag, audit row, fingerprint bump; web Replace button in `PagesDialog.tsx`. Route matrix row (staff).

## Fixed while finishing
sr-db payments_repo call signature; p13_h_a test (`state::text`); library_repo cfi test page indexes (chapter has 3 pages); reminders pg test deletes `user_settings` before the account; lounge assertions for the creator; blocked-reader filter in `friends_reading`; unused i18n keys `partyTitleRequired/partyStartsWith/partyChange` removed; prettier.

## Tests run (all pass)
- clippy `-D warnings --all-targets` on sr-tv, sr-library, sr-library-social, sr-watchparty, sr-chat, sr-features, sr-events, sr-media, sr-db, sr-api, sr-server: clean. `cargo fmt --all` clean.
- Lib tests: sr-tv 52, sr-library 130, sr-library-social 6, sr-watchparty 39, sr-chat 304, sr-features 103, sr-events 15, sr-media 109, sr-db 10, sr-api 559, sr-server 47 (48 pg ignored).
- Postgres (scratch DB, `--include-ignored --test-threads=1`): sr-db p13_h_a 2, library_repo 11, library_wiring 16, payments_repo 8, media_repo 8; sr-library-social db 19; sr-watchparty db 23; sr-server lib tv_wiring 4, watchparty 3, feature_actions 8; sr-api lounge 25, guilds 20; route_matrix 5 (no `other` or `?` cells).
- Web: tsc, eslint, prettier clean; vitest 201 files / 2022 tests; vite build; bundle budgets ok; i18n verify MISSING 0, `src/i18n` 46 tests.
- OpenAPI, TS schema and MSW handlers regenerated (810 operations).

## Open
- Reading heartbeat does not yet send the section title as `state` (needs the EPUB reader, H-19).
- Page replace on the web takes a file read to a data URI; there is no staged-upload input.
- Cinema share level control is absent on the web (Library level exists beside the Activity Privacy section only).
