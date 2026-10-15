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
- Error body `{ "code": <int>, "message": "...", "errors": {…field tree…} }`. Codes reuse Discord's JSON error codes where an equivalent exists (e.g. `10003 Unknown Channel`, `50013 Missing Permissions`). SIDE ROOM-specific codes start at `900000`. `900011` `AGE_CONFIRMATION_REQUIRED` (HTTP 403) means the content is visible but opening it needs the account to confirm it is 18 or older: show the age dialog, `POST /sr/age-gate`, retry (doc 06 §7, D-184). `GET /library/chapters/{id}/pages` returns it for age-restricted series, and every message endpoint under `/channels/{id}/...` returns it for an age-restricted (NSFW) server channel, or a thread of one, until the account has answered yes (D-032, LNG-CHN-006); DMs are never gated. Its permission checks come first, so a member who cannot see the channel still gets 50001.
- Rate-limit headers `X-RateLimit-Limit/Remaining/Reset/Reset-After/Bucket/Scope`. 429 body includes `retry_after`.
- Pagination: `before`, `after`, `around`, `limit`.
- `X-Audit-Log-Reason` header is honoured.
- Image attachments can carry `width`, `height` (1 to 16384 each, both or neither) and `blurhash` (6 to 128 base-83 characters) in the upload-slot request (`POST /channels/{id}/attachments`, per file). They are declared by the uploading client, never derived on the server (the server does not decode the picture), are stored in the existing `attachments` columns and come back on `AttachmentObject`, including inside forward snapshots. They exist only for layout and the placeholder shown while the image loads; nothing trusts them for security. Pictures uploaded by other clients simply have none.
- Spoiler attachments use Discord's `SPOILER_` filename prefix. `spoiler: true` on an upload slot makes the server prefix the stored filename (once; a name that already starts with `SPOILER_` is kept and counts as a spoiler), so `AttachmentObject.filename` carries the prefix and no extra field is needed (D-177).
- **Embed images and `proxy_url` (D-176).** The client never loads a third-party image URL by itself. Where Discord's embed objects carry `proxy_url`, SIDE ROOM fills it on `embed.image`, `embed.thumbnail` (not `embed.video`, whose `proxy_url` is always removed) with a same-origin path `/cdn/external/{hmac}/{url_b64}` (section 5.2), and fills `embed.author.proxy_icon_url` and `embed.footer.proxy_icon_url` the same way. The fields are added when a message is rendered (REST and gateway, forward snapshots included), for link previews and for embeds written by bots or users alike; a `proxy_url` sent by a client is discarded. The original `url` stays in the object, but a client may show it only behind an explicit "Load image" click, without a referrer. URLs that are not absolute http(s), and paths of our own CDN, get no `proxy_url`. Only images are proxied (PNG, JPEG, GIF, WebP).
- Permission bits: **exactly Discord's bit positions** (`VIEW_CHANNEL = 1<<10`, `SEND_MESSAGES = 1<<11`, …). SIDE ROOM-only permissions use high bits starting at `1<<56` (see doc 07).
- Attachments: when an upload slot is requested with `spoiler: true` the server prefixes the stored filename with `SPOILER_` (once; an already-prefixed name is not doubled), exactly like Discord, so `AttachmentObject.filename` carries the marker and there is no extra field (D-177). Embeds: `proxy_url` on `image`, `thumbnail`, `video`, `author.icon_url` and `footer.icon_url` is the only URL clients load. It points at `/cdn/external/{hmac}/{url_b64}`, the SSRF-safe, size- and time-limited, re-encoded image proxy; until that proxy exists `proxy_url` is absent and clients render text and offer a per-embed "Load image" action instead of fetching the original `url` (D-176).

## 4. Resource coverage (Discord-compatible)

Implement these route families with Discord-compatible request and response shapes. Phase is in brackets.

- **Users** [1]: `GET/PATCH /users/@me` (`global_name` is the display name), `GET /users/{id}`, `GET /users/@me/guilds`, `DELETE /users/@me/guilds/{id}`, `POST /users/@me/channels`, connections, application role connections (later). SIDE ROOM additions for the Settings screens: `GET/PATCH /users/@me/settings` (a client-owned JSON blob, merged as an RFC 7386 merge patch, at most 64 KiB / 8 levels / 256 top-level keys, versioned, `SR_USER_SETTINGS_UPDATE` to the user's sessions), `GET/PATCH /users/@me/notification-defaults` (`message_notifications` 0/1/2/null; resolved after the per-server and per-channel choices and before the server's own default; servers only, DMs keep their own scope), `GET/PATCH /users/@me/privacy` (friend-request sources, DMs from server members, Message Requests). Signed-in password, username and email changes are under `/sr/auth/*` (section 5.1).
- **Guilds** [1]: create/get/modify/delete, channels, members (list, search, add, modify, remove), roles, bans, prune, widgets, vanity URL, welcome screen, onboarding, previews, MFA level, integrations (stub).
  Member listing takes `query` (nickname, username or display name, case-insensitive, at most 64 characters) and `role_id`; `GET /guilds/{id}/members/search` is Discord's alias and `GET /guilds/{id}/roles/{role_id}/members` lists a role's members. Role modify takes `unicode_emoji` (one emoji or `null`) and `icon` (image data URI cropped to a square, `null` removes it); setting one clears the other, and the icon hash is served at `/cdn/icons/{guild_id}/{hash}.png`. `GET/POST /guilds/{id}/prune` previews and runs a prune (`KICK_MEMBERS`; members who joined more than `days` (1 to 30) ago and wrote nothing in that time (with last-seen tracking, D-166, also not seen in that time), hold no role except `include_roles`, rank below the caller and are not the owner; at most 10,000 per call; one `MEMBER_PRUNE` audit entry). The guild object carries `afk_channel_id` and `afk_timeout`.
  Lounge servers, phase 10 (LNG-GLD-006/008/010/011/012/013, LNG-INV-004/005, LNG-CHN-004/006; handoff `docs/dev/handoff/feat-p10-lounge-be.md`):
  - **Bulk ban**: `POST /guilds/{id}/bulk-ban` `{user_ids (1 to 200), delete_message_seconds?, reason?}` needs `BAN_MEMBERS` and `MANAGE_GUILD`, bans each user under the single-ban rules and answers `{banned_users, failed_users}` (403 when nobody could be banned); every ban emits `GUILD_BAN_ADD`, an audit entry and the member removal.
  - **Server tag** (D-012 is unrelated; this is Discord's server tag, free here): `GET/PUT/DELETE /guilds/{id}/tag` reads and sets the server's tag `{tag (2 to 4 letters or digits), badge (icon id 0 to 15), color (24-bit RGB), members_can_display}` (reading needs membership, changing needs `MANAGE_GUILD`, changes are audited). A person picks the one tag they show with `GET/PUT /users/@me/primary-guild` `{guild_id: string|null}` (they must be a member and the server's tag must allow display); the response is `{primary_guild}`. User references (members, bans, message authors, user objects) then carry `primary_guild: {identity_guild_id, identity_enabled, tag, badge, color}`, only while the tag exists, members may display it and the person is still a member. There is no `PATCH /users/@me/primary-guild`, no `GET /guilds/tag-badges` and no `tag`/`tag_badge` on the guild object (the phase-10 integration kept one implementation; the badge list is the fixed set of 16 icon ids the client maps to glyphs).
  - **Per-server profile**: `PATCH /guilds/{id}/members/@me` `{nick?, avatar?, banner?, bio? (190)}` (data URIs for the images, `null` clears a field) and `GET /users/@me/guilds/{id}/member`; the member object carries `avatar`, `banner` and `bio`. Nobody edits another member's profile. Images are stored under `/cdn/avatars/{user_id}/{hash}` with a hash that is specific to the server.
  - **Vanity URL**: `GET|PATCH /guilds/{id}/vanity-url` (`MANAGE_GUILD`; `{code, uses}`; 3 to 32 letters, digits or hyphens, case-insensitive and unique across servers, reserved route names refused, never the text of an existing invite). `GET /invites/{code}` and `POST /invites/{code}` fall back to the vanity code when no invite has that text: it never expires, has no use limit and counts joins in `uses`. The invite object's `guild` now carries `splash`, `banner`, `description` and `vanity_url_code`.
  - **Staff Server flag**: the guild object (`GET /guilds/{id}`, `GUILD_CREATE`, which follows READY for every server) and the `GET /users/@me/guilds` entries carry `is_staff_guild: true` for the platform's Staff Server only; other servers omit the field (ADM-STF-010).
  - **Join pipeline**: joining by invite, vanity code or from the directory adds the member as one transaction, posts the join notice (message type 7, authored by the new member) in the system channel and dispatches `GUILD_MEMBER_ADD`. A Community with screening on (`PATCH /guilds/{id}/sr/community` `screening_enabled`, `screening_description`, `screening_rules` (1 to 16 rules)) admits members as `pending: true`: they can read but not write, react or join voice until `PUT /guilds/{id}/requests/@me` (`GUILD_MEMBER_UPDATE`). `GET /guilds/{id}/member-verification` returns the form (`enabled`, `description`, `form_fields[{field_type:"TERMS", values}]`) to every member. `POST /guilds/{id}/onboarding-responses` `{option_ids}` checks the answers against the onboarding form (required questions, single-select), grants the roles of the chosen options (never a managed role or one with moderation power) and returns `{channel_ids, role_ids}`.
  - **Temporary invites**: a member who joined through an invite with `temporary: true` and holds no role is removed (`GUILD_MEMBER_REMOVE`) once they have been offline for a minute.
  - **Server Directory** (D-012): `GET /directory/categories`, `GET /directory?query&category&limit&offset` (approved listings of servers that still are Community, featured first, then by members), `GET /directory/{id}`, `POST /directory/{id}/join` (no invite; same pause, ban and screening rules; session only). A server's managers (`MANAGE_GUILD`) ask with `PUT /guilds/{id}/directory` `{category, description?}`, read the state with `GET` and withdraw with `DELETE`; a first request, or one after a rejection or removal, and every edit of an approved listing waits for staff. Staff with `MANAGE_DIRECTORY` work `GET /admin/directory?status=pending|approved|rejected|removed` and `PATCH /admin/directory/{id}` `{decision: approve|reject|remove, featured?, note?}`; decisions go to the platform access log. Bots cannot use any `/directory` route.

- **Channels** [1]: get/modify/delete, messages (CRUD, bulk delete, crosspost), reactions, pins, typing, permissions overwrites, invites, follow announcement channel, threads (start, join, leave, members, archived lists), forum posts.
  Announcement channels (LNG-CHN-004): `PATCH /channels/{id}` `{type: 5}` turns a text channel into an announcement channel and `{type: 0}` back (no other change of type); `flags` bit `1 << 24` (SIDE ROOM's `AUTO_PUBLISH`) makes an announcement channel publish every message as it is sent. `POST /channels/{id}/followers` `{webhook_channel_id}` subscribes a text or announcement channel where you hold `MANAGE_WEBHOOKS` (idempotent; the subscription is a follower webhook, type 2, in `GET /channels/{id}/webhooks`, and deleting it unfollows). `POST /channels/{id}/messages/{id}/crosspost` publishes one message (your own with `SEND_MESSAGES`, others' with `MANAGE_MESSAGES`; 40033 when it already was, 50024 outside announcement channels): each follower receives a copy with flag `IS_CROSSPOST` (`1 << 1`), the original author, the text and embeds, a `message_reference` to the source and no pings; the source gets `CROSSPOSTED` (`1 << 0`) and a `MESSAGE_UPDATE`. Attachments, stickers and polls are not copied. Slowmode is enforced on every message endpoint (`BYPASS_SLOWMODE`, `MANAGE_MESSAGES` and `MANAGE_CHANNELS` skip it; 429 with `retry_after`).
- **Emoji / Stickers / Soundboard** [1–2].
- **Invites** [1]: get/delete (server invites).
  Group DM invites (G-47): the owner of a group DM makes a link with `POST /channels/{id}/invites` (type `1`, no `guild`, `channel {id, type: 3, name, icon}`, 24-hour expiry, unlimited uses, never single use); `GET /channels/{id}/invites` lists the live ones (owner), `GET /invites/{code}?with_counts=true` previews one (`approximate_member_count` is the group's size), `POST /invites/{code}` adds the caller (the others get `CHANNEL_RECIPIENT_ADD` and `CHANNEL_UPDATE`, the newcomer `CHANNEL_CREATE`; a full group answers `MaxGroupSize`), `DELETE /invites/{code}` revokes (owner). Table `group_dm_invites` (migration 0796). Group DM icons (G-50): `PATCH /users/@me/channels/{id} {icon}` takes an image data URI through the media pipeline, stores the content hash and serves it at `/cdn/channel-icons/{channel}/{hash}`; anything but an image data URI is `400`, `null` removes the icon.
- **Server safety fields** (G-36). The guild object carries `mfa_level` (0 none, 1 moderator actions need a second factor), `system_channel_flags` (Discord bits: 1 join, 2 boost, 4 setup tips, 8 join-reply button, 16 and 32 role-subscription messages; set by `PATCH /guilds/{id}`; bit 1 stops the join notice), `widget_enabled` and `widget_channel_id` (`GET|PATCH /guilds/{id}/widget`, `MANAGE_GUILD` to write, the channel must belong to the server). `POST /guilds/{id}/mfa {level}` is owner only and needs the owner to have a second factor. With level 1 the moderator routes (kick, ban, unban, bulk ban, prune, role and channel create/edit/delete/reorder/assign, overwrites, server edit and delete) answer `403 900004 SR_REAUTH_REQUIRED` until the session proves a second factor: `GET|POST /sr/auth/reauth {method: totp|recovery|passkey, code}` opens the same 15-minute window as `/admin/reauth`. Bots are exempt. `GET|PATCH /guilds/{id}/sr/community` gains `dm_spam_filter`, `explicit_media_filter_dms`, `hide_suspicious_links` and `warn_new_account_days` (0 to 365); they are stored and audited (`GUILD_UPDATE` changes) and not enforced by a filter yet. Every guild edit writes `changes` for name, levels, flags, system channel, `mfa_level` and the widget.
- **Sticker packs** (G-44): `GET /sticker-packs` → `{sticker_packs: [{id, name, description, cover_sticker_id, stickers: [{id, name, tags, type: 1, format_type, pack_id}]}]}`, empty until a pack is installed (no default packs ship; the owner supplies artwork). `GET /stickers/{id}/url` signs a pack sticker's image. Pack stickers are standard stickers (`guild_id` null), so `sticker_ids` on message create already accepts them in any channel. Tables `sticker_packs` and `stickers.pack_id` (migration 0794).
- **Scheduled events**: see the next bullet.
- **Webhooks** [1]: create, execute (incl. `?wait`, `thread_id`), Slack/GitHub-compatible endpoints [later].
  Webhook tokens are stored only as a SHA-256 hash (open question 137), so a URL cannot be shown again after creation; `POST /webhooks/{id}/reset-token` issues a new token (and URL) and invalidates the old one. Create and modify take `avatar` (image data URI, cropped to a square, `null` removes it on modify); the hash is returned as `avatar` and served at `/cdn/avatars/{webhook_id}/{hash}.png`. Deleting the webhook or replacing the avatar deletes the stored image.
- **Polls** [1]: answer voters, end poll.
- **Audit logs** [1]. **AutoMod** [1]. **Scheduled events** (Discord shapes; table `guild_scheduled_events`, migration 0800): `GET|POST /guilds/{id}/scheduled-events`, `GET|PATCH|DELETE .../{event_id}`, `GET .../{event_id}/users?limit&with_member&before&after`, and `PUT|DELETE .../{event_id}/users/@me` for "interested". Create needs `CREATE_EVENTS` or `MANAGE_EVENTS` (everyone has the first by default; voice and stage events also need `VIEW_CHANNEL` and `CONNECT` in the channel); change or delete needs `MANAGE_EVENTS`, or `CREATE_EVENTS` on your own event. `entity_type` 1 stage and 2 voice take a `channel_id` of that type; 3 (external) takes `entity_metadata.location` and an end time and no channel. `privacy_level` is 2 only. The start must be in the future and the end after it; a server keeps at most 100 open events. `PATCH {status}` moves forward only: 2 starts a scheduled event, 3 ends a running one, 4 cancels a scheduled one; completed and canceled events are read-only (and leave the list). The server also starts events at their start time and ends running ones at their end time (30-second tick), and winds up a voice or stage event whose channel was deleted. Objects add `user_count` (`with_user_count=true`) and `user_rsvp` (the caller). Audit actions 100 to 102. **Stage instances** [2]. **Guild templates** [1]. Create-from-template is one server-side transaction (D-180): `GET /guilds/templates/builtin` lists the eight built-in templates (`id`, `name`, `description`, `serialized_source_guild` with `settings`, `roles` and `channels` using local ids, the same shape as a template made from a server); `POST /guilds/templates/{code}` `{name, icon?}` and `POST /guilds` `{name, template}` take a built-in id or a user template code, create roles, channels, overwrites and settings in one transaction and emit a single `GUILD_CREATE`.
- **Voice** [2]: regions, voice state modification.
- **Applications / Interactions / Commands** [6]: global and guild commands, interaction responses, follow-ups, components (buttons, selects, modals), autocomplete. Interactions are delivered over the gateway **or** to an HTTP endpoint with Ed25519 signatures (same as Discord).
  As built (what is and is not Discord-compatible, every route): [specs/developer-platform.md](specs/developer-platform.md). Interactions are delivered over the gateway only for now; the HTTP endpoint with Ed25519 signatures is not built.
- **OAuth2** [6]: authorize, token, revoke, introspect, scopes `identify email guilds bot applications.commands applications.commands.permissions.update` (`guilds.join` and the rest are not issued yet), PKCE `S256`. Details in [specs/developer-platform.md](specs/developer-platform.md).

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
| Voice | `GET /sr/voice/turn` (ephemeral TURN credentials), `PATCH /guilds/{id}/voice-states/{user_id}` (moderators; section 6.5), stage instances, stream previews, DM calls and the soundboard (section 6.6) |
| Submissions | `/sr/submissions` (create draft, upload parts, submit, withdraw), `/sr/review/queue`, `/sr/review/{id}/decision` |
| Watch parties | `/sr/parties` (create, join, control) + gateway events |
| Admin | `/sr/admin/...` (staff-only, see the admin spec) |
| Features/settings | `/sr/features` (registry + effective settings for a scope) |
| Profile catalogue | `GET /sr/profile-catalog` → `{effects: [{slug, label_key}], decorations: [{slug, asset, label_key}]}`; `PATCH /users/@me` accepts only listed slugs for `profile_effect` and `avatar_decoration`. Effects `sparkle`, `aurora`, `snow`, `embers` are built in; `decorations` is empty until artwork is supplied (G-48). |
| Second factor | `GET|POST /sr/auth/reauth` (see the server safety fields above) |
| Packages | `/sr/packages` (themes/extensions/layouts/activities) |

### 5.1 Entrance and account auth endpoints

Implemented in `crates/sr-api/src/routes/auth/` (`routes::auth::router_with(service)`). Paths are relative to `/api/v1`. Errors use the standard body; field errors are the Discord `errors` tree (`errors.<field>._errors[0].message`). "Session" means a logged-in cookie session with the `X-SR-CSRF` header on non-GET requests. Pre-login POSTs carry no CSRF token: they require `Content-Type: application/json` and, when the browser sends an `Origin`, it must be one of the allowed origins (else 403).

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `GET /sr/invites/{code}` | none | n/a | `{valid:true}`, `{valid:false}` (unknown, malformed, used and revoked are identical), or `{valid:false,reason:"closed"}` (cap reached; returned for every code). Per IP: 10/min and 50/day, then 429 |
| `GET /sr/usernames/check?username=` | none | n/a | `{available:bool}` (invalid and reserved names are unavailable). 30/min per IP |
| `POST /sr/register` | none | `{code,email,username,display_name?,password,accepted_house_notes:true}` | 201 `{id}` and the `sr_session` cookie. 400 with field errors (`email`, `username`, `display_name`, `password`, `rules`); 403 `900006` for any unusable invite; 403 `900005` when closed. Consumes the invite, checks the cap and issuer credit, writes ledger rows (issuer -1 `redemption`, new account +5 `initial_grant`) in one transaction, and emails a verification link. 5/min and 20/day per IP |
| `POST /sr/age-gate` | session | `{over_18:bool}` | 204. Stores the "Are you 18 or older?" answer (D-032) |
| `POST /sr/auth/login` | none | `{login,password}` (username or email) | `{status:"ok"}` and the session cookie, or `{status:"mfa_required",methods:["passkey","totp","recovery"]}` (`passkey` only when the account has one, `totp` only when enrolled) and a 5-minute `sr_mfa` cookie. 401 `"That doesn't match our list."` for unknown account or wrong password alike; 429 with `retry_after` after 5 failures per login name and client address (30 s doubling to 15 min), 50 attempts per login name per hour, or 30 per IP; 403 `900007`/`900008` for deactivated or banned accounts, only after the correct password |
| `POST /sr/auth/2fa` | `sr_mfa` cookie | `{method,code,trust_device}`, `method` being `totp`, `recovery` or `passkey` (for `passkey`, `code` is the JSON text of `{ceremony_id, credential}` from `POST /sr/auth/passkeys/login/begin {second_factor:true}`) | `{status:"ok"}` (+ `recovery_codes_remaining` for recovery) and the session cookie. 401 on a wrong, replayed or expired code; the pending login is dropped after 5 wrong codes. `trust_device: true` also sets a 30-day `sr_trusted` cookie (API-scoped); a login from that browser skips the second factor until the device is revoked (`GET`/`DELETE /sr/auth/trusted-devices[/{id}]`) |
| `POST /sr/auth/logout`, `POST /sr/auth/logout-all` | session | n/a | 204 and the cookie is cleared |
| `GET /sr/auth/sessions` | session | n/a | `[{id,device_name,created_at,last_seen_at,expires_at,current}]`, newest first |
| `DELETE /sr/auth/sessions/{id}` | session | n/a | 204, or 404 `10020` |
| `POST /sr/auth/totp/enroll` | session | n/a | `{otpauth_uri,manual_entry_key}` (pending secret) |
| `POST /sr/auth/totp/verify` | session | `{code}` | `{recovery_codes:[10 codes]}` (shown once). 400 `errors.code` on a wrong code |
| `DELETE /sr/auth/totp` | session | `{password}` | 204 (removes TOTP; recovery codes too unless passkeys remain). 403 `900004` on a wrong password |
| `POST /sr/auth/recovery-codes` | session | `{password}` | `{recovery_codes:[10 codes]}`, replacing the old set |
| `POST /sr/auth/forgot` | none | `{email}` | Always 204 (no account enumeration; the email is sent in the background). 5 per 10 min per IP, one mail per address per minute |
| `POST /sr/auth/reset` | none | `{token,password}` | 204 and all sessions are revoked; 2FA is still required at the next login. 400 `errors.token` for an invalid, used or expired (30 min) link; 400 `errors.password` for a weak password (the link stays valid) |
| `POST /sr/auth/verify-email` | none | `{token}` | 204, or 400 `errors.token` (24 h, single use). Also confirms an address change started with `POST /sr/auth/email/change` (the link is the same `/verify-email/{token}` page; the new address becomes the login and is marked verified, and the old one is told) |
| `POST /sr/auth/password` | session | `{current_password,new_password,revoke_other_sessions?}` | 204. Re-checks the current password (shares the login lockout), applies the signup strength rules, by default revokes every other session and trusted device (`revoke_other_sessions: false` keeps them), mails a notice and writes a platform audit row. 403 `900004` on a wrong password, 400 `errors.new_password` |
| `PUT /sr/auth/username` | session | `{username,password}` | `{username}`. Same rules as signup; two changes per account per day (429); 400 `errors.username` (`taken`/`invalid`). Friends and shared servers get `USER_UPDATE` |
| `POST /sr/auth/email/change` | session | `{new_email,password,mfa?}` | 204. Mails a 24 h single-use link to the new address; the account keeps its old address until the link is used. The old address gets a notice with a "This wasn't me" link at the same time, and again once the change completes. When the account has two-factor authentication on, `mfa: {method: "totp"|"recovery"|"passkey", code}`\|"recovery", code}` is required unless this session passed a second-factor check (login 2FA or an earlier email change) in the last 15 minutes; otherwise 400 `errors.mfa.required` (or `invalid`). An address that already belongs to someone else also answers 204 and sends nothing (no oracle). One request per minute per account (D-167) |
| `POST /sr/auth/email/cancel` | none | `{token}` | 204, or 400 `errors.token` (7 days, single use). The "This wasn't me" link (`/email-change/cancel?token=`): cancels a pending change; once the change has completed it revokes every session and trusted device instead. The notice sent on completion replaces the one sent on request (D-167) |

| `POST /sr/auth/verify-email/resend` | session | n/a | 204. One mail per account per minute (429 otherwise); no-op once verified |
| `POST /sr/auth/login-link` | none | `{token}` | `{status:"ok"}` and the session cookie for the one-time Owner link printed by `sr-cli bootstrap` (1 hour, single use). 400 `errors.token` otherwise; a reset link never logs in |
| `POST /sr/auth/passkeys/register/begin` | session | `{password}` | `{ceremony_id, options}` (`navigator.credentials.create` options; resident key and user verification required). Re-checks the password (403 `SR_REAUTH_REQUIRED` and the login lockout apply); 429 per account (10 per 10 minutes); 400 at 20 passkeys |
| `POST /sr/auth/passkeys/register/finish` | session | `{ceremony_id, name?, credential:{rawId, response:{clientDataJSON, attestationObject}}}` | `{passkey, recovery_codes}` where `recovery_codes` is a fresh batch (shown once) only when the account had no unused ones, else null. The ceremony is bound to the session that began it and works once. 400 for any verification failure (wrong origin or RP id, bad attestation, duplicate credential id) |
| `POST /sr/auth/passkeys/login/begin` | none | `{second_factor?}` | `{ceremony_id, options}`. Default: passkey-only login with an empty allow list (discoverable credential, user verification required). With `second_factor: true` and the `sr_mfa` cookie: options restricted to that account's passkeys, finished through `POST /sr/auth/2fa`. 60 per minute per IP (429) |
| `POST /sr/auth/passkeys/login/finish` | none | `{ceremony_id, credential:{rawId, response:{clientDataJSON, authenticatorData, signature, userHandle}}}` | `{status:"ok"}` with the session cookie (counts as a fresh second-factor check). 401 generic for every failure; 403 `SR_ACCOUNT_DEACTIVATED` / `SR_PLATFORM_BANNED` only after a valid assertion. 30 per minute per IP plus the per-IP lockout |
| `POST /sr/auth/passkeys/verify/begin` | session | n/a | `{ceremony_id, options}` for a second-factor proof on a signed-in session (email change). The result goes into `mfa: {method:"passkey", code}` where `code` is the JSON text of `{ceremony_id, credential}`; 400 when the account has no passkey |
| `GET /sr/auth/passkeys` | session | n/a | `[{id,name,created_at,last_used_at,backup_eligible}]`, oldest first |
| `PATCH /sr/auth/passkeys/{id}` | session | `{name}` | the renamed passkey; 400 `errors.name` (1 to 64 characters), 404 for another account's id |
| `DELETE /sr/auth/passkeys/{id}` | session | `{password}` | 204. Password re-check; 404 for another account's id. Removing the last second factor (no passkey, no TOTP) also drops the recovery codes |
| `GET /sr/auth/trusted-devices` | session | n/a | `[{id,device_name,created_at,expires_at}]` |
| `DELETE /sr/auth/trusted-devices/{id}` | session | n/a | 204, or 404 `10020` |
| `GET /sr/auth/discord/start?intent=login` or `register`, optional `next` | none | n/a | 302 to Discord with a 10-minute `sr_oauth_state` cookie. 404 when `[discord]` has no client id |
| `GET /sr/auth/discord/callback` | cookies | n/a | 302 back to the app: `/login` (signed in, or `?error=discord_unlinked`), `/login/2fa` when a second factor applies, or `/register` with a 10-minute `sr_oauth` cookie holding a pending Discord link (consumed by `POST /sr/register`). The server-side state is checked, so a forged callback is refused |
| `GET /sr/auth/discord/pending` | cookie | n/a | `{pending:true,username}` or `{pending:false}` |
| `DELETE /sr/auth/discord/pending` | cookie | n/a | 204 and the pending link is dropped |

Passkeys (ENT-AUTH-010, D-161) use the pure-Rust verifier in `sr-auth::webauthn` (`none` attestation; ES256, EdDSA, RS256); the rows above for `/sr/auth/passkeys/*` are the whole surface. Pending ceremonies are single-use, expire after 5 minutes and live in process memory (single node, like the pending-2FA table and the lockouts). A signature counter that does not increase fails the attempt and writes the platform audit action `passkey.counter_regression`. Binary fields in options and results are base64url without padding. Discord OAuth is implemented (`oauth2` with rustls) as listed above. The CSRF token is issued by `GET /sr/csrf` (section 2).

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

**External images (LNG-MSG-014, D-176): `GET /cdn/external/{hmac}/{url_b64}`.** Both segments are base64url without padding: `url_b64` is the third-party URL (at most 2048 bytes, http or https) and `hmac` is `HMAC-SHA256(K, url)` where `K = HMAC-SHA256(server.secret_key, "sideroom/external-image/v1")`. There is no `ex`: the link is stable, so browsers cache it. The signature is the credential, checked in constant time before anything is fetched, so the route is not an open relay and only URLs the server itself put in a message can be requested. The handler:

- fetches through the same SSRF-safe fetcher as link previews: scheme and credentials are checked, the host is resolved and **every** resolved address is checked after DNS (private, loopback, link-local, CGNAT, multicast, reserved, cloud metadata, IPv4-mapped, NAT64, 6to4 and Teredo forms; decimal, hex, octal and short IPv4 spellings are normalised by the URL parser first), the connection is pinned to the checked addresses so DNS cannot change between check and connect, and the same rules apply to each of at most three redirects. The operator's `[unfurl] deny_hosts` and `allow_hosts` apply;
- reads at most 10 MiB, within 10 s in total (connect 5 s); only an image content type is accepted, then the bytes must sniff as PNG, JPEG, GIF or WebP and decode within the pixel budget (24 million pixels). SVG, HTML, AVIF and anything else is 415;
- re-encodes from pixels at no more than 2048 px a side (JPEG stays JPEG, an animated GIF stays animated, everything else becomes PNG), so EXIF, XMP, ICC and text chunks are gone. The pure-Rust encoder has no lossy WebP, so WebP is not produced;
- stores the result as `external/{sha256(url)}` in the bucket for 30 days (the first 12 bytes of the object hold `SRX1` and the expiry) and deletes expired objects with the hourly `external_images.gc` job. A failed fetch is remembered for ten minutes in memory;
- answers `200` with the real `Content-Type`, `Content-Disposition: inline`, `Cache-Control: public, max-age=86400`, a strong `ETag` (`If-None-Match` gives 304), `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; sandbox` and `Referrer-Policy: no-referrer`.

| Status | When |
|---|---|
| 400 | `hmac` or `url_b64` is not base64url |
| 403 | The signature does not match, or the address is not allowed (the body never says what a name resolved to) |
| 413 | Over 10 MiB, or over the pixel budget |
| 415 | Not a PNG, JPEG, GIF or WebP |
| 429 | Per client address (bucket `GET /cdn/external/{hmac}/{url_b64}`, 300 per 10 s); a cache miss also counts per user (session) or per address (`GET /cdn/external (fetch)`, 30 per minute) and across all callers (`GET /cdn/external (global)`, 240 per minute) |
| 502 / 504 | The other site failed or timed out |
| 503 | Six fetches are already running; `Retry-After: 5` |

Responses of the normal `/cdn/{*path}` route:

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
| 18 | STREAM_CREATE | → |
| 19 | STREAM_DELETE | → |
| 20 | STREAM_WATCH | → |
| 22 | STREAM_SET_PAUSED | → |
| 31 | REQUEST_SOUNDBOARD_SOUNDS | → |
| 100+ | SIDE ROOM ops: `100 WATCH_PARTY_SYNC`, `101 E2EE_MLS` (MLS proposals/commits/welcome relay), `102 LAZY_REQUEST` (member-list ranges, like Discord's op 14 for user clients) | ↔ |

### 6.2 Intents
Discord's intent bits, including privileged `GUILD_MEMBERS`, `GUILD_PRESENCES`, and `MESSAGE_CONTENT`. Privileged intents for released apps require approval in the developer portal (like Discord's verification).

### 6.3 Dispatch events
All Discord events relevant to implemented features use the same names and payloads (`READY`, `GUILD_CREATE`, `MESSAGE_CREATE`, `MESSAGE_UPDATE`, `MESSAGE_DELETE`, `MESSAGE_REACTION_ADD`, `TYPING_START`, `PRESENCE_UPDATE`, `VOICE_STATE_UPDATE`, `VOICE_SERVER_UPDATE`, `THREAD_*`, `GUILD_MEMBER_*`, `INTERACTION_CREATE`, …). SIDE ROOM additions are prefixed `SR_`:
`SR_ENTITLEMENT_UPDATE`, `SR_PAYMENT_UPDATE`, `SR_INVITE_CREDITS_UPDATE`, `SR_PARTY_STATE`, `SR_SUBMISSION_UPDATE`, `SR_LIBRARY_UPDATE` (followed series got a chapter/episode), `SR_CINEMA_UPDATE` (own Cinema state), `SR_ACTIVITY_UPDATE` (watching/reading presence), `SR_STAFF_ALERT`. Watch Parties add `WATCH_PARTY_UPDATE`, `WATCH_PARTY_PLAYBACK`, `WATCH_PARTY_REACTION` and `WATCH_PARTY_MESSAGE` (user-scoped dispatches to the participants; payloads in `docs/specs/watch-parties.md` section 7.3).

`SR_LIBRARY_UPDATE` is addressed to one account's own sessions (it is a user-scoped dispatch, like `RELATIONSHIP_ADD`): `d` is a flat object whose `type` says what changed, ids are strings. Types: `progress` (`chapter_id`, `series_id`, `page_index`, `percent`, `cfi`, `completed`, `updated_at_ms`), `progress_cleared`, `read_marks` (`series_id`, `chapter_id`, `read`, `up_to`), `bookmark_set` / `bookmark_removed` (the bookmark), `follow` (`series_id`, `shelf`, `notify`) / `unfollow`, `upload` (the upload object, to the publisher who started it) and `chapter_published` (`series_id`, `series_title`, `chapter_id`, `chapter_number`, `chapter_title`, `published_at_ms`, to followers with the bell on). Clients treat it as a hint and fall back to the REST reads; see `docs/specs/library-reader.md` section 6.

`SR_CINEMA_UPDATE` is the Cinema twin of `SR_LIBRARY_UPDATE`: addressed to one account's own sessions, flat `d` with a `type` and string ids. Types: `progress` (`title_id`, `episode_id` (`"0"` for a movie), `position_secs`, `duration_secs`, `percent`, `completed`, `updated_at_ms`), `progress_cleared`, `watched` (`title_id`, `watched`, `changed`), `history_cleared`, `history_settings` (`paused`), `list_item_added` / `list_item_removed` (`list_id`, `title_id`), `list_changed` (`list_id`, `name`, `visibility`, `item_count`, `updated_at_ms`), `list_deleted`, `rating` (`title_id`, `episode_id`, `score`) and `rating_removed`. Clients treat it as a hint and refetch; see `docs/specs/cinema-media.md` section 8.

`SR_ACTIVITY_UPDATE` carries "watching" presence to the friends of the person watching (and, for the user setting `cinema.share_watching = everyone`, to people who share a server; `off` sends nothing). `d`: `{type: "watching", user_id, activity, title_id?, episode_id?, expires_at_ms}` or `{type: "stopped", user_id}`. `activity` is a Discord "Watching" activity object (`type` 3, `name` "SIDE ROOM Cinema", `details` the title, `state` "S1 E3 · Episode title", `timestamps.start`). Age-restricted titles are masked (no title, no ids) and never sent beyond friends. The record expires 45 s after the last player heartbeat, so a client drops the activity at `expires_at_ms` unless refreshed. Folding the activity into `PRESENCE_UPDATE.activities` is a gateway follow-up.

`SR_SUBMISSION_UPDATE` is also a user-scoped dispatch, addressed to the submitter (and, for `request_fulfilled`, to everybody who upvoted the request). `d` is a flat object: `type`, `submission_id`, `kind` (`library.series`, `library.items`), `state`, `title`, `at_ms`, and for the decisions a `reason`. Types: `claimed` (a reviewer picked it up), `comment` (a public note from a reviewer, `reason` is its text), `changes_requested`, `approved`, `rejected`, `published`, and `request_fulfilled` (`request_id`, `submission_id`, `title`, `section`). Nothing is sent for a reviewer's internal notes. Clients treat it as a hint and refetch `GET /submissions/{id}`; the REST timeline is the source of truth. See `docs/specs/submissions-and-review.md` section 9.

`SR_PAYMENT_UPDATE` is a user-scoped dispatch addressed to the payer's own sessions (donations, `docs/specs/donations.md`). `d` is a flat object: `type` (`invoice`), `invoice_id`, `state` (`open`, `seen`, `confirming`, `confirmed`, `overpaid`, `late_paid`, `underpaid`, `expired`, `cancelled`, `resolved`), `confirmations`, `confirmations_required`, `received_sats`, `confirmed_sats`, `credited`, `late`. Sent on creation, on every observed change (a payment seen, a block, a reorg that reverts a credit) and when staff resolve the invoice. A hint only: clients refetch `GET /donations/invoices/{id}` (and poll it every 15 s as a fallback while the invoice is open). This is the SIDE ROOM name for the `SR_INVOICE_UPDATE` that doc 08 section 1 originally planned.

`SR_ENTITLEMENT_UPDATE` has two shapes. To one user's sessions: `{type:"user", user_id, reason, patron:{active, in_grace, permanent, until_ms}, limits}` where `limits` is the full `effective_limits` object outside any server (`granted`, `revoked`, `credited`, `reverted`, `expiring`, `grace` and `lapsed` are the reasons); to the members of a server (guild topic): `{type:"guild", guild_id, level, pool_cents, changed}`. Consumers (the voice service re-evaluating sender limits, upload checks) drop their cached `Entitlements` entries on it and read the new limits; the in-process cache is invalidated by the same code path that emits the event.

Rich presence for watching/reading uses standard `activities` on `PRESENCE_UPDATE` (type 3 "Watching", plus a custom type for "Reading"), so it renders like Discord's activity cards.

`GUILD_SCHEDULED_EVENT_CREATE`, `_UPDATE` and `_DELETE` carry the event object; `_USER_ADD` and `_USER_REMOVE` carry `{guild_scheduled_event_id, user_id, guild_id}`. They need the `GUILD_SCHEDULED_EVENTS` intent (`1 << 16`) and go to every member of the server. Voice joins and leaves also travel on the internal event bus (`VoiceJoined`, `VoiceLeft`, per guild channel, moves are a leave then a join) for feature modules such as join-to-create; clients keep using `VOICE_STATE_UPDATE`.

### 6.4 Session lifecycle
`HELLO(heartbeat_interval=41250)` → `IDENTIFY(token, intents, properties)` → `READY(session_id, resume_gateway_url, user, guilds (unavailable stubs), private_channels, relationships, read_state, user_settings, sessions)`. `private_channels` entries carry full `recipients` user objects (everyone but the viewer), the same shape as `GET /users/@me/channels` and the DM `CHANNEL_CREATE` (D-181); a conversation in the viewer's Message Requests also carries `is_message_request: true`. The gateway records every frame size in the `sr_gateway_dispatch_bytes` histogram (`/internal/metrics`) so the "p95 above 64 KiB" trigger for browser zstd is measurable → `GUILD_CREATE` per guild. Resume buffers are kept in memory per session for 2 minutes or 5,000 events.

### 6.5 Voice (gateway ops, dispatches, REST)
The media signalling (voice WebSocket) is in `docs/specs/lounge-voice-video.md` section 4. This section is the gateway and REST side.

**Op 4 `VOICE_STATE_UPDATE` (client→gateway).** `{guild_id, channel_id, self_mute, self_deaf, self_video?, self_stream?}`; `channel_id: null` leaves voice. Sending the current channel again with different `self_mute`/`self_deaf` only updates those flags. A different `channel_id` in the same guild moves; a channel in another guild first leaves the old one (a user has one voice connection at a time, as on Discord). The gateway stores voice state **in memory on the node** that holds the session (lost on restart; see the spec, section 4.1). Without `guild_id` (or with `null`) `channel_id` names a DM or group DM and the join is a **call** (section 6.6). `self_stream` is ignored: only `STREAM_CREATE` sets it, after the checks. `self_video` goes through the sender allowance; a refusal keeps the join and answers `SR_VOICE_ERROR` (`video_limit_reached`, with `limit` and `count`; `missing_permission` when `STREAM` is missing).

**Dispatch `VOICE_STATE_UPDATE`.** Discord's voice state object: `{guild_id, channel_id, user_id, member?, session_id, deaf, mute, self_deaf, self_mute, self_video, self_stream, suppress, request_to_speak_timestamp}`; `channel_id: null` means the user left. Sent to every session in the guild that holds `VIEW_CHANNEL` on the old or the new channel (intent `GUILD_VOICE_STATES`, bit 7; user clients get it always) and always to the affected user's own sessions. `GUILD_CREATE` carries `voice_states` (same objects without `guild_id`, only channels the recipient can see).

**Dispatch `VOICE_SERVER_UPDATE`** (to the joining session only; also after a move): `{token, guild_id, channel_id, endpoint, ice_servers}`. See the spec, section 4.2.

**Dispatch `SR_VOICE_ERROR`** (to the session that sent op 4, when the join or update was refused): `{guild_id, channel_id, code, message}`. Codes: `unknown_channel`, `not_voice_channel`, `not_member`, `missing_permission` (no `VIEW_CHANNEL`/`CONNECT`), `channel_full`, `age_confirmation_required` (age-restricted channel, `POST /sr/age-gate` then retry), `video_limit_reached`, `stream_quality_not_allowed`, `stream_full`, `unknown_stream`, `not_in_voice`, `already_streaming`, `voice_unavailable` (no SFU configured), `bad_request`. Extra fields (`limit`, `count`, `allowed`) are merged into the body where a toast needs them. Discord stays silent here; the code lets the client show a reason.

**Permissions.** Join: `VIEW_CHANNEL` and `CONNECT` on the channel (timed-out members lose `CONNECT`); a channel with `user_limit` rejects further joiners with `channel_full` unless they hold `MOVE_MEMBERS`, `MANAGE_CHANNELS` or `ADMINISTRATOR`. Speak: `SPEAK`, no server mute. Voice activity: without `USE_VAD` the member must use push-to-talk and the SFU enforces it (D-046). `PRIORITY_SPEAKER` sets the priority speaking bit.

**REST.** `PATCH /guilds/{guild_id}/voice-states/{user_id}` with `{channel_id?: string|null, mute?: bool, deaf?: bool, suppress?: bool, request_to_speak_timestamp?: string|null, reason?: string}` (also `X-Audit-Log-Reason`):
- `suppress` (stage channels only, `MUTE_MEMBERS`): `false` invites the member to speak, `true` sends a speaker back to the audience; on `` with a `channel_id` (the stage the caller is in) and `request_to_speak_timestamp` (any ISO time raises a hand, `null` lowers it; needs `REQUEST_TO_SPEAK`) it is the member's own stage state (section 6.6);
- `mute` needs `MUTE_MEMBERS` on the user's current channel, `deaf` needs `DEAFEN_MEMBERS`;
- `channel_id: "<id>"` moves the user and needs `MOVE_MEMBERS` on the current channel and on the target (plus `CONNECT` on the target for the actor), the target must be a voice channel in the same guild;
- `channel_id: null` disconnects and needs `MOVE_MEMBERS`;
- the target must be in voice (otherwise 400 `Target user is not connected to voice`, Discord code 40032). `@me` as `user_id` is accepted for the actor's own mute/deaf only.
Each change is written to the guild audit log (`MEMBER_MOVE` 26 with `channel_id`/`count`, `MEMBER_DISCONNECT` 27 with `count`, `MEMBER_UPDATE` 24 with `mute`/`deaf` changes) and dispatched as `VOICE_STATE_UPDATE`. Server mute and deafen survive moves inside the voice session and are cleared when the user leaves voice (in-memory state; persistence is a follow-up).

`GET /sr/voice/turn` returns coturn ephemeral credentials `{username, password, ttl, uris}` (HMAC-SHA1, TTL 1 h, `403` with `SR_FEATURE_DISABLED` (900001) when TURN is not configured). `ice_servers` in `VOICE_SERVER_UPDATE` carries the same credentials.

### 6.6 Lazy member list (op 102, `GUILD_MEMBER_LIST_UPDATE`)
The member sidebar of a server is served in ranges instead of with the whole `GUILD_CREATE` member list (LNG-GLD-010). Sessions whose intents include `GUILD_MEMBERS` (user clients always) send

`{"op": 102, "d": {"guild_id": "<id>", "channels": {"<channel_id>": [[0, 99], [100, 199]]}}}`

for at most 5 channels, 8 ranges each, at most 200 items per range. A channel the session cannot see, or a server it is not in, is ignored; a channel with an empty or invalid range list is unsubscribed; sending a new request replaces the ranges and re-syncs. The gateway answers each channel with one `GUILD_MEMBER_LIST_UPDATE`:

`{guild_id, id, member_count, online_count, groups: [{id, count}], ops: [...]}`

where `id` is `"everyone"` when every member of the server can see the channel and the channel id otherwise, and `groups` are in list order. The list is Discord's: members who can see the channel, under their highest hoisted role (online members only, roles by position, highest first), then `online`, then `offline`; each group starts with an item `{"group": {"id", "count"}}`, members are items `{"member": <member object + presence>}` sorted by nickname, display name or username (case-insensitive). `ops` are applied to the client's slice in order: `SYNC {range, items}` (all requested ranges at the first answer, and again when a change would cost as many ops as the range has rows), then `UPDATE {index, item}`, `INSERT {index, item}` and `DELETE {index}` with absolute list indices. Updates follow joins, leaves, role, profile and channel changes (members refetched) and presence changes (read live); bursts are coalesced into one refresh per server (75 ms). Servers above 20,000 members are not served lazily.

### 6.7 Temporary membership
A member who joined through a temporary invite and holds no role is removed once all their sockets have been closed for a minute (`GUILD_MEMBER_REMOVE`), judged from the same presence the gateway broadcasts (`GuildDirectory::user_offline`).
### 6.8 Streams, stages, calls and the soundboard
Specified in `docs/specs/lounge-voice-video.md` (sections 4.9 to 4.12, 7, 8a, 8b); this is the wire summary. All of it needs voice to be enabled, otherwise `SR_FEATURE_DISABLED` (900001) or `SR_VOICE_ERROR voice_unavailable`.

**Go Live (client ops).** `18 STREAM_CREATE {channel_id, guild_id?, height?, fps?}`, `19 STREAM_DELETE {stream_key}`, `20 STREAM_WATCH {stream_key}`, `22 STREAM_SET_PAUSED {stream_key, paused}`. Stream key: `guild:{guild}:{channel}:{user}` or `call:{channel}:{user}`. Dispatches: `STREAM_CREATE {stream_key, rtc_server_id, region, viewer_ids, paused, max_viewers, quality}` (to the streamer on start, to a viewer on watch), `STREAM_UPDATE` (same body, to the streamer and viewers when viewers or pause change), `STREAM_DELETE {stream_key, reason, unavailable}`; the voice state shows `self_stream`. `SR_VOICE_LIMITS_UPDATE {guild_id, video_senders, stream_viewers, exempt}` after a server level change. `POST /streams/{stream_key}/preview {thumbnail: "data:image/jpeg;base64,..."}` (streamer; 204; 400 not a JPEG, 413 over 256 KiB, 429 within 5 s of the last) and `GET /streams/{stream_key}/preview` (`image/jpeg`, ETag).

**Stage instances.** `POST /stage-instances {channel_id, topic, privacy_level?: 2}`, `GET|PATCH|DELETE /stage-instances/{channel_id}`, `GET /guilds/{guild_id}/stage-instances`; dispatches `STAGE_INSTANCE_CREATE`, `STAGE_INSTANCE_UPDATE`, `STAGE_INSTANCE_DELETE` (intent `GUILDS`). Errors: 10067 unknown stage instance, 150006 a stage is already open. Request to speak, invite to speak and stepping down are `PATCH .../voice-states/...` as above.

**DM calls.** Join by op 4 without `guild_id`. Dispatches `CALL_CREATE {channel_id, message_id, region, ringing, initiator_id, group, voice_states}`, `CALL_UPDATE {channel_id, message_id, region, ringing, initiator_id, group}`, `CALL_DELETE {channel_id}` to every member of the DM. `GET /channels/{channel_id}/call`, `POST /channels/{channel_id}/call/ring {recipients?}`, `POST /channels/{channel_id}/call/stop-ringing {recipients?}`. The finished call is a message of type 3 with `call: {participants, ended_timestamp}`.

**Soundboard.** `GET /soundboard-default-sounds`, `GET /soundboard-default-sounds/{id}/audio`; `GET|POST /guilds/{id}/soundboard-sounds`, `GET|PATCH|DELETE /guilds/{id}/soundboard-sounds/{sound_id}`, `GET .../{sound_id}/audio`; `POST /channels/{id}/send-soundboard-sound {sound_id, source_guild_id?}`. Dispatches `GUILD_SOUNDBOARD_SOUNDS_UPDATE {guild_id, soundboard_sounds}` (intent `GUILD_EXPRESSIONS`), `VOICE_CHANNEL_EFFECT_SEND {channel_id, guild_id, user_id, emoji, animation_type, animation_id, sound_id, sound_volume}` (intent `GUILD_VOICE_STATES`), and `SOUNDBOARD_SOUNDS {guild_id, soundboard_sounds}` in answer to op 31. Errors: 10097 unknown sound, 30045 sound slots full, 900012 cooldown (429).


## 7. Bot compatibility checklist (porting guide)

Published at `docs/developers/porting-from-discord.md` (Phase 6):
1. Override the REST base URL and gateway URL (all major libraries support this).
2. Use a SIDE ROOM bot token.
3. Unsupported or different: Nitro/SKU/monetization endpoints (return 404), Stage Discovery, Discord-specific integrations (Twitch/YouTube), and `cdn.discordapp.com` URLs (use `<host>/cdn`).
4. Extra capabilities: the `/sr/*` APIs with scopes `sr.cinema.read`, `sr.library.read`, `sr.parties`.
5. Channel listings are permission-filtered, like Discord: `GET /guilds/{id}/channels` returns only the channels the caller can view; the owner, `ADMINISTRATOR` and bots holding it see all (D-160). A bot that assumed it sees every channel with only `VIEW_CHANNEL` missing from a role must grant the permission or expect gaps.

A conformance test suite (`crates/sr-compat-tests`) runs real discord.js and serenity example bots against a test server in CI (Phase 6). It includes a permission-filtered listing case: a bot without `VIEW_CHANNEL` on a private channel must not see it in `GET /guilds/{id}/channels`, and the same bot with `ADMINISTRATOR` must (D-160).
