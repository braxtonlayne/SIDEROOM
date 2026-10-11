# 16 — Watch Parties (`WP`)

Phase 5. Spec: [watch-parties](../specs/watch-parties.md).

## WP-BE — Backend
- [ ] **WP-BE-001** [DB] parties, party_members, party_queue
- [ ] **WP-BE-002** [BE] Create party (bound to channel / DM / private link), join, leave, end, transfer host, settings (shared control, queue permissions, auto-pause on buffering)
- [ ] **WP-BE-003** [BE] Server-authoritative playback state + control op validation (host / CONTROL_WATCH_PARTY)
- [ ] **WP-BE-004** [GW] Op 100 WATCH_PARTY_SYNC (clock ping/pong, state, control requests), `SR_PARTY_STATE` dispatch
- [ ] **WP-BE-005** [BE] Buffering aggregation → auto-pause / continue-after-timeout
- [ ] **WP-BE-006** [BE] Queue (add, reorder, remove, auto-advance)
- [ ] **WP-BE-007** [BE] Limits by server level; age-restriction checks for all members
- [ ] **WP-BE-008** [BE] System message with live-updating party card in the bound channel
- [ ] **WP-BE-009** [BE] Watch Party channel type behavior (persistent theatre, idle end after 5 min empty)
- [ ] **WP-BE-010** [BE] Live TV parties (no seek; follow live edge)

## WP-FE — Frontend
- [ ] **WP-FE-001** [FE] Clock-offset estimator + drift correction (rate nudging 0.95/1.05, hard seek > 2s)
- [ ] **WP-FE-002** [FE] Theatre layout (player + right panel tabs Chat / Queue / People)
- [ ] **WP-FE-003** [FE] Host controls & shared-control states (disabled controls for non-controllers, "Request control")
- [ ] **WP-FE-004** [FE] Queue panel (add from search, drag reorder, remove)
- [ ] **WP-FE-005** [FE] People panel (host badge, buffering indicators, kick for host)
- [ ] **WP-FE-006** [FE] Reactions overlay (floating emoji, rate-limited)
- [ ] **WP-FE-007** [FE] Ready check modal
- [ ] **WP-FE-008** [FE] "Watch together" entry points (title page, Live TV, voice call bar "Start Watch Party", message card)
- [ ] **WP-FE-009** [FE] Destination picker modal (channels you can post in, DMs, private link)
- [ ] **WP-FE-010** [FE] Party card in chat (now playing, viewers, Join)
- [ ] **WP-FE-011** [FE] Mini player + voice continuity when navigating sections
- [ ] **WP-FE-012** [FE] Webcam bubbles overlay (optional, from the voice call)
- [ ] **WP-FE-013** [TEST] 10-client drift simulation (±300 ms target)
