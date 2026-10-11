# 05 — API and Gateway

The SIDE ROOM API is **deliberately shaped like Discord's API v10** (D-060). Goal: a Discord bot written with discord.js, discord.py, serenity, or twilight should port by changing the base URLs and token, plus a short compatibility checklist. The first-party web client uses the same API. There is no private "client-only" API, except for platform-only resources (Cinema, Library, admin, donations) under their own prefixes.

## 1. Base URLs

| Purpose | URL |
|---|---|
| REST | `https://<host>/api/v1` |
| Gateway | `wss://<host>/gateway?v=1&encoding=json` (optionally `&compress=zstd-stream`) |
| CDN-like assets | `https://<host>/cdn/...` (signed when private) |
| Media (HLS) | `https://<host>/media/...` |
| Activities origin | `https://activities.<host>` |

**Compatibility shim:** `/api/v10/*` is an alias of `/api/v1/*` for Discord-compat routes, so libraries that hardcode `v10` work after a base-URL override.

## 2. Authentication

| Client | Header |
|---|---|
| Web app | session cookie (`sr_session`) + CSRF header `X-SR-CSRF` on mutations |
| Bots | `Authorization: Bot <token>` |
| OAuth2 apps acting for a user | `Authorization: Bearer <token>` |

Discord's user-token automation ("self-bots") is not supported, and user sessions cannot be used from non-browser clients without the CSRF token.

## 3. Conventions (identical to Discord)

- Snowflake IDs as strings. ISO8601 timestamps.
- Error body `{ "code": <int>, "message": "...", "errors": {…field tree…} }`. Codes reuse Discord's JSON error codes where an equivalent exists (e.g. `10003 Unknown Channel`, `50013 Missing Permissions`). SIDE ROOM-specific codes start at `900000`.
- Rate-limit headers `X-RateLimit-Limit/Remaining/Reset/Reset-After/Bucket/Scope`. 429 body includes `retry_after`.
- Pagination: `before`, `after`, `around`, `limit`.
- `X-Audit-Log-Reason` header is honoured.
- Permission bits: **exactly Discord's bit positions** (`VIEW_CHANNEL = 1<<10`, `SEND_MESSAGES = 1<<11`, …). SIDE ROOM-only permissions use high bits starting at `1<<56` (see doc 07).

## 4. Resource coverage (Discord-compatible)

Implement these route families with Discord-compatible request and response shapes. Phase is in brackets.

- **Users** [1]: `GET/PATCH /users/@me`, `GET /users/{id}`, `GET /users/@me/guilds`, `DELETE /users/@me/guilds/{id}`, `POST /users/@me/channels`, connections, application role connections (later).
- **Guilds** [1]: create/get/modify/delete, channels, members (list, search, add, modify, remove), roles, bans, prune, widgets, vanity URL, welcome screen, onboarding, previews, MFA level, integrations (stub).
- **Channels** [1]: get/modify/delete, messages (CRUD, bulk delete, crosspost), reactions, pins, typing, permissions overwrites, invites, follow announcement channel, threads (start, join, leave, members, archived lists), forum posts.
- **Emoji / Stickers / Soundboard** [1–2].
- **Invites** [1]: get/delete (server invites).
- **Webhooks** [1]: create, execute (incl. `?wait`, `thread_id`), Slack/GitHub-compatible endpoints [later].
- **Polls** [1]: answer voters, end poll.
- **Audit logs** [1]. **AutoMod** [1]. **Scheduled events** [1]. **Stage instances** [2]. **Guild templates** [1].
- **Voice** [2]: regions, voice state modification.
- **Applications / Interactions / Commands** [6]: global and guild commands, interaction responses, follow-ups, components (buttons, selects, modals), autocomplete. Interactions are delivered over the gateway **or** to an HTTP endpoint with Ed25519 signatures (same as Discord).
- **OAuth2** [6]: authorize, token, scopes `identify email guilds guilds.join bot applications.commands`.

Each route's spec is generated from Rust (`utoipa`) and published at `/api/v1/openapi.json`. Swagger-style docs are served at `/developers/docs/api`.

## 5. SIDE ROOM–specific APIs

Prefix `/api/v1/sr/...`. These are first-party-only unless an OAuth scope is granted.

| Family | Examples |
|---|---|
| Platform invites | `GET /sr/invites` (mine), `POST /sr/invites`, `DELETE /sr/invites/{code}`, `GET /sr/invites/{code}` (public validity check: `valid`/`invalid` only, no issuer info), `POST /sr/register` |
| Credits | `GET /sr/invite-credits` |
| Donations | `POST /sr/invoices`, `GET /sr/invoices/{id}`, `GET /sr/entitlements/@me`, `GET /sr/guilds/{id}/boost-status`, `GET /sr/leaderboards/{kind}` |
| Cinema | `/sr/cinema/titles`, `/titles/{id}`, `/seasons`, `/episodes`, `/assets/{id}/playback`, `/progress`, `/lists`, `/ratings`, `/reviews`, `/search`, `/trending?scope=friends\|known\|global` |
| Live TV | `/sr/tv/channels`, `/sr/tv/guide?from&to`, `/sr/tv/channels/{id}/now` |
| Library | `/sr/library/series`, `/editions`, `/chapters/{id}/pages`, `/progress`, `/bookmarks`, … |
| Submissions | `/sr/submissions` (create draft, upload parts, submit, withdraw), `/sr/review/queue`, `/sr/review/{id}/decision` |
| Watch parties | `/sr/parties` (create, join, control) + gateway events |
| Admin | `/sr/admin/...` (staff-only, see the admin spec) |
| Features/settings | `/sr/features` (registry + effective settings for a scope) |
| Packages | `/sr/packages` (themes/extensions/layouts/activities) |

## 6. Gateway

### 6.1 Opcodes (Discord numbering)

| Op | Name | Dir |
|---|---|---|
| 0 | DISPATCH | ← |
| 1 | HEARTBEAT | ↔ |
| 2 | IDENTIFY | → |
| 3 | PRESENCE_UPDATE | → |
| 4 | VOICE_STATE_UPDATE | → |
| 6 | RESUME | → |
| 7 | RECONNECT | ← |
| 8 | REQUEST_GUILD_MEMBERS | → |
| 9 | INVALID_SESSION | ← |
| 10 | HELLO | ← |
| 11 | HEARTBEAT_ACK | ← |
| 31 | REQUEST_SOUNDBOARD_SOUNDS | → |
| 100+ | SIDE ROOM ops: `100 WATCH_PARTY_SYNC`, `101 E2EE_MLS` (MLS proposals/commits/welcome relay), `102 LAZY_REQUEST` (member-list ranges, like Discord's op 14 for user clients) | ↔ |

### 6.2 Intents
Discord's intent bits, including privileged `GUILD_MEMBERS`, `GUILD_PRESENCES`, and `MESSAGE_CONTENT`. Privileged intents for released apps require approval in the developer portal (like Discord's verification).

### 6.3 Dispatch events
All Discord events relevant to implemented features use the same names and payloads (`READY`, `GUILD_CREATE`, `MESSAGE_CREATE`, `MESSAGE_UPDATE`, `MESSAGE_DELETE`, `MESSAGE_REACTION_ADD`, `TYPING_START`, `PRESENCE_UPDATE`, `VOICE_STATE_UPDATE`, `VOICE_SERVER_UPDATE`, `THREAD_*`, `GUILD_MEMBER_*`, `INTERACTION_CREATE`, …). SIDE ROOM additions are prefixed `SR_`:
`SR_ENTITLEMENT_UPDATE`, `SR_INVITE_CREDITS_UPDATE`, `SR_PARTY_STATE`, `SR_SUBMISSION_UPDATE`, `SR_LIBRARY_UPDATE` (followed series got a chapter/episode), `SR_ACTIVITY_UPDATE` (watching/reading presence), `SR_STAFF_ALERT`.

Rich presence for watching/reading uses standard `activities` on `PRESENCE_UPDATE` (type 3 "Watching", plus a custom type for "Reading"), so it renders like Discord's activity cards.

### 6.4 Session lifecycle
`HELLO(heartbeat_interval=41250)` → `IDENTIFY(token, intents, properties)` → `READY(session_id, resume_gateway_url, user, guilds (unavailable stubs), private_channels, relationships, read_state, user_settings, sessions)` → `GUILD_CREATE` per guild. Resume buffers are kept in memory per session for 2 minutes or 5,000 events.

## 7. Bot compatibility checklist (porting guide)

Published at `docs/developers/porting-from-discord.md` (Phase 6):
1. Override the REST base URL and gateway URL (all major libraries support this).
2. Use a SIDE ROOM bot token.
3. Unsupported or different: Nitro/SKU/monetization endpoints (return 404), Stage Discovery, Discord-specific integrations (Twitch/YouTube), and `cdn.discordapp.com` URLs (use `<host>/cdn`).
4. Extra capabilities: the `/sr/*` APIs with scopes `sr.cinema.read`, `sr.library.read`, `sr.parties`.

A conformance test suite (`crates/sr-compat-tests`) runs real discord.js and serenity example bots against a test server in CI (Phase 6).
