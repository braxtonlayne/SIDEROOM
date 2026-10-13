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

### 5.1 Entrance and account auth endpoints

Implemented in `crates/sr-api/src/routes/auth/` (`routes::auth::router_with(service)`). Paths are relative to `/api/v1`. Errors use the standard body; field errors are the Discord `errors` tree (`errors.<field>._errors[0].message`). "Session" means a logged-in cookie session with the `X-SR-CSRF` header on non-GET requests. Pre-login POSTs carry no CSRF token: they require `Content-Type: application/json` and, when the browser sends an `Origin`, it must be one of the allowed origins (else 403).

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `GET /sr/invites/{code}` | none | n/a | `{valid:true}`, `{valid:false}` (unknown, malformed, used and revoked are identical), or `{valid:false,reason:"closed"}` (cap reached; returned for every code). Per IP: 10/min and 50/day, then 429 |
| `GET /sr/usernames/check?username=` | none | n/a | `{available:bool}` (invalid and reserved names are unavailable). 30/min per IP |
| `POST /sr/register` | none | `{code,email,username,display_name?,password,accepted_house_notes:true}` | 201 `{id}` and the `sr_session` cookie. 400 with field errors (`email`, `username`, `display_name`, `password`, `rules`); 403 `900006` for any unusable invite; 403 `900005` when closed. Consumes the invite, checks the cap and issuer credit, writes ledger rows (issuer -1 `redemption`, new account +5 `initial_grant`) in one transaction, and emails a verification link. 5/min and 20/day per IP |
| `POST /sr/age-gate` | session | `{over_18:bool}` | 204. Stores the "Are you 18 or older?" answer (D-032) |
| `POST /sr/auth/login` | none | `{login,password}` (username or email) | `{status:"ok"}` and the session cookie, or `{status:"mfa_required",methods:["totp","recovery"]}` and a 5-minute `sr_mfa` cookie. 401 `"That doesn't match our list."` for unknown account or wrong password alike; 429 with `retry_after` after 5 failures per login name and client address (30 s doubling to 15 min), 50 attempts per login name per hour, or 30 per IP; 403 `900007`/`900008` for deactivated or banned accounts, only after the correct password |
| `POST /sr/auth/2fa` | `sr_mfa` cookie | `{method:"totp" or "recovery",code,trust_device}` | `{status:"ok"}` (+ `recovery_codes_remaining` for recovery) and the session cookie. 401 on a wrong, replayed or expired code; the pending login is dropped after 5 wrong codes. `trust_device: true` also sets a 30-day `sr_trusted` cookie (API-scoped); a login from that browser skips the second factor until the device is revoked (`GET`/`DELETE /sr/auth/trusted-devices[/{id}]`) |
| `POST /sr/auth/logout`, `POST /sr/auth/logout-all` | session | n/a | 204 and the cookie is cleared |
| `GET /sr/auth/sessions` | session | n/a | `[{id,device_name,created_at,last_seen_at,expires_at,current}]`, newest first |
| `DELETE /sr/auth/sessions/{id}` | session | n/a | 204, or 404 `10020` |
| `POST /sr/auth/totp/enroll` | session | n/a | `{otpauth_uri,manual_entry_key}` (pending secret) |
| `POST /sr/auth/totp/verify` | session | `{code}` | `{recovery_codes:[10 codes]}` (shown once). 400 `errors.code` on a wrong code |
| `DELETE /sr/auth/totp` | session | `{password}` | 204 (removes TOTP and recovery codes). 403 `900004` on a wrong password |
| `POST /sr/auth/recovery-codes` | session | `{password}` | `{recovery_codes:[10 codes]}`, replacing the old set |
| `POST /sr/auth/forgot` | none | `{email}` | Always 204 (no account enumeration; the email is sent in the background). 5 per 10 min per IP, one mail per address per minute |
| `POST /sr/auth/reset` | none | `{token,password}` | 204 and all sessions are revoked; 2FA is still required at the next login. 400 `errors.token` for an invalid, used or expired (30 min) link; 400 `errors.password` for a weak password (the link stays valid) |
| `POST /sr/auth/verify-email` | none | `{token}` | 204, or 400 `errors.token` (24 h, single use) |
| `POST /sr/auth/verify-email/resend` | session | n/a | 204. One mail per account per minute (429 otherwise); no-op once verified |
| `POST /sr/auth/login-link` | none | `{token}` | `{status:"ok"}` and the session cookie for the one-time Owner link printed by `sr-cli bootstrap` (1 hour, single use). 400 `errors.token` otherwise; a reset link never logs in |
| `GET /sr/auth/trusted-devices` | session | n/a | `[{id,device_name,created_at,expires_at}]` |
| `DELETE /sr/auth/trusted-devices/{id}` | session | n/a | 204, or 404 `10020` |
| `GET /sr/auth/discord/start?intent=login` or `register`, optional `next` | none | n/a | 302 to Discord with a 10-minute `sr_oauth_state` cookie. 404 when `[discord]` has no client id |
| `GET /sr/auth/discord/callback` | cookies | n/a | 302 back to the app: `/login` (signed in, or `?error=discord_unlinked`), `/login/2fa` when a second factor applies, or `/register` with a 10-minute `sr_oauth` cookie holding a pending Discord link (consumed by `POST /sr/register`). The server-side state is checked, so a forged callback is refused |
| `GET /sr/auth/discord/pending` | cookie | n/a | `{pending:true,username}` or `{pending:false}` |
| `DELETE /sr/auth/discord/pending` | cookie | n/a | 204 and the pending link is dropped |

Not implemented yet: passkeys (`/sr/auth/passkey/*`). The `webauthn-rs` crate depends on the C `openssl` library, which needs an ADR (doc 03 section 4) before it can be used. Discord OAuth is implemented (`oauth2` with rustls) as listed above. The CSRF token is issued by `GET /sr/csrf` (section 2).

### 5.2 CDN delivery: `/cdn/...`

`GET|HEAD /cdn/<kind>/<segment>/...?ex=<unix-seconds>&sig=<base64url>` serves stored media. It lives on the public listener **outside** `/api/v1`: no cookie, no CSRF, no `Authorization`. The signature (`HMAC-SHA256(secret, "{canonical path}\n{ex}")`, constant-time compare) is the credential, so a signed URL is a bearer link. Buckets are never public; the server streams objects from storage.

| `<kind>` | Object key | Source |
|---|---|---|
| `attachments` | `chat/{channel}/{attachment}/{filename}` | message attachments (`url` / `proxy_url` are returned signed, valid 24 h) |
| `emoji` | `emoji/{guild}/{emoji}.{ext}` | custom emoji (`/cdn/emoji/{guild}/{emoji}.{png,jpg,gif,webp}`) |
| `stickers` | `sticker/{guild}/{sticker}.{ext}` | guild stickers |
| `avatars` | `avatars/{user}/{hash}` | user avatars and profile banners (`PATCH /users/@me`); public kind, see below |
| `icons` | `icons/{guild}/{hash}` | guild icon, banner and splash (`PATCH /guilds/{id}`); public kind, see below |

Any other kind is 404. Emoji objects carry no URL in the Discord shape; clients build the path from the guild and emoji ids and obtain a signature from the server. Rust code builds paths with `routes::cdn::cdn_path(kind, &[segments])`, which percent-encodes each segment (everything but `A-Za-z0-9-._~`) and yields the canonical path that is signed. The server decodes the request path and re-canonicalises it before verifying, so `%20` vs a literal space, or hex case, do not matter.

**Image variants (FND-STOR-003).** Any image object accepts `?size=` and `format=`:

- `size`: the longest edge in pixels, a power of two from 16 to 4096 (anything else is 400). The aspect ratio is kept and the image is never upscaled past the original.
- `format`: `png`, `jpg` (or `jpeg`), `webp` or `gif`. For `avatars` and `icons` the URL extension (`/cdn/avatars/{user}/{hash}.webp`) names the format and `format=` overrides it. With neither, the output follows the source (JPEG stays JPEG, GIF stays GIF, otherwise WebP if `Accept` lists `image/webp`, else PNG) and the response has `Vary: Accept`.
- Animated GIF stays an animated GIF when resized. Requesting another format gives the first frame. Animated WebP sources are decoded as their first frame, and WebP output is lossless-only, because the pure-Rust `image` encoder has no lossy or animated mode.
- Outputs are re-encoded from pixels, so EXIF/XMP/ICC metadata never survives, and the EXIF orientation is applied.
- Each variant is computed once and cached in object storage as `<key>@<size>.<ext>` (`<key>@orig.<ext>` for a format-only conversion). Deleting an avatar or icon deletes its variants.
- Work is bounded: sources over 16 MiB or 24 million pixels (animations: 32 million pixels in total, 400 frames) give 413; a source that is not a PNG, JPEG, GIF or WebP gives 415. Resizes run at most four at a time.
- Signatures cover the variant: the signed string is `{path}?size=N&format=F` with only the parameters present. A URL without them signs the bare path exactly as before, so URLs minted earlier stay valid, but they cannot be turned into a variant by appending `size`. Emoji and stickers get signed variant URLs from `GET /guilds/{guild}/emojis/{id}/url?size=&format=` and `.../stickers/{id}/url` (one hour, any member).
- `Range` is ignored on variants.

**Exception: `avatars` and `icons` are public kinds.** Their URLs are `/cdn/avatars/{user}/{hash}.{ext}` and `/cdn/icons/{guild}/{hash}.{ext}`, where `hash` is 32 hex characters of a SHA-256 over the normalised image (prefix `a_` when animated). The unguessable hash is the credential, as on Discord, so these two kinds are served **without** `ex`/`sig`, with `Cache-Control: public, max-age=31536000, immutable`. This is a deliberate exception to docs/02 section 5 ("Buckets are never public; URLs are signed"): every API object already shows these hashes to anyone who can see the user or guild, and a per-viewer signed URL for every avatar in a member list would defeat caching. The bucket itself stays private and the server streams the object. A signature that is present must still be valid (a bad one is 403), and every other kind still requires one. The owner can overrule this; turning it off means returning `avatar_url` fields signed per request.

Responses:

| Status | When |
|---|---|
| 200 / 206 | Object (or one range). Always `Content-Length`, `Accept-Ranges: bytes`, `ETag`, `Cache-Control: private, max-age=<remaining signed lifetime, at most 86400>`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; sandbox`, `Referrer-Policy: no-referrer` |
| 304 | `If-None-Match` matches the ETag (strong tag derived from key and storage etag) |
| 400 | Malformed path: `..`, `.`, empty segment, trailing slash, backslash, bad or non-UTF-8 percent escape, control character, over-long |
| 403 | `ex` or `sig` missing, malformed, or not matching the path |
| 404 | Unknown kind (before any signature check), or object missing (only after a valid signature) |
| 410 | Signature valid but `ex` is in the past (the client should refetch the message for a fresh URL) |
| 413 | Variant source over the byte or pixel budget |
| 415 | Variant requested for something that is not a PNG, JPEG, GIF or WebP |
| 416 | Unsatisfiable range, with `Content-Range: bytes */<len>` |
| 429 | Per-client-IP limit (bucket `GET /cdn/{*path}`, default 300 per 10 s, shared process-wide limiter) |

`Range`: a single `bytes=a-b`, `a-` or `-n` range gives 206 with `Content-Range`. A multi-range or syntactically invalid header is ignored and the whole object is sent with 200 (RFC 9110 permits ignoring Range). `If-Range` with a stale ETag also gives 200. HEAD returns the same headers without a body.

Content type: images, video and audio from an allow-list (png, jpeg, gif, webp, avif, mp4, webm, mov, mp3, ogg, flac, wav, m4a, aac) are served with their real type and `Content-Disposition: inline`, chosen by file extension, or by sniffing the first 512 bytes when the extension is unknown. Everything else (HTML, SVG, PDF, scripts, unknown) is forced to `Content-Type: application/octet-stream` with `Content-Disposition: attachment`, so an upload cannot run in the site origin. CORS allows only the configured public origin(s), without credentials. No compression layer is applied.

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
