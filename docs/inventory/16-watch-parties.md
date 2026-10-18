# 16 — Watch Parties (`WP`)

Phase 5. Spec: [watch-parties](../specs/watch-parties.md).

## WP-BE — Backend
- [x] **WP-BE-001** [DB] parties, party_members, party_queue (migration 0480)
- [x] **WP-BE-002** [BE] Create party (bound to channel / DM / private link), join, leave, end, transfer host, settings (shared control, queue permissions, auto-pause on buffering) (crates/sr-watchparty and the /watch-parties routes)
- [x] **WP-BE-003** [BE] Server-authoritative playback state + control op validation (host / CONTROL_WATCH_PARTY)
- [x] **WP-BE-004** [GW] Op 100 WATCH_PARTY_SYNC (clock ping/pong, state, control requests), `SR_PARTY_STATE` dispatch (built as HTTP clock and heartbeat endpoints plus WATCH_PARTY_UPDATE and WATCH_PARTY_PLAYBACK dispatches; gateway op 100 stays reserved, see the spec section 7)
- [x] **WP-BE-005** [BE] Buffering aggregation → auto-pause / continue-after-timeout
- [x] **WP-BE-006** [BE] Queue (add, reorder, remove, auto-advance)
- [x] **WP-BE-007** [BE] Limits by server level; age-restriction checks for all members _(p12: the party limit follows the server level, 2/4/8/16, `EntitlementLimits` over `Entitlements`; Postgres test with a Level 2 server)_
- [x] **WP-BE-008** [BE] System message with live-updating party card in the bound channel *(p15 rest-c: message type 200 `MessageType::WatchParty`, `sr-server/src/party_cards.rs` posts and rewrites the card, `AnnouncementKind::Updated` from the party service, table `watch_party_cards` (migration 1001); tests `party_cards.rs`, `sr-watchparty` Postgres suite)* _(earlier: an opt-in Watch Party announcer posts a start and end text as a bot message (userfeat announcers); it stays)_
- [x] **WP-BE-009** [BE] Watch Party channel type behavior (persistent theatre, idle end after 5 min empty) _(D-208: no channel type; the "Watch party room" preset (`sr_watch_party_room` on channel create, create-channel dialog) makes a voice channel with a persistent bound party; a non-persistent bound party ends 5 minutes after it empties)_
- [x] **WP-BE-010** [BE] Live TV parties (no seek; follow live edge) _(p12: `tv_channel_id` on create and change media, `PlayGate::resolve_live` answered by `sr-tv`; tests in `sr-watchparty` db.rs and `sr-tv` service_tests)_

## WP-FE — Frontend
- [x] **WP-FE-001** [FE] Clock-offset estimator + drift correction (rate nudging 0.95/1.05, hard seek > 2s) (apps/web/src/watchparty/sync.ts; the ignore band is 100 ms, not 250 ms, see the handoff)
- [x] **WP-FE-002** [FE] Theatre layout (player + right panel tabs Chat / Queue / People)
- [x] **WP-FE-003** [FE] Host controls & shared-control states (disabled controls for non-controllers, "Request control")
- [x] **WP-FE-004** [FE] Queue panel (add from search, drag reorder, remove)
- [x] **WP-FE-005** [FE] People panel (host badge, buffering indicators, kick for host)
- [x] **WP-FE-006** [FE] Reactions overlay (floating emoji, rate-limited)
- [x] **WP-FE-007** [FE] Ready check modal
- [x] **WP-FE-008** [FE] "Watch together" entry points (title page, Live TV, voice call bar "Start Watch Party", message card) *(p15 rest-c: the message card `watchparty/PartyMessageCard.tsx` now offers Join)* (the title page, the voice call bar, Live TV (guide now panel, live player) and the message card)
- [x] **WP-FE-009** [FE] Destination picker modal (channels you can post in, DMs, private link)
- [x] **WP-FE-010** [FE] Party card in chat (now playing, viewers, Join) *(p15 rest-c: `PartyMessageCard.tsx` in `MessageItem`; one shared party state per channel in `useChannelParty.ts`)* (the strip at the top of the channel stays)
- [x] **WP-FE-011** [FE] Mini player + voice continuity when navigating sections
- [ ] **WP-FE-012** [FE] Webcam bubbles overlay (optional, from the voice call)
- [x] **WP-FE-013** [TEST] 10-client drift simulation (±300 ms target) (apps/web/src/watchparty/__tests__/drift.sim.test.ts)
