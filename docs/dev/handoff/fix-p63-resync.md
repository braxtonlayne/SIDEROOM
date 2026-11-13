# Wave 63 handoff: resync (SEC-AUD-161), lost gateway dispatches after bus lag

Design only. No code, no test, no build, no stack, no database was touched. Base `integration/phase62` at `a625b943`. Read on 2026-11-12.

Legend: **read** = I read it in the code at `file:line` on this base. **assumed** = not read, stated as a working assumption. **not found** = I looked and there is nothing.

## Summary

- The task brief assumes the repair is small because the web client already handles INVALID_SESSION. Reading the server side says otherwise: **the server never sends INVALID_SESSION for a live session** (only after a failed RESUME, `conn.rs:487`), and a socket that already has a session answers a second IDENTIFY with close 4005 (`conn.rs:407-411`). Worse, **a full re-identify ends the member's voice call**: a voice state is bound to the session id that joined (`voice.rs:845`, `:1258-1275`), and the old session is gone or detached after the identify.
- Recommended: **resync in place**. The server re-sends state to the session that is already connected (same socket, same session id, sequenced, resume-safe): a fresh READY plus GUILD_CREATEs for a user-topic lag (the client already applies exactly that, with stale-marking of the open channel's messages), and a fresh GUILD_CREATE plus a new marker dispatch `SR_RESYNC` for a server-topic lag. A queue with a rate cap and a per-key floor sits in front of it.
- Rejected: INVALID_SESSION d:false (Discord's way), because of voice, the connection cap and a 4005 detour. Its cost and the cases where it would still be right are in section 4.
- Size: one builder, one wave (Rust half about 350 lines plus tests, web half about 120 lines plus tests). The Rust half is the long pole.

---

## 1. Where lag is seen and where it is swallowed

The bus (`crates/sr-events/src/bus.rs`) is an in-process `tokio::broadcast` per topic, capacity 256 (`DEFAULT_CAPACITY`, `:57`), firehose 4096. **read.** Topics: Platform, Firehose, Guild, Channel, User (`event.rs:10-26`). Two ways to subscribe:

- `subscribe()` (`bus.rs:118`): `filter_map(|r| r.ok())` drops the `Lagged` item **silently**. read.
- `subscribe_lagging()` (`bus.rs:133`): maps it to `Err(missed)`. read.
- `LagTapBus` (`sr-server/src/lag_bus.rs:44`): a second broadcast of every published event (capacity 4096) whose items are `Result<_, Lagged>`; `app.rs` hands `lag_bus.tap()` to six consumers. read.

Nothing subscribes to `Topic::Channel` outside tests (grep of non-test code). read.

| # | subscriber (file:line) | stream | on lag today | can a lost event leave a client or cache wrong? |
|---|---|---|---|---|
| 1 | gateway platform pump `sr-gateway/src/hub.rs:669` | `subscribe_lagging(Platform)` | coalesces the gap, `reconcile_memberships()` (`:680-700`, `:1379`): re-reads each connected user's guilds, `GUILD_CREATE` for new, `GUILD_DELETE` for gone (read) | Membership: repaired (D-620). **Not repaired:** a lost `PlatformBroadcast` (`SR_FEATURE_UPDATE` from `sr-features/src/service.rs:364`, `SR_STAFF_ALERT`, `SR_LIBRARY_UPDATE`, `SR_CINEMA_UPDATE`, `SR_INVITE_CREDITS_UPDATE`, `SR_USER_UPDATE`: read) leaves a client with old feature flags, announcements and so on until reload. |
| 2 | gateway guild pump `hub.rs:963`, `on_guild_lag` `:1002` | `subscribe_lagging(Guild(g))`, one pump per guild per node | `schedule_list_refresh(guild, true)`: member-list rebuild only (read) | **Yes, everything else**: channel create/update/delete, role and permission changes, messages, edits, deletes, reactions, voice states, threads, typing, guild settings. The comment at `:1000` says so. |
| 3 | gateway user pump `hub.rs:984`, `on_user_lag` `:1010` | `subscribe_lagging(User(u))`, one pump per connected user | `send_recheck()` to each session, `clear_age_cache`, `clear_safety_cache` (read) | **Yes**: DMs and DM edits/deletes, read-state acks (unread marks), relationship events, self events, call events, entitlement and settings updates. |
| 4 | `FeatureService::spawn_invalidation_listener` `sr-features/src/service.rs:398` | `subscribe_lagging(Platform)` | drops the whole flag cache (read) | No (cache refilled on next read). |
| 5 | `FeatureRuntime::spawn` `sr-features/src/runtime.rs:120` | `FirehoseDedup::subscribe` -> `subscribe` (silent) | nothing | A lost `ChannelDeleted` skips `forget_channel` (`runtime.rs:84-88`): module settings rows of a deleted channel stay. Low. Other module reactions: **assumed** idempotent or tick-covered (not read). |
| 6 | `sr-activities/src/voice.rs:81` | `subscribe(Firehose)` (silent) | nothing | A lost `VoiceLeft` leaves the member in an activity's participant list (`service.voice_left` is only called from this event; no sweep found). Low. |
| 7 | `integration_policy::spawn_subscriber` `sr-server/src/integration_policy.rs:136` | `subscribe(Platform)` (silent) | nothing | **Yes, a stale security setting**: the policy (unfurl deny/allow hosts, rate) is rebuilt only on a matching `FeatureSettingsChanged`; a lost one keeps the old policy until the next change or a restart. Not in scope here; one-line fix (use `subscribe_lagging` and `refresh` on `Err`). Flagged in section 7. |
| 8 | `staff_feed::spawn` `sr-server/src/staff_feed.rs:1507` | `subscribe(Platform)` (silent) | nothing | **Yes**: a lost staff alert never becomes a card or an email. Flagged, not in scope. |
| 9 | search `search_jobs.rs:244` (tap, `app.rs:1116`) | tap, `Err` seen | `clear_access()`, flush, rate-limited `search.reindex` (5 min floor, `:195-235`) (read) | Repaired (SEC-AUD-140). Model for the floor in section 4. |
| 10 | push `push_notify.rs:78` (tap, `app.rs:1354`) | tap, `let Ok(ev) = item else { continue }` | skips (comment: "only loses notifications that are by now stale") | A missed push. Accepted by the code's own comment. |
| 11 | voice event listener `events_jobs.rs:65` (tap, `app.rs:1352`) | tap, `let Ok(..) else continue` | skips | A lost `VoiceJoined` does not start the voice/stage event. **assumed** covered by the event clock (not read). |
| 12 | Discord bridge `bridge_wiring.rs:123` (tap, `app.rs:1408`) | tap | `warn!` "some messages were not mirrored" (read) | **Yes**: messages lost to Discord, no repair. Flagged, not in scope. |
| 13 | bridge sync `bridge_sync_wiring.rs:353` (tap, `app.rs:1522`) | tap | ignores `Err`; comment: scheduled reconcile looks at everything | No (read, by the comment). |
| 14 | scrapbook cascade `scrapbook_jobs.rs:446` (tap, `app.rs:1643`) | tap | `warn!`; comment: the removals sweep catches up | No (by the comment). |
| 15 | `arcade_jobs.rs`, `takedown_*`, `party_rooms.rs`, `sr-api` `audit.rs`/`events.rs`/`storage/events.rs` | `subscribe` | all inside `#[cfg(test)]` | n/a (read: the `cfg(test)` lines precede each hit). |

So the gateway pumps (rows 1 to 3) are the only place where a lost event reaches **a client**. Everything else is server-side state, handled or flagged.

---

## 2. Sequence numbers and resume

| question | answer |
|---|---|
| Where is a dispatch numbered? | In `Session::deliver` (`session.rs:148-170`): `seq += 1`, frame built with `s = seq`, pushed to the session's `buf`, then `try_send` to the live connection. Called by the pump **after** it received the event from the bus. read. |
| So a dispatch lost at the pump (bus lag)? | **Never numbered.** The session's sequence is contiguous from its own point of view; a RESUME with the client's last `seq` finds `after == g.seq` or a covered gap and replays nothing for the lost events. A RESUME cannot notice a bus-lag gap. read. |
| What does the buffer hold? | The last `resume_buffer` frames (default 5000, `hub.rs:99`) no older than `resume_window` (120 s, `:98`), per session (`session.rs:41-47`, `trim` `:135`). read. |
| When is a dispatch numbered and then dropped? | If the connection's queue is full (`try_send` fails, `session.rs:163-168`) the session detaches; the frames stay in `buf`, so a RESUME replays them. That gap is covered by resume. read. |
| RESUME when the buffer no longer reaches back | `Session::resume` returns false when `after > seq` or the first buffered seq is above `after + 1` (`:178-196`); `conn.rs:487` answers `INVALID_SESSION` with `d: false` on the same socket and waits for IDENTIFY (the socket has no session then, so IDENTIFY is accepted). An expired or unknown session id takes the same branch (`find_resumable`, `hub.rs:954`). read. |
| Does anything else send INVALID_SESSION? | **not found.** `grep INVALID_SESSION` in `sr-gateway`: only that line plus the constant. |
| What happens if a client sends IDENTIFY on a socket that has a session? | `Stop::Close(4005 ALREADY_AUTHENTICATED)` (`conn.rs:407-411`). 4005 is not in the web client's fatal set (`gateway/protocol.ts:37-44`), so the web client reconnects with its backoff. read. |

Consequence for the design: **resume cannot carry the repair**. The repair has to be a state transfer, not a replay.

---

## 3. What the client does on a fresh READY (and on INVALID_SESSION)

### INVALID_SESSION itself (`gateway/client.ts:214-224`, read)
`d !== true`: sessionId and seq are cleared. A timer of 1000 + random*4000 ms then calls `identifyOrResume()` **on the same socket**. The connection status is **not** changed: no banner during the wait (`Banners.tsx:44-52` only shows for connecting/reconnecting/closed). The heartbeat keeps running (it sends `seq: null`).

Against today's server, an INVALID_SESSION on a live session would therefore end like this: wait 1 to 5 s, send IDENTIFY, server closes 4005, client sets 'reconnecting' (banner), reconnects after `delayMs(attempt)` (0.5 to 1 s for the first attempt, `client.ts:112-119`), then identifies on a new socket. It would work, by accident, in about 2 to 6 s, with a banner for the last second. read (traced).

### A fresh READY into the store (`store/reducer.ts:359-424`, read)

| what | replaced or merged |
|---|---|
| current user, users | merged |
| `guilds` | stubs merged into the existing entries and marked `unavailable: true`; **entries not in READY are not deleted from `guilds`**, only `guildOrder` is replaced (so the server leaves the rail but its channels, roles and members stay in memory until a GUILD_DELETE). |
| channels / roles / members of a guild | untouched by READY itself; each following `GUILD_CREATE` **replaces** that guild's channel index and role index with the authoritative list and the guild's voice states (`reduceGuildCreate` `:273-335`), but **merges** members and presences (`putMembers` `:157-171`: members who left stay). |
| private channels (DMs), message requests | private channels merged by id; message requests replaced |
| read state (unread marks, mention counts) | **replaced** from `d.read_state` |
| voice states | **reset to `{}`**, then rebuilt from each GUILD_CREATE's `voice_states` |
| presences | replaced |
| message windows | the cached history **stays on screen but is marked `stale`**; optimistic (pending) sends are kept (`:394-413`) |
| typing | not touched by READY (entries expire by their own TTL, `pruneTyping`) |
| threads | `GUILD_CREATE.threads` and `THREAD_LIST_SYNC` are **upserted**, never pruned (`lounge/threads/threadStore.ts:242-254`): a deleted thread stays |
| friends/relationships | `startSocialLive` (`lounge/shell/social.ts:143-148`) reloads them over REST on every READY if they were loaded, and reseeds message requests |

What is refetched after READY: the **open channel's newest page** (any window with `stale`: `store/hooks.ts:118-120`, `lounge/live.tsx:362-367`; `loadInitialMessages` drops stale ids not in the fresh page, `store.ts:385-410`), friends. Not refetched: member lists (the web uses REST `loadMembers` once per guild, `store.ts:371-374`, and does not send the gateway member-list op: `LAZY_REQUEST` has no sender in `apps/web/src`), threads of non-open channels, other per-feature stores (about 25 `onDispatch` listeners; only `social.ts` reacts to READY). All read.

What a member sees on a re-identify: with INVALID_SESSION d:false and a working server path, nothing for 1 to 5 s (no banner), then a burst: the rail, unread dots and the open channel's messages swap to fresh data. A message being typed: the composer is not remounted by a store update (**assumed**, the draft state was not read); the Send request itself uses REST. Optimistic pending sends survive (read, above).

**Active voice connection and calls (read for the server half, assumed for the web half).** The voice state is keyed by user but carries the session id that joined (`voice.rs:133-139`, set at `:845`). `voice_sweep` (every 15 s, `hub.rs:102`) removes every voice state whose session is **gone** from the registry, or detached longer than `session_grace` (`voice.rs:1258-1275`). A full identify makes a new session id; the old session stays registered, detached, until `resume_window` (120 s) and then goes; `voice_sweep` then ends the call. A new session cannot take the call over: a join from another session needs `sr_e2ee` declared (`voice.rs:798`). So **a re-identify drops the member out of voice** within about `session_grace` seconds, and the web client's voice state (reset to `{}` by READY, restored from the GUILD_CREATE's stale `voice_states`) would show them connected until then. This also happens today after a failed RESUME; it is just rare there.

---

## 4. The proposal

### 4.1 Recommended: resync in place

Principle: the lag happened on the **pump**, not on the socket. Keep the session and the socket and push fresh state down the same session through `Session::deliver`. Everything stays sequenced (a RESUME after a drop mid-resync replays it), the voice state keeps its session id, the connection cap is not touched, the client needs no new socket, no auth round trip and no 1 to 5 s wait.

| pump | who is told | with what |
|---|---|---|
| **server topic** (`on_guild_lag`) | every non-bot session of `guild_sessions(guild)` (`hub.rs:1057`) that `owns_guild` | per session: `guild_create_body(user, guild)` (per-user, permission-filtered: `hub.rs:900-940`) as `GUILD_CREATE`, then `THREAD_LIST_SYNC` if `thread_list_sync` returns one, then the marker `SR_RESYNC {"scope":"guild","guild_id":"..."}`. The client's `reduceGuildCreate` **already replaces** the guild's channels, roles and voice states for a guild it has (read, section 3), so this is accepted as is; the marker is new (4.3). |
| **user topic** (`on_user_lag`) | every non-bot session of that user | a fresh `READY` (same `session_id`, so `client.ts:238-241` just re-stores it) followed by `GUILD_CREATE` (+ `THREAD_LIST_SYNC`) for each guild of the session, built the way `create_session` builds them (`hub.rs:800-870`: all bodies first, then delivered back to back, so the 30 ms burst window in `store/connect.ts:19-49` batches READY and the GUILD_CREATEs into one store update and the rail never shows every server as "unavailable"). The client already handles this exact sequence (reconnect path), including `stale` marking, read-state replacement and the friends reload. No new dispatch is needed. |
| **platform** | no per-session resync of guild state. Membership is already repaired by `reconcile_memberships`. For the lost `PlatformBroadcast` events: every non-bot session gets `SR_RESYNC {"scope":"platform"}` after the reconcile; the web refetches feature flags and announcements on it (the listeners are the `onDispatch` users of `SR_FEATURE_UPDATE` and the announcement store, `lounge/announcements/useActiveAnnouncements.ts:60`; the builder lists them). Cheap: no payload build. |

Bot sessions (`Session.bot`): **unchanged** (they get today's behaviour). A GUILD_CREATE for a guild a bot already has would read as a re-join to a Discord-style bot library. Owner question 3.

Order of steps for one lag (all of it runs after the lag was seen):

1. **Immediately, as today (D-628):** user topic: `send_recheck()`, `clear_age_cache`, `clear_safety_cache`. Server topic: `schedule_list_refresh(guild, true)`. Platform: `reconcile_memberships()`. These are cheap and are what authorization depends on; they must not wait in a queue.
2. **Then enqueue the resync** (key = guild, user, or platform). The credential re-check comes first on purpose: a revoked session closes with 4004 (`conn.rs:204`, `:258-275`) and must not be sent a snapshot first. Before delivering to a session the worker checks `session.is_attached()` and that the session is still registered and still subscribed to the guild (a member who left meanwhile gets nothing: `leave_user` already sent GUILD_DELETE).
3. **Worker** builds and delivers at the capped rate (4.2).

**Snapshot race (important, and the main risk of this design).** A snapshot built at time T0 and delivered at T1 can overwrite a live event that the pump delivered between T0 and T1 (a `CHANNEL_CREATE` arrives, then the older GUILD_CREATE replaces the channel index without it). Mitigation: a per-guild counter `snapshot_epoch` incremented by the guild pump for every event that changes what a GUILD_CREATE carries (everything except messages, edits, deletes, reactions, typing and presence; a small `fn changes_snapshot(&DomainEvent) -> bool` next to `lists::changes_members`, `lists.rs:51`). The worker reads the epoch before building and again before delivering; if it moved, it rebuilds, at most twice, and if it moves a third time it delivers anyway and re-queues the key once after the floor. Messages do not need the counter: they are repaired by the stale-window refetch, which is REST and independent of the snapshot. **assumed** that this is enough; the test in section 5 forces the race.

### 4.2 Storm control

Lag means the node is busy. Numbers (defaults, all in `GatewayConfig`, `hub.rs:34-110`, so tests can shrink them; none is an operator setting, following D-639):

| control | value | where |
|---|---|---|
| Client jitter | **none needed**: there is no new socket and no identify. The cost is server-side, so the spread is the server's rate cap. (Section 4.3 keeps the client's own 1 to 5 s for the INVALID_SESSION path that stays for the failed-RESUME case.) | n/a |
| Floor between two rounds for the same key | **30 s** for a guild, **30 s** for a user, **60 s** for platform. A lag inside the floor sets a dirty flag; one more round runs when the floor ends (same shape as `list_dirty` / `REFRESH_DEBOUNCE`, `lists.rs:27`, `:190-218`, and `search_jobs::LagRecovery`, `:195-215`). | new `ResyncQueue` in a new file `sr-gateway/src/resync.rs`, state in `State` |
| Cap of sessions told per second, whole node | **40 sessions/s** (burst 40), one worker task, at most **4** payload builds in flight (the existing `GUILD_CREATE_CONCURRENCY`, `hub.rs:31`). A 2000-member server drains in about 50 s. | worker in `resync.rs` |
| Order inside the queue | oldest key first, one key's sessions together, so one server finishes rather than all servers crawling | worker |
| Queue bound | at most **256 keys** queued; a further lag on a key already queued only sets its dirty flag; past the bound, the oldest key is dropped with a `warn!` (a dropped key is retried by the next lag or by the client's next reconnect, which is today's behaviour) | worker |
| Identify rate limit | **not found.** The gateway route has a connection cap and request ids but "not the API's rate limit or timeout layers" (`app.rs:2064-2077`); `max_connections` counts `session_count() + unidentified_count()` and answers 503 beyond it (`:163-176`); `max_unidentified_per_ip` is 32 (`hub.rs:66-69`); an op budget of 120 per 60 s per connection (`conn.rs:43-44`). Nothing limits IDENTIFY itself. | read |

### 4.3 Client changes

- `SR_RESYNC` in `store/reducer.ts` (`reduceDispatch`): `scope: guild`: mark the windows of that guild's channels `stale` (the same code that READY uses, `:394-413`, factored into a helper), drop `typing` entries for those channels, and refetch the member list if it was loaded (`loadMembers`; a replace rather than merge is a known gap, section 7). `scope: platform`: nothing in the reducer; the feature and announcement stores listen with `onDispatch`.
- Known client gaps a resync does not fix (not regressions, listed so nobody expects them): members who left stay in `members` (merge), deleted threads stay (upsert), guilds not in READY stay in `guilds` (only `guildOrder` is replaced). Cheap fix for the first and third are in the build plan as "tail".
- The INVALID_SESSION path in `client.ts` stays as it is.

### 4.4 Rejected alternative: INVALID_SESSION d:false, "identify again" (Discord's way)

What it would take: a new `Out::Invalidate` the pumps send to each affected session; `conn.rs` sends op 9 `false`, then removes the session (so no RESUME can use it), clears `self.session` and builds a new `session_tx`, so the same socket accepts the IDENTIFY the client sends after 1 to 5 s. Without that last part the client's IDENTIFY hits 4005 (section 2).

| cost | detail |
|---|---|
| Voice | The call ends (section 3): the voice state is bound to the old session id. Fixing it needs a rebind of voice states to the new session at identify, which is new and touches `voice.rs` (1858 lines) and E2EE rules (`voice.rs:798`). |
| Connection cap | The old session stays registered up to 120 s; for that time `session_count()` counts it and its replacement, so a whole-server re-identify can double the count and hit the 503 at `app.rs:171` for everyone, including the members trying to identify. |
| Work per member | A full READY and every guild's GUILD_CREATE for each member of a lagged **server**, to repair one server. |
| Storm | The jitter is the client's (1 to 5 s) and no server cap exists; the 1 to 5 s window is also too short for a 2000-member server. |
| Benefit | Atomic: a new session gets one consistent snapshot, no ordering race (4.1); identical for all clients including bots and third-party libraries; less new server code. |

It is still the right answer for: bot sessions (if the owner wants them repaired, owner question 3), and the failed-RESUME case that exists today. I keep that path as it is.

---

## 5. The build plan

One builder, one wave. Rust first (the lock for cargo is the budget), then web.

### Files

| file | change |
|---|---|
| `crates/sr-gateway/src/resync.rs` (new) | `ResyncQueue` (keys, dirty flags, floors, bounded), the worker task, `deliver_guild_resync`, `deliver_user_resync`, `deliver_platform_resync`, the `changes_snapshot` epoch check |
| `crates/sr-gateway/src/hub.rs` | `on_guild_lag`, `on_user_lag`, the platform pump's lag arm call `resync`; `Gateway::start` spawns the worker; the guild pump bumps `snapshot_epoch`; `GatewayConfig` gets `resync_floor`, `resync_platform_floor`, `resync_sessions_per_second`, `resync_queue_max` (defaults above) and a `Debug` line each; `State` gets the queue |
| `crates/sr-gateway/src/lists.rs` | `changes_snapshot` next to `changes_members` (or in `resync.rs`) |
| `crates/sr-gateway/src/lib.rs` | `mod resync;` |
| `crates/sr-gateway/src/tests.rs` (+ a new `resync_tests.rs`) | the tests below |
| `docs/05-*` gateway protocol doc and `apps/web/src/developers/gatewayDocs.generated.ts` | list `SR_RESYNC` (the generated file comes from a script: **assumed**; the builder finds it) |
| `apps/web/src/store/reducer.ts` | `SR_RESYNC` case, stale-marking helper shared with READY |
| `apps/web/src/store/store.ts` | on `SR_RESYNC` guild: refetch members if loaded (small) |
| `apps/web/src/lounge/announcements/useActiveAnnouncements.ts` and the feature-flag listener (the builder finds it: `SR_FEATURE_UPDATE`) | refetch on `SR_RESYNC` platform |
| `apps/web/src/store/store.test.tsx`, `store/connect.test.ts` | tests below |

No route, no response shape, no migration, no `openapi.json` change: `SR_RESYNC` is a gateway dispatch, not an API type. **Check:** the first-paint chunk rule (D-636) applies to the web half; `store/reducer.ts` is in the Lounge chunk (**assumed**, check with `check-bundle-budget.mjs`).

### Tests (names are proposals)

Harness: `Lagging::new()` (`tests.rs:2774`: a bus of capacity 4 and a pump that can be blocked on demand), `emit`, `msg`, `set_list`; `session_revoke_tests.rs:110` for the user-topic flood (publish 300 events on `Topic::User` before the pump runs, single-threaded runtime) and `TestWorld`.

| test | proves |
|---|---|
| `a_lagged_guild_topic_resends_the_guild_to_its_sessions` | after a forced lag a session gets `GUILD_CREATE` (with a channel created during the lost events) then `SR_RESYNC`, in that order, with sequence numbers continuing the session's own |
| `a_lagged_guild_topic_resend_is_filtered_per_member` | a member without view permission on a private channel does not see it in the resent GUILD_CREATE (D-628 intent: no leak through the repair) |
| `a_lagged_user_topic_sends_a_fresh_ready_and_guilds_with_the_same_session_id` | READY carries the same `session_id`; one GUILD_CREATE per guild follows |
| `a_resync_is_not_sent_to_a_revoked_session` | revoke + lag: the socket closes 4004, no READY/GUILD_CREATE was written first |
| `a_resync_is_not_sent_to_a_member_who_left_meanwhile` | `MemberLeft` lost in the same gap: GUILD_DELETE (reconcile), no GUILD_CREATE |
| `a_second_lag_inside_the_floor_waits_and_then_runs_once` | two laps of lag inside `resync_floor` (set to 200 ms in the test config) yield exactly one more round after the floor, not two |
| `resync_stays_under_the_session_rate` | 12 sessions, rate 4/s: the 12 deliveries span at least 2 s (paused tokio time) |
| `the_queue_is_bounded` | lags on `resync_queue_max + 5` guilds: no more than the bound is queued, the extra is logged and dropped |
| `a_snapshot_overtaken_by_a_structural_event_is_rebuilt` | a `CHANNEL_CREATE` handled between the build and the delivery causes a rebuild; the delivered GUILD_CREATE contains the channel (the race test, uses a directory gate like `staff_gate` in `Lagging`) |
| `a_resync_keeps_the_voice_state_of_the_session` | a member in voice, user-topic lag: after the resync `voice_sweep` does not remove them (the failure mode of the rejected design) |
| `bot_sessions_are_not_resynced` | bot session sees no GUILD_CREATE/READY/SR_RESYNC after a lag |
| `a_lagged_platform_topic_sends_the_platform_marker` | reconcile as today plus `SR_RESYNC {"scope":"platform"}` |

Existing `a_lagged_guild_topic_refreshes_the_member_list` (`tests.rs:2891`) and `a_lagged_user_topic_rechecks_that_users_sessions` (`session_revoke_tests.rs:110`) must stay green unchanged: they prove step 1 still runs first.

Client (vitest, single files): in `store/store.test.tsx` or a new `store/resync.test.ts`: `SR_RESYNC guild marks only that guild's channel windows stale and keeps pending sends`; `SR_RESYNC guild drops typing of that guild only`; `a second READY with the same session id replaces read state and keeps pending sends` (the in-place user path, no new code but nothing tests it with an unchanged session id); `SR_RESYNC platform leaves the stores alone`. `gateway/gateway.test.ts`: `a second READY with the same session id keeps the sequence counter` (client `onDispatchFrame`, `client.ts:230-246`).

### Honest size
Rust: `resync.rs` about 250 lines, `hub.rs` about 60, tests about 350. Web: about 120 lines plus 4 tests. One builder, one wave; the cold cargo build of `sr-gateway` plus its tests is the lock cost. If time is short, drop in this order: platform marker (tail), bounded-queue test, member refetch on resync, the snapshot-epoch retry (then document the race as a known gap, because the stale-window refetch and the next structural event heal it; I would not ship without at least the test marked `#[ignore]` with the reason).

---

## 6. Proposed decision row and threat-model sentence

**D-next.** When a gateway pump misses events (event-bus lag), it first does what D-628 says (re-check credentials, drop caches, rebuild the member list, reconcile memberships) and then re-sends state to the **affected sessions that are already connected**: a fresh READY and GUILD_CREATEs for a user topic, a fresh per-member GUILD_CREATE and an `SR_RESYNC` marker for a server topic, the marker alone for the platform topic. Rounds are queued with a floor per key and a cap on sessions told per second. Sessions keep their id, so voice and calls continue. Bot sessions are not resynced. No setting.
*Why:* an in-memory queue overflowing must not leave a member looking at a deleted channel or missing a permission change until they reconnect, and Discord's "identify again" would end their voice call, double the session count and send every member of a busy server to the busiest endpoint at once.

**Threat-model sentence (for the row D-628 touches):** a resync snapshot is built per member with the same visibility rules as GUILD_CREATE at connect and only after the credential re-check, so recovering from lag never shows a member more than a fresh connection would, and a revoked or departed member receives nothing; the cost of a lag-storm is bounded by the per-key floor and the node-wide rate cap (residual risk: a flood that keeps the guild topic lagging can still cost one bounded round per 30 s per server).

---

## 7. Open questions for the owner's Fable agent (each with my default)

1. **In place or Discord's INVALID_SESSION?** Default: in place (sections 3 and 4.4).
2. **Numbers**: floor 30 s (guild, user) and 60 s (platform), 40 sessions per second, queue bound 256. Default as written; they are code constants in `GatewayConfig`, not operator settings (D-639 style).
3. **Bot sessions**: default unchanged (no resync; bots are told in the developer docs that events can be lost and REST is the source of truth). Alternative: INVALID_SESSION d:false for bots, which needs the `Out::Invalidate` path of 4.4.
4. **Should the web show anything during a resync?** Default no (silent swap, the open channel's messages refetched in place). Alternative: a small "Catching up" line in the connection banner while a resync is in flight (needs the server to say when it ends; not in this design).
5. **Platform marker in wave 1 or tail?** Default tail (platform lag is rare: capacity 256 and low volume).
6. **Out of scope but found (separate items, defaults: file as SEC-AUD rows next wave):**
   - `integration_policy.rs:136` ignores lag: a lost `FeatureSettingsChanged` keeps a stale unfurl/GIF policy until restart. One-line fix (`subscribe_lagging` + `refresh` on `Err`).
   - `staff_feed.rs:1507` ignores lag: a lost staff alert never becomes a card or email.
   - `bridge_wiring.rs:123` only warns: mirrored messages are lost for good on lag.
   - Web: `putMembers` merges (members who left stay), `threadStore.upsert` never prunes, READY does not delete `guilds` entries that left (section 3). Cheap fixes in the reducer, with tests.
   - A full identify after a failed RESUME also ends voice (section 3): worth a voice rebind to the new session id when the old session of the same user is gone (a separate design item in `voice.rs`).

## Proposed inventory row

SEC-AUD-161 (design, no code): lost gateway dispatches after bus lag; the design and build plan are in `fix-p63-resync.md`.

## Cleanup

Nothing to clean: no database, no cargo target, no stack, no lock was taken; no scratch files were created. The only file written is this handoff.
