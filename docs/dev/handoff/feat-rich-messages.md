# Handoff: feat/rich-messages (paused)

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/rich, branch feat/rich-messages (base integration/phase1).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/rich-target. Planned DB: `sr_rich` (NOT created yet), ports 8103/9103.
Migrations reserved: 0120-0129 (0120 written). Singleton job ids reserved for me: 120-129.

## Done (verified by unit tests, `cargo test -p <crate>`)
- crates/sr-unfurl (new): SSRF policy table, pinned-IP fetcher with injectable Resolver/HttpTransport,
  max 3 redirects revalidated, content-type allow-list, reqwest+rustls transport, scraper head parser
  (bounded: 128 KiB / 3000 tags), oEmbed-lite, 24h moka cache, per-host limiter. 25 tests pass. Committed.
- sr-chat: Embed.provider + is_unfurled(), Poll expiry_ms/finalized, PollEmoji, ForwardSnapshot,
  MessageReference.forward, Attachment.duration_ms/waveform, embed_rules (limits), poll_rules,
  rich_repo::RichStore trait + memory impl, SendMessage.forward/ForwardRequest, voice-shape rule.
  181 sr-chat tests pass (before the last small edits).
- sr-markdown: extract_preview_links (skips `<url>`), tests pass.
- sr-db: migration 0120 (messages.poll_expires_at + index, stickers.guild_id nullable), PgRichStore
  (src/rich_repo.rs), chat_repo reference snapshot + poll_expires_at insert + attachment duration/waveform.
  Compiles; NOT run against Postgres.
- `cargo check -p sr-api --all-targets` passes at the last commit (send.rs gets `forward: None`).

## In progress / state
Everything is committed (wip commit). sr-api behaviour is unchanged so far: no new routes yet.

## Remaining checklist
- [ ] config: `[unfurl]` in sr-core Config (enabled, deny_hosts, allow_private_for_tests=false loudly documented, per_host_per_minute)
- [ ] sr-api messages: new files only: rich.rs (RichDeps: store, JobScheduler trait, Unfurler, UnfurlGate), field `rich` on MessagesDeps + with_rich()
- [ ] send.rs: enable poll (plan_poll, SEND_POLLS already checked in chat), sticker_ids (<=3, same guild or guild_id NULL, available),
      voice message validation (audio/ogg, <=120s, base64 waveform <=256 bytes, size cap), AttachmentRef.duration_secs/waveform,
      embeds via validate_embeds, allowed_mentions conflicts/duplicates/100 cap, forward (message_reference.type=1: check source view+history, snapshot, empty body)
- [ ] model.rs: MessageObject.poll (+results), sticker_items, message_snapshots, message_reference.type, AttachmentObject.duration_secs/waveform; render.rs: fetch tallies/stickers
- [ ] poll routes: PUT/DELETE /channels/{c}/polls/{m}/answers/{a}/voters/@me, GET .../answers/{a}, POST .../polls/{m}/expire; mount in mod.rs + openapi
- [ ] events: DomainEvent::PollVoteAdded/Removed (+topics, dm_channel), gateway MESSAGE_POLL_VOTE_ADD/REMOVE, intents GUILD_MESSAGE_POLLS 1<<24 (DM 1<<25), redact message_snapshots in hub.rs redact_for
- [ ] jobs (sr-server): messages.unfurl (payload channel_id,message_id), messages.poll_finalize (delayed per poll), singleton messages.poll_sweep (id 120, ~30s); JobScheduler over sr_db::jobs::enqueue; wire PgRichStore, Unfurler, feature `unfurl` (default on) in app.rs; Arc<MessagesDeps> shared with handlers
- [ ] unfurl job: skip DMs (no per-user flag; documented), honour SUPPRESS_EMBEDS, replace only is_unfurled embeds, MESSAGE_UPDATE via DomainEvent::MessageEdited; re-run on edit
- [ ] tests: tower oneshot for polls/stickers/voice/embeds/forward; Postgres #[ignore] tests (tests/rich_store.rs); job handler tests with fake transport
- [ ] live verify with DB sr_rich, node script (style of scratchpad smoke2.mjs), local test page needs unfurl.allow_private_for_tests
- [ ] fmt, clippy -D warnings, cargo test --workspace, ignored PG tests (SR_TEST_DATABASE_URL, --test-threads=1), cargo deny check, OpenAPI regen (bash)
- [ ] tick inventory: LNG-MSG-007, -010, -014, -015, -019 only when truly done

## Decisions / notes
- Unfurled embeds are marked by provider presence or type != rich; client embeds are normalised to type rich, provider/video stripped.
- Image proxying not planned; thumbnails reference remote URLs directly (privacy caveat).
- Duration is client-declared (no server-side symphonia decoding).
- Forward snapshots reference source attachments rather than copying objects.
- Tooling: no python on this machine; perl one-liners with complex quoting are fragile, use the Edit tool.
- cargo check --workspace may exceed 2 min on first build (aws-lc-sys); run in background.

## Processes
No sr-server, node or test server was started. DB sr_rich not yet created; shared sr_test untouched.
