# Handoff: feat/rich-messages (implementation complete, gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/rich, branch feat/rich-messages (base integration/phase1).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/rich-target, use CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0.
Scratch DB `sr_rich` (left in place, migrated), live scripts in C:/Users/braxt/Desktop/SIDEROOM_wt/rich-live (env.sh, live.mjs). Port 8103/9103.

## Done
- sr-unfurl crate: SSRF policy table, pinned-IP fetch, 3 redirects revalidated, resolver/transport injectable, rustls only,
  scraper head parser, oEmbed-lite, 24h cache, per-host limiter. `[unfurl]` config (deny_hosts, per_host_per_minute,
  allow_private_for_tests=false: TEST ONLY, logs a warning at boot) and the `unfurl` feature module (default on).
- Message create: polls (2-10 answers, <=168h, multiselect), stickers (<=3, same guild or standard), voice messages
  (audio/ogg, <=120s, waveform 1..256 bytes, no other content), forwards (`message_reference.type=1`), embed validation,
  allowed_mentions validation. Payload: poll+results, sticker_items, message_snapshots, attachment duration_secs/waveform.
- Endpoints: PUT/DELETE /channels/{c}/polls/{m}/answers/{a}/voters/@me, GET .../answers/{a}, POST .../polls/{m}/expire.
- Events/gateway: MESSAGE_POLL_VOTE_ADD/REMOVE (intent 1<<24), MESSAGE_UPDATE on finalise and on previews.
- Jobs: messages.unfurl, messages.poll_finalize (delayed per poll), messages.poll_sweep singleton id 120 (30 s).
- Migration 0120. OpenAPI client regenerated.
- Gate: fmt, clippy -D warnings, cargo test --workspace (71 suites ok), cargo deny ok, all #[ignore] PG tests ok on sr_rich.
- Live (sr-server + sr_rich): polls, votes, gateway events, sweep finalise, previews of a local page, image embed,
  `<url>` skip, content-type reject, SUPPRESS_EMBEDS, stickers, forward, validation; with the flag off loopback is not fetched.

## Not done / caveats
- Postgres ignored tests could NOT be run on shared sr_test: it already holds migration 130 from another branch
  (sqlx VersionMissing(130)). They pass on sr_rich. Re-run on sr_test after the branches merge.
- DMs are never unfurled (no per-user privacy flag). No image proxy (thumbnails point at the remote URL).
- Voice duration is client-declared (no symphonia decoding); forwards reference source attachments (not copied).
- Inventory: ticked LNG-MSG-007, -010 only. -014 (no image proxy / allow list), -015 (no spoiler handling), -019 (no server-side decode) stay open.
- Poll vote via bulk `answers/@me` (Discord client shape) not implemented. Poll emoji usability is not checked.
- The server process dies if started as a plain background child of the tool shell; start detached (PowerShell Start-Process).
