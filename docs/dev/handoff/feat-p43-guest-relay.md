# Handoff: feat/p43-guest-relay (wave 43-A, ARC-GST-001)

Branch `feat/p43-guest-relay` (base `integration/phase42`). Not pushed, no PR. Roadmap: `docs/dev/roadmap-p28.md`
"Worktree 43-A: Guest Relay". Decisions served: D-385, D-397, D-399; threat-model row **T41**.

## The contract (43-B builds to this; no change was needed after the first commit)

Rust types and constants: `crates/sr-activities/src/guest.rs` (commit `898d4de6`, "guest-link contract"). Socket frames:
`crates/sr-gateway/src/guest.rs` (module doc).

### REST (all under `/api/v1`)

| Route | Who | Answer |
|---|---|---|
| `POST /activity-instances/{instance_id}/guest-links` | a current participant of the running instance (session) | `201 {lobby_token, expires_at}`; 404 not a participant / no such instance, 409 game does not declare `guest_play` or 5 working links already |
| `DELETE /activity-instances/{instance_id}/guest-links/{lobby_token}` | the instance starter (lobby host) or the member who minted the link | `204`; 403 another participant, 404 stranger / unknown link |
| `POST /play/{lobby_token}/join` | nobody (no credentials, no cookie, no CSRF) | `200 {guest_token, callsign, expires_at, topic, socket_path, application_id, application_name, capabilities}`; 404 for every dead link (unknown, malformed, expired, revoked, lobby ended); 409 lobby full (16 guests); 429 rate limit (20 joins/minute per address) |

* `lobby_token`: 64 lowercase hex digits, shown once, link lifetime 6 hours (`expires_at`, RFC 3339 UTC) or until the
  lobby host revokes it or the Activity ends. The client builds `/play/<lobby_token>#key=<ephemeral_game_key>`; the key
  never reaches the server.
* `guest_token`: `srg_` + 64 hex, session lifetime 2 hours (never past the link). **Every REST route answers it 403** (bearer
  or `sr_session` cookie). The only thing that takes it is the guest socket.
* `capabilities` (added at the commander's request after 43-B): the game's manifest flag names, sorted, **reserved ones
  included** (so `ingame_voice` appears when declared and the runner can refuse it). Flag names only.
* **Seat ids:** the relay stamps plain numbers (`seat`, `from`) plus `role`; it does not send `g<seat>`/`m<seat>` ids.
  43-B's runner builds `g<seat>` (role `guest`) and `m<seat>` (role `member`) itself from those two fields, as its
  `contract.ts` does. A seat number is unique per lobby and never reused while the lobby is open.
* `callsign`: e.g. `Amber Otter 42`, random, not chosen by the guest ("lobby-approved nickname" is not built; see open).
* `application_id` / `application_name` tell the runner which game bundle to load. **Serving that bundle to a person with no
  session is 43-B's** (the activity proxy host `<id>.activities.<host>` is what serves bundles today; check that it
  needs no cookie for the guest path).

### Guest socket: `GET /gateway/guest?v=1` (root of the public listener, beside `/gateway`)

JSON text frames, one object each (binary frames are dropped with close 4403):

```
client -> relay
  {"op":"hello","guest_token":"srg_..."}          a guest
  {"op":"hello","lobby_token":"<64 hex>"}         a signed-in member of the Activity: send the sr_session cookie and an
                                                  allowed Origin (the app's own origin); this is the member's seat
  {"op":"subscribe","topic":"game:<lobby_token>"} optional; only the one topic; answered {"op":"subscribed","topic":..}
  {"op":"publish","topic":"game:<lobby_token>","data":"<base64>"}   opaque bytes (<= 16 KiB + 64) for every other seat
  {"op":"ping"}
relay -> client
  {"op":"ready","seat":n,"role":"guest|member","callsign":"..","key_id":"<32 hex>","epoch":0,
   "scope":{"data":true,"media":false},"peers":[{"seat","role","callsign"}]}
  {"op":"data","from":seat,"data":"<base64>"}
  {"op":"peer","event":"join","peer":{..}}   {"op":"peer","event":"leave","seat":n}
  {"op":"pong"}   {"op":"closed","reason":"revoked|ended"}
```

* The first frame must be `hello` within 10 s. The topic is implied by the credential; the relay compares the SHA-256 of the
  topic's token with the seat's lobby. A member seat shows as `Host` (the starter) or `Player`; **no account id, handle or
  avatar is ever sent to a guest.** Members of the Activity do not use their MLS data channel for guests: the **host's runner
  bridges the game's frames onto the lobby with the key from the link fragment** (`sideroom/lobby/v1/data` label; frames are
  opaque to the relay, so the encryption scheme is the runner's choice; suggestion: `sr_e2ee::data` frames under a key
  derived from `#key=`).
* Close codes: 4400 bad frame (not JSON / no op), 4401 unauthorized (bad or dead credential, hello timeout), **4403 forbidden:
  a second topic, an unknown op, a binary frame, a frame before hello ("403/Drop")**, 4409 lobby full (32 seats), 4410 gone
  (revoked / expired / Activity ended; also announced as `closed` first), 4413 frame over 32 KiB.
* The credential is re-checked every 5 seconds, so revoke / expiry / lobby end disconnect a seat within that time.
* Over-budget sends (256 kbit/s per seat) and over-size lobby payloads are dropped without closing the seat.

### Manifest flags

`guest_play` and `lobby_channel` are now `FlagStatus::Active` in `arcade::FLAGS` (a running instance lists them in
`capabilities`; a development build runs with every active flag). `ingame_voice` stays `Reserved`: the validator accepts it, a
running instance never lists it, the runner must refuse it. `guest_play` still requires `TIER_1_ORIGINAL` (unchanged).
A guest link needs `guest_play` on the instance. `lobby_channel` is active and is the flag a game declares for the lobby
sub-room; today a seat exists only while the lobby has a live guest link (see open).

## Done

* **Migration 2080** `activity_guest_links` (lobby token digest, creator, expiry, revocation) and `activity_guest_sessions`
  (guest token digest, callsign, expiry). `cargo test -p sr-db --test secret_columns` green (columns are `*_sha256`).
* `sr-activities`: `guest.rs` (contract), `service/guest_db.rs` (`create_guest_link`, `revoke_guest_link`, `guest_join`,
  `guest_seating`, `member_seating`).
* `sr-api`: the three routes, OpenAPI, per-address join limiter (`GUEST_JOINS_PER_MINUTE = 20`), and the 403 for any
  guest-shaped credential in the `Principal` extractor (`auth/extract.rs`), checked before any lookup.
* `sr-voice::lobby` (the per-lobby sub-room): `LobbyHub`, `LobbyId` (SHA-256 of the token), seats, per-seat budget, bounded
  queues, `LobbyKeys {key_id, epoch}` per lobby, `Scope {data: true, media: Reserved}`. Shares no state or type with `SfuCore`.
* `sr-gateway::guest`: the guest socket (`GuestGateway`, `GuestAuthority` trait, `run_guest` state machine), `sr-gateway` gained
  a path dependency on `sr-voice` and `sha2`. `sr-server::guest_relay` implements the authority over `ActivityService` and
  `app.rs` mounts `/gateway/guest` when `[activities] enabled`.
* Design note `docs/dev/design/ingame-voice.md` (media leg of the sub-room: transport, lobby keys with epoch bump on rekey,
  what a guest hears, what must exist before it is built).
* Docs: `docs/15-threat-model.md` row T41, `docs/inventory/21-feature-phase.md` "Wave 43" section with ARC-GST-001 (ticked
  once the integrator has the gates on the merged tree; unticked here because the box is for merged proofs).
* `route_matrix.tsv`: 3 rows (membership, membership, public/none); `authz_matrix.rs`: seeded guest-play game + live link,
  overlays and a write-phase token override; no ceiling raised, no `WRITE_OBSERVED` entry added.

## Tests run (all on scratch DB `sr_p43_guest_relay`, dropped at the end)

| Gate | Result |
|---|---|
| cargo fmt --check, clippy --all-targets -D warnings (sr-activities, sr-voice, sr-gateway, sr-api, sr-server) | ok |
| sr-activities lib / Postgres db (12, incl. 2 guest tests) / arcade (7) / trust_db (9) | 47 / 12 / 7 / 9 pass |
| sr-voice lib (incl. 7 lobby tests) / sr-gateway lib (206, incl. 9 guest socket tests) | 50 / 206 pass |
| sr-db secret_columns, migration_versions; sr-api openapi_names, route_matrix | pass (route_matrix 6) |
| sr-api lib --include-ignored / sr-server lib --include-ignored | 811 / 245 pass |
| sr-server immutable_rules --include-ignored (embeds the whole authz matrix, read and write phases, normal and killed states, and `guest_links_hold_over_http`) | 24 pass (with SR_TEST_VOICE_PORT_BASE=53800: another agent held 52000..) |
| `authz_matrix::guest_links_hold_over_http` | pass: guest token 403 on every authenticated route as bearer and cookie (about 2300 calls) and the named chat/member/profile/shelf cases; real WebSocket: opaque frames both ways, 5 forged second topics closed 4403, member gateway refuses the token, revoke closes the guest 4410, join after revoke 404, join rate limited 429 |
| OpenAPI, api-client, MSW handlers regenerated; mocks:check | up to date (1173 operations) |
| scripts/threat-model.test.mjs, prettier on the docs | pass |


## Open / not done (with reasons)

* **Nickname approval** ("lobby-approved nickname"): not built; guests only get a random callsign (D-397 safest default).
* **Member seat without a guest link**: a member's seat is allowed only while the lobby has a live link (the seat is looked up
  through `activity_guest_links`). A `lobby_channel`-only game that never mints a link has no relay yet. Needs an owner
  decision (a link-less "members lobby" would be a lobby row per instance); the data leg itself (`LobbyHub`) does not care.
* **Rekey on revoke**: `LobbyKeys.epoch` exists and is announced, but nothing bumps it yet (the key is the link's fragment, which
  the server cannot change). Documented in T41 residual and the design note.
* **Media leg / `ingame_voice`**: designed only, as asked.
* **Scaling**: `LobbyHub` is in process (one node). The existing gateway and SFU are also single node in this repo; a second
  node would need sticky routing by lobby (documented here, not built).
* **`GET /play/<token>` page and bundle serving for guests**: 43-B (the SPA route and the isolated runner bundle). The guest
  join answer gives the app id; whether the activity proxy origin serves a bundle with no session is for 43-B to confirm.

## Decisions (defaults the owner may change)

* Link life 6 h, session life 2 h, 5 working links per instance, 16 guests per link, 32 seats per lobby, 20 joins/min per
  address, recheck 5 s, per-seat budget 256 kbit/s (the call data channel's default).
* Revoke by instance starter or link minter (the roadmap says "lobby host").
* A dead link is always 404 (no state oracle); a made-up or revoked guest token on the socket is 4401.

## Owner questions

1. Should a members-only `lobby_channel` (no guest link) get a lobby too (see open)?
2. Nickname approval for guests, or keep random callsigns only?
3. Should revoking a link rekey the lobby (needs the host to rotate the `#key=`)?
