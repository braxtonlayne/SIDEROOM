# Handoff: feat/p11-msgs (Lounge messages, expression, notifications)

Scope: `docs/inventory/05-lounge-messages.md`. Branch `feat/p11-msgs` off `integration/phase11`. Migration used: `0670_web_push.sql`.

## Done

Audit result first: reactions, components storage, crosspost, gateway events, emoji and sticker CRUD with `GUILD_EMOJIS_UPDATE` / `GUILD_STICKERS_UPDATE`, and the search indexing jobs already existed; those boxes are ticked with a note naming the file.

New work:

- **LNG-MSG-021/022/023** (`crates/sr-server/src/jobs.rs`): the purge now deletes stored attachment objects before the rows (a message whose objects fail to delete is retried, never orphaned). New hourly singleton `attachments.expire_large` deletes the object, marks the row `expired` and dispatches `MESSAGE_UPDATE`. Both jobs take `now_ms`; `retention_clock_tests` (Postgres) move the clock to the exact boundary (30 days, 7 days) and cover message holds and attachment holds.
- **LNG-NTF-003** (`crates/sr-push`, `routes/push.rs`, `sr-server/src/push_notify.rs`, `push_pg.rs`, migration 0670): VAPID key (generated once, sealed with the server secret; per-boot when no secret is configured), RFC 8291 `aes128gcm` encryption (verified against the RFC test vector), SSRF-safe delivery through `sr-net`, `GET /push/vapid-key`, `POST|DELETE /users/@me/push-subscriptions` (10 devices per user), fan-out consumer on the event firehose with per-channel collapse keys (`Topic`). Own implementation on RustCrypto + rustls instead of the `web-push` crate (that crate pulls hyper/openssl-era dependencies).
- **LNG-NTF-002** (`sr-chat/src/social/notifications.rs`): `decide()` combines level, mute, suppress everyone/roles/highlights, direct mention, reply, role, thread follow, keyword, DM and `mobile_push`; `SocialService::decide_notification` loads the scope. The push consumer uses it per recipient after a view-permission check.
- **LNG-EXP-002/003**: uploads are shrunk to 128 px (emoji) / 320 px (static PNG and GIF stickers).
- **LNG-EXP-005** (`routes/gifs.rs`): Tenor v2 proxy (`/gifs/config|trending|search|categories`), 60 s cache, previews through the signed external image proxy. Off unless `[gif] provider = "tenor"` and `gif.api_key` (`SR_GIF__API_KEY_FILE` works) are set.
- **LNG-NTF-006 (partial)**: `GET /users/@me/mentions` (7 days, role/everyone/server filters, permission-filtered) and the inbox popout with Mentions and Unreads tabs.
- Frontend: GIF picker (EXP-009), emoji picker upgrades (EXP-007), notification dialog additions and bell levels (NTF-004/005), desktop notifications with sounds and DND (NTF-007), tab title and favicon badge (NTF-008).
- Route matrix rows, OpenAPI, api-client types and MSW mocks regenerated.

## Still open (and why)

- LNG-MSG-015: spoiler handling inside embeds not built (the suppress flag is).
- LNG-MSG-016: the join notice has no randomised lines; pin and call records exist and thread-created is emitted by the threads module, "follow added" and "server support contribution" do not.
- LNG-MSG-019: no server-side waveform/duration (`symphonia` not added; the client supplies them).
- LNG-EXP-001/004: no `sticker_packs` table and no default packs. Needs original artwork and a send-path change so message creation accepts non-guild sticker ids.
- LNG-EXP-011, LNG-EXP-012: custom emoji popout and burst reaction variant not started.
- LNG-NTF-006: "For You" tab not built. LNG-NTF-009 (optional DM toasts) not built.
- LNG-MSGUI-018, LNG-CMP-005 (client-side size check with Patron hint; the rest exists), LNG-CMP-007 (multipart resume needs server endpoints), LNG-CMP-001 (Lexical rewrite, skipped as instructed).
- Keyword highlights: the engine supports them but there is no stored per-user keyword list or UI.
- EXP-007: the category sidebar with server icons is not built (sections are stacked).
- Klipy provider not implemented (Tenor only).
- Push is sent even when the user has a live gateway session on another device (no "active elsewhere" suppression yet).

## Tests run

- `cargo test -p sr-core -p sr-push -p sr-chat -p sr-api` (all green, `--include-ignored --test-threads=1`), route matrix regenerated.
- `cargo test -p sr-server --lib` with `SR_TEST_DATABASE_URL`: all new Postgres tests pass (retention clock tests, push store/audience, mentions query). Four pre-existing tests (`gateway_pg` x2, `voice_pg_tests` x2) fail here only because the shared `sr_test` database already holds migration 660 from another agent's branch (`VersionMissing(660)`); they use that shared DB and are unrelated.
- `cargo fmt --check`, `cargo clippy -D warnings` on sr-core, sr-chat, sr-push, sr-api, sr-server.
- Web: prettier, eslint, tsc, full vitest (1497 passed), build and bundle budget.

## Decisions

- VAPID key stored sealed in `push_vapid_key`, not operator config; GIF provider key is operator config.
- Push content (title, 140-char body) is end-to-end encrypted to the browser; the push service sees only ciphertext.
- Emoji/sticker images that the pipeline cannot decode (animated WebP, APNG, truncated files) are stored as uploaded rather than rejected, because the CDN re-validates each variant.
- Bots are denied on `/push/`, `/gifs/`, `/users/@me/push-subscriptions`, `/users/@me/mentions`.

## Owner questions

1. Should `sticker_packs` default packs ship as procedurally drawn original artwork, or will artwork be supplied?
2. Add Klipy now, or is Tenor enough?
3. Should push be suppressed while the user has an active gateway session (Discord does)?
