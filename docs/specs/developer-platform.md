# Developer platform (applications, bots, OAuth2, commands, interactions)

Status: backend built on `feat/p8-dev-be` (inventory DEV-APP, DEV-BOT, DEV-SDK backend parts). Governing docs: [11](../11-extensibility-and-developer-platform.md), [05](../05-api-and-gateway.md), [07](../07-permissions-and-staff.md), open question 56. The web side (developer portal, consent screen, command picker) is coded against section 3 of this file.

This file has two parts: how the platform behaves (1 and 2) and **HTTP API as built** (3), the contract the web agent codes against. Paths are relative to `/api/v1` (`/api/v10` is an alias). Every id is a snowflake string. Timestamps are ISO8601. Errors use the Discord body `{code, message, errors?}`; OAuth2 token endpoints use the RFC 6749 body (section 3.7).

## 1. Model

* **Developer** = an account with the `APP_DEVELOPER` flag (`accounts.flags` bit 1). Any member can apply (`POST /developers/apply`); staff holding `REVIEW_APPS` approve or reject. Doc 11 section 2.1 lets people create applications before approval, so the flag is a trust marker and gates nothing in this phase except the staff list.
* **Application** (`applications`): owned by one account (no teams: the docs do not specify them, so they are not built). Creating an application also creates its **bot account**: a row in `accounts` with `is_bot = true`, an unusable password hash, a synthetic address `bot-<id>@bots.invalid`, and username `bot_<snowflake>`. The display name (`global_name`) is the application name. Bot accounts never count toward the account cap (D-013), never log in (no password verifies, `POST /sr/auth/login` answers like any wrong password) and cannot be registered over.
* **Release state** (`release_state`): `development` (default), `in_review`, `released`, `suspended`.
  * `development`: the bot can be installed only by the **owner**, into a guild where the owner holds `MANAGE_GUILD` (or owns it) and which has **25 members or fewer** (doc 11 section 2.1). Any other install is refused with `900120` (HTTP 403, "App not released").
  * Owners submit a **version** (`POST /applications/{id}/versions`) which snapshots the manifest (name, description, scopes, redirect URIs, requested privileged intents, command list, URLs) and moves the application to `in_review`. A staff decision (`REVIEW_APPS`) approves it (`released`; the approved intents are set by the reviewer) or rejects/asks for changes (back to `development`, or stays `released` for later versions). A later version that changes scopes, redirect URIs or requested intents is flagged `needs_review` with the reason; the diff against the previously approved manifest is returned on every version.
  * `suspended` (staff action): bot tokens and OAuth tokens stop authenticating, the gateway refuses IDENTIFY for the bot, new interactions are refused. Nothing is deleted; `unsuspend` restores the previous state.
* **Bot public flag** (`bot_public`): when false only the owner may add the bot to a server (even when released). `bot_require_code_grant` is stored and returned but only informational (the authorize flow always returns a code when a `redirect_uri` is given).
* **Privileged intents.** `GUILD_MEMBERS` (1<<1), `GUILD_PRESENCES` (1<<8), `MESSAGE_CONTENT` (1<<15). The developer toggles `intents_requested` on the bot settings. What the gateway allows on IDENTIFY: in `development` the requested set (test servers only, so low risk); in `released` only `requested AND approved` (staff approve with a version decision). IDENTIFY with a bit outside the allowed set closes with `4014`. The application `flags` field reports Discord's `GATEWAY_*` / `GATEWAY_*_LIMITED` bits accordingly.
* **Credentials** (all random, all stored as SHA-256 hashes with a per-kind domain label; raw values are shown once):

  | Credential | Format | Stored | Where it works |
  |---|---|---|---|
  | Bot token | Discord shape `base64url(bot user id).base64url(issued unix s).base64url(32 random bytes)` (open question 56) | `bot_tokens.token_hash`, `revoked_at` | `Authorization: Bot <token>`, gateway IDENTIFY `token` (without the `Bot ` prefix, as Discord libraries send it). One live token per application: a reset revokes the old one. |
  | Client secret | 43-char base64url (256 bits) | `applications.client_secret_hash` | `/oauth2/token`, `/oauth2/token/revoke`, `/oauth2/introspect` client authentication |
  | OAuth2 authorization code | 43 chars, 5 minutes, single use | `oauth2_codes.code_hash` | `/oauth2/token` |
  | OAuth2 access / refresh token | 43 chars each; access 7 days, refresh 90 days, refresh tokens rotate | `oauth2_tokens.access_hash`, `refresh_hash` | `Authorization: Bearer <token>`; refresh at `/oauth2/token` |
  | Interaction token | 43 chars, valid 15 minutes | `interactions.token_hash` | `/interactions/{id}/{token}/callback`, `/webhooks/{application_id}/{token}[/messages/...]` |

  Hash lookups go through a unique index on the digest and the stored digest is compared again in constant time before a match is accepted. Secrets never appear in `Debug`, logs or audit rows.
* **Install** (`application_installs`, one row per application and guild): created by the authorize flow. With the `bot` scope the bot account joins the guild with a **managed role** (`managed = true`, `tags.bot_id`) that holds the granted permissions. Granted permissions = requested `permissions` AND the installer's own guild permissions (an installer without `ADMINISTRATOR` cannot hand out `ADMINISTRATOR`, or anything else they lack); this is stricter than Discord on purpose (no privilege escalation). An `audit_log_entries` row `BOT_ADD` (28) is written, `GUILD_MEMBER_ADD` and `GUILD_ROLE_CREATE` are dispatched, and the application's audit trail records the install. `applications.commands` without `bot` installs the commands only (no member).
* **Authorization policy (who may call what)**, enforced centrally in the `Principal` extractor, not per handler:
  * **Session** (browser cookie + `X-SR-CSRF`): everything for the signed-in user. Owner-only operations (create/modify/delete applications, reset secret/token, versions, the authorize POST) accept sessions only; bots, bearers and anyone else get `20001`/`50001`.
  * **Bot** (`Authorization: Bot`): the Discord-compatible bot routes. Denied outright (`20001`, HTTP 403): `/admin/*`, `/developers/*`, `/sr/auth/*`, `/oauth2/authorize`, owner-only `/applications` management routes, user-account routes (`/users/@me/settings`, `/users/@me/relationships*`, `/users/@me/notes`...). A bot can only ever act with the guild/channel permissions of its roles; every route re-checks them.
  * **Bearer** (`Authorization: Bearer`): default deny. Only these routes accept it: `GET /users/@me` (`identify`; the `email` field needs `email`), `GET /users/@me/guilds` (`guilds`), `GET /oauth2/@me`, `PUT /applications/{id}/guilds/{guild_id}/commands/permissions` and `.../{command_id}/permissions` (`applications.commands.permissions.update`). Everything else answers `50001` "Missing Access".

## 2. Compatibility with Discord

Compatible: the application, user, member, role and message object shapes used here; bot token format and `Authorization: Bot`; gateway IDENTIFY/READY/RESUME, privileged-intent close codes `4013`/`4014`, `GET /gateway/bot`; application command objects, option types 1-11, command limits (100 chat-input, 15 user, 15 message commands per application and scope, 200 command creations per application, guild and day is **not** enforced), bulk overwrite, command permissions v2 semantics; interaction types 2-5 and callback types 4-9 (type 1 PONG accepted, 12 refused); `INTERACTION_CREATE` payload; ephemeral flag `64`; `/interactions/{id}/{token}/callback`; `/webhooks/{application_id}/{token}` follow-ups and `/messages/@original|{id}`; the 3 second initial-response window and the 15 minute token life; OAuth2 authorization code flow with PKCE (`S256`), `state`, `identify`/`email`/`guilds`/`bot`/`applications.commands` scopes, `/oauth2/token`, `/oauth2/token/revoke`, `/oauth2/@me`; `X-RateLimit-*` headers and the global 50 requests per second per bot.

Different or missing (see also doc 05 section 7):

* Teams, app testers, application emoji, role connections, rich presence assets, activities (`DEV-ACT-*`), monetization/SKUs: not built.
* No HTTP interactions endpoint and no Ed25519 verification (`DEV-BOT-005`); interactions are delivered over the gateway only. `interactions_endpoint_url` and `verify_key` are not part of the object.
* No sharding: IDENTIFY `shard` must be absent or `[0, 1]` (else close `4010`); `GET /gateway/bot` reports `shards: 1`.
* Installs: only server installs. `guilds.join`, `guilds.members.read`, `connections`, `webhook.incoming`, `rpc*`, `sr.*` scopes are rejected with `invalid_scope`. Redirect URIs must be `https`, or `http` on `localhost`/`127.0.0.1`/`[::1]`; custom schemes are refused. Only `response_type=code`; no implicit grant, no `client_credentials`.
* Interaction responses: a non-ephemeral response is a normal message sent **as the bot**, so it needs the bot's own `SEND_MESSAGES` (and the other send permissions) in the channel; Discord lets the token bypass this. Ephemeral messages are never stored: they are dispatched to the invoking user's sessions only (`MESSAGE_CREATE`/`MESSAGE_UPDATE` with flag 64) and kept 15 minutes so the bot can edit them. A deferred response (`5`, `6`) shows "is thinking" to the invoking user only (via `INTERACTION_SUCCESS`); other members see the message when the bot posts it. Interactions work in servers only (no DM or user installs).
* Components: buttons only (custom_id/style/label/url/disabled), as far as the existing message model stores them. Select menus and modal text inputs pass through the interaction payload (`values`, modal `components`) but the stored message model does not keep select options. Modals are delivered to the invoking user as `INTERACTION_MODAL_CREATE`.
* Message `interaction` metadata and `application_id` on messages are not stored.
* Bot token reset does not disconnect running gateway sessions of the old token; they end at the next reconnect (a revoked token cannot IDENTIFY or RESUME).

## 3. HTTP API as built

Auth column: **S** = browser session (CSRF on writes), **B** = bot token of the named application, **U** = user bearer token, **St** = staff session with the platform permission shown, **-** = none. "Owner" means the application's `owner_id`; anyone else gets `10002 Unknown Application` (404, the same answer as a missing id, so ids cannot be probed).

### 3.1 Developer onboarding

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `POST /developers/apply` | S | `{pitch (1-2000), prior_work? (0-2000), accepted_terms: true}` | 201 `DeveloperRequest`. 409 `900010` if one is pending or the account is already a developer. 400 field errors |
| `GET /developers/status` | S | | `{is_developer: bool, request: DeveloperRequest\|null}` (latest request) |
| `GET /admin/developers/requests?state=pending\|approved\|rejected&limit&before` | St `REVIEW_APPS` | | `[DeveloperRequest + account:{id,username,global_name}]`, newest first, default `pending` |
| `POST /admin/developers/requests/{id}/decision` | St `REVIEW_APPS` | `{approve: bool, note?}` (note required on reject) | `DeveloperRequest`. Approve sets `APP_DEVELOPER`. 409 if already decided. Written to the staff access log |

`DeveloperRequest`: `{id, account_id, state: "pending"|"approved"|"rejected"|"suspended", pitch, prior_work, created_at, reviewed_at|null, reviewer_note|null}`.

### 3.2 Applications (owner portal)

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `POST /applications` | S | `{name (2-32), description? (0-400)}` | 201 `{application: Application, client_secret, bot_token}`; the two secrets are shown **once**. 400 field errors; 400 `30000`-style "max 25 applications" as `InvalidFormBody` on `name` |
| `GET /applications` | S | | `[Application]` (mine, oldest first) |
| `GET /applications/{id}` | S owner | | `Application` |
| `PATCH /applications/{id}` | S owner | any of `name`, `description`, `icon` (image data URI or null), `tags` (<=5, 1-20 chars), `bot_public`, `bot_require_code_grant`, `redirect_uris` (<=10, validated, see 3.3), `install_params` `{scopes:[], permissions:"<bits>"}`, `terms_of_service_url`, `privacy_policy_url` (https, <=512), `privileged_intents` (array of `GUILD_MEMBERS`/`GUILD_PRESENCES`/`MESSAGE_CONTENT` = requested set) | `Application`. Renaming also renames the bot's `global_name`. Audit row `app.update` |
| `PATCH /applications/{id}/bot` | S owner | `{username}` (2-32, bot display name) | `BotUser` |
| `POST /applications/{id}/reset-secret` | S owner | `{password}` (re-confirmation, `900004` on a wrong password) | `{client_secret}` (once). The old secret stops working immediately. Audit `app.secret_reset` |
| `POST /applications/{id}/bot/reset-token` | S owner | `{password}` | `{token}` (once). Old token revoked. Audit `app.token_reset` |
| `DELETE /applications/{id}` | S owner | `{password}` | 204. Removes the bot from every server, revokes tokens, grants and installs, deletes commands; the bot account is kept as a deleted account so old messages keep an author. Audit `app.delete` (survives the delete) |
| `GET /applications/{id}/audit-log?limit&before` | S owner | | `[{id, actor_id, action, details, created_at}]`, newest first, limit 1-100 (default 50) |
| `GET /applications/{id}/installs` | S owner | | `[{guild_id, installed_by, scopes, permissions, created_at}]` |
| `GET /applications/{id}/versions` | S owner | | `[AppVersion]`, newest first |
| `POST /applications/{id}/versions` | S owner | `{note?}` | 201 `AppVersion` (manifest snapshot of the current settings). 409 `900010` when one is already pending or the application is suspended |
| `POST /applications/{id}/versions/{version_id}/withdraw` | S owner | | `AppVersion` (state `withdrawn`) |
| `GET /admin/developers/versions?state=pending&limit&before` | St `REVIEW_APPS` | | `[AppVersion + application:{id,name,owner_id}]` |
| `POST /admin/developers/versions/{id}/decision` | St `REVIEW_APPS` | `{decision: "approve"\|"reject"\|"request_changes", note?, approved_intents?: [names]}` | `AppVersion`. Approve: `release_state = released`, `intents_approved = approved_intents` (default: none; only the three privileged names are accepted). Note required for reject/request_changes |
| `POST /admin/developers/apps/{id}/suspend` | St `REVIEW_APPS` | `{reason}` | `Application`. All of the app's bot and OAuth tokens stop authenticating |
| `POST /admin/developers/apps/{id}/unsuspend` | St `REVIEW_APPS` | `{reason?}` | `Application` (back to `development` or `released`, whichever it was) |
| `GET /applications/@me` | B | | `Application` (public subset: no `redirect_uris`, `install_params`) |

`Application`: `{id, name, icon|null, description, tags[], bot_public, bot_require_code_grant, flags, release_state, owner:{id,username,global_name,bot:false}, bot:BotUser, redirect_uris[], install_params:{scopes[],permissions}, terms_of_service_url|null, privacy_policy_url|null, privileged_intents:{requested:[names], approved:[names]}, client_secret_hint:"…abcd"|null, created_at}`. `client_secret` itself is never returned again.

`BotUser`: `{id, username, global_name, avatar|null, bot: true}`.

`AppVersion`: `{id, application_id, version (int, 1-based), state: "pending"|"approved"|"rejected"|"changes_requested"|"withdrawn", manifest, needs_review: bool, needs_review_reasons[] ("scopes"|"redirect_uris"|"intents"|"first_release"), diff:{changed:[{key, old, new}]}, note|null, reviewer_note|null, created_at, decided_at|null}`.

### 3.3 OAuth2 and installs

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `GET /oauth2/authorize?client_id&scope&redirect_uri?&response_type=code&state?&code_challenge?&code_challenge_method=S256&permissions?&guild_id?` | S | query | Consent screen data: `{application:{id,name,icon,description,bot_public,release_state,privacy_policy_url,terms_of_service_url}, bot:BotUser\|null, scopes:[{name,description}], redirect_uri\|null, state, user:{id,username,global_name}, guilds:[{id,name,icon,owner,permissions,can_install,reason?}], permissions:"<bits>", already_authorized: bool}`. `guilds` is only filled when `scope` has `bot` or `applications.commands`; `can_install` is true only with `MANAGE_GUILD` (or owner) and, in `development`, only for the owner in a guild with <=25 members (`reason`: `"missing_permission"`, `"app_not_released"`, `"bot_not_public"`). An unknown client, a redirect URI that is not registered or an invalid scope is a **400 JSON error, never a redirect** |
| `POST /oauth2/authorize` | S | `{client_id, scope, redirect_uri?, response_type:"code", state?, code_challenge?, code_challenge_method?, permissions?, guild_id?, authorize: bool}` | `{location}`: the URL to send the browser to. Approve: `redirect_uri?code=…&state=…&guild_id=…`, or `/oauth2/authorized` when there is no redirect URI. Deny (`authorize:false`) with a valid redirect: `redirect_uri?error=access_denied&state=…`. With `bot`: `guild_id` is required, the caller must hold `MANAGE_GUILD` there; the install (3.2 model) happens before the redirect. 403 `50013` without the permission, 403 `900120` for an unreleased app, 403 `50001` for a private bot of someone else, 409 if the bot is already in the guild (the install is then just refreshed: same answer with `already_installed: true` in an extra field) |
| `POST /oauth2/token` | client auth | form or JSON: `grant_type=authorization_code` with `code`, `redirect_uri`, `code_verifier?`; or `grant_type=refresh_token` with `refresh_token`. Client auth: HTTP Basic `client_id:client_secret`, or `client_id`/`client_secret` in the body. A request with no secret is accepted only for a code that was issued with a `code_challenge` and carries the matching `code_verifier` | `{access_token, token_type:"Bearer", expires_in, refresh_token, scope}` (space-separated). Errors per RFC 6749 (3.7). 30 requests/min per client and per address |
| `POST /oauth2/token/revoke` | client auth | `{token, token_type_hint?}` (form or JSON) | 200 `{}` for any input (RFC 7009); only tokens of the authenticated client are revoked |
| `POST /oauth2/introspect` | client auth | `{token, token_type_hint?}` | RFC 7662: `{active:false}` or `{active:true, scope, client_id, username, sub, exp, iat, token_type:"Bearer"}`; only the issuing client may introspect its tokens (others get `{active:false}`) |
| `GET /oauth2/@me` | U | | `{application: Application(public), scopes[], expires, user?:UserObject}` (`user` needs `identify`) |
| `GET /oauth2/tokens` | S | | `[{id, application:{id,name,icon}, scopes[], created_at}]` the apps the user authorized |
| `DELETE /oauth2/tokens/{grant_id}` | S | | 204; revokes the grant and every token of it. Does not remove an installed bot |

Redirect URI rules: absolute, `https`, or `http` for `localhost`, `127.0.0.1`, `[::1]`; no fragment, no userinfo, <=512 chars. Matching at authorize and token time is an exact string comparison against the registered list.

PKCE: only `S256`. If a `code_challenge` was sent the verifier is mandatory at the token endpoint and compared in constant time (`BASE64URL(SHA256(verifier))`, verifier 43-128 chars of `[A-Za-z0-9-._~]`).

Code and refresh handling: a used code presented again revokes every token minted from it. A used refresh token presented again revokes the whole grant (rotation reuse detection).

`GET /users/@me` for a bearer returns `id`, `username`, `global_name`, `avatar`, `bot:false` (+ `email` and `verified` with the `email` scope); `GET /users/@me/guilds` needs `guilds`.

### 3.4 Gateway for bots

* `GET /gateway/bot` (B): `{url:"wss://<host>/gateway", shards:1, session_start_limit:{total:1000, remaining:1000, reset_after, max_concurrency:1}}`.
* IDENTIFY with a bot token: `intents` is required; unknown bits close `4013`; privileged bits not allowed close `4014`; `shard` other than `[0,1]` closes `4010`. READY carries `user` (with `bot:true`), `application:{id, flags}`, `guilds` (unavailable stubs) as usual.
* Message events: without `MESSAGE_CONTENT` the bot receives `content`, `embeds`, `attachments`, `components` and `poll` blanked, except for DMs, messages that mention it and its own messages (doc 05 section 6.2).
* `INTERACTION_CREATE` is delivered to the bot user's sessions (no intent needed). `GUILD_CREATE` follows an install.

### 3.5 Application commands (B, `application_id` must be the token's application, or `@me` is **not** accepted)

Global: `GET/POST/PUT /applications/{app}/commands`, `GET/PATCH/DELETE /applications/{app}/commands/{id}`. Guild: the same under `/applications/{app}/guilds/{guild}/commands[...]`; the application must be installed in the guild (else `50001`). `GET` takes `with_localizations=true`.

Command body: `name` (chat input: 1-32 of lowercase letters/digits/`-`/`_`; user/message commands 1-32 with spaces and capitals allowed), `type` (1 chat input default, 2 user, 3 message), `description` (chat input 1-100; user/message must be empty), `options` (<=25; types 1-11; sub-command nesting max two levels: group > sub-command > options; required options first; <=25 choices; choices and `autocomplete` are mutually exclusive; `min_value`/`max_value`, `min_length` 0-6000, `max_length` 1-6000, `channel_types`), `name_localizations`/`description_localizations` (locale to string, same length rules), `default_member_permissions` (bit string or null), `dm_permission` (accepted, stored; no DM interactions yet), `nsfw`, `contexts`, `integration_types` (stored). The serialized command may not exceed 8000 characters. `POST` with an existing name and type updates it (200, else 201). `PUT` replaces the whole set for that scope in one transaction and returns it; ids of unchanged names are kept. Limits per scope: 100 chat-input, 15 user, 15 message commands. Errors use `50035` field trees (`options.0.name`...) or `30032`-style "max commands" as `InvalidFormBody` on `name`.

Command object: `{id, application_id, version, default_member_permissions, type, name, name_localizations, description, description_localizations, guild_id?, dm_permission, options?, nsfw, contexts, integration_types}`.

Permissions v2: `GET /applications/{app}/guilds/{guild}/commands/permissions` and `.../{command_id}/permissions` (B of that application) return `{id, application_id, guild_id, permissions:[{id, type (1 role, 2 user, 3 channel), permission: bool}]}`; `PUT .../{command_id}/permissions` `{permissions:[...]}` (<=100 entries) needs a **bearer token** with `applications.commands.permissions.update` of a user holding `MANAGE_GUILD` in the guild (bot tokens and sessions are refused, as on Discord). Evaluation when a member invokes a command (see 3.6): `default_member_permissions` first (unless the member has `ADMINISTRATOR`... Discord: the command default only matters if there is no explicit overwrite), then an explicit user entry, then role entries (deny wins over allow only inside the same level; `@everyone` = guild id), then channel entries (`channel id`, or `guild id - 1` for all channels); the invoker also needs `USE_APPLICATION_COMMANDS` in the channel.

### 3.6 Interactions

| Method and path | Auth | Request | Response |
|---|---|---|---|
| `POST /interactions` | S (the member using the app) | `{type: 2\|3\|4\|5, application_id, channel_id, guild_id?, nonce?, data}` | 204. Validates membership, channel visibility, `USE_APPLICATION_COMMANDS`, install, the command and its permissions, option types against the registered schema, then creates the interaction and dispatches `INTERACTION_CREATE` to the bot user's sessions. 400 `50035`, 403 `50001`/`50013`, 404 `10063` unknown command |
| `POST /interactions/{id}/{token}/callback` | token | `{type, data?}` | 204. Within 3 s of creation, once. 404 `10062` after the window or for an unknown id; 400 `40060` if already acknowledged; 401 `50027` for a wrong token |
| `GET /webhooks/{application_id}/{token}/messages/@original` | token | | the original response message (non-ephemeral: the stored message; ephemeral: its saved payload) |
| `PATCH /webhooks/{application_id}/{token}/messages/@original` or `/{message_id}` | token | `{content?, embeds?, components?, flags?}` | message; for a deferred interaction the first edit **creates** the original |
| `DELETE /webhooks/{application_id}/{token}/messages/@original` or `/{message_id}` | token | | 204 |
| `POST /webhooks/{application_id}/{token}?wait` | token | `{content, embeds, components, flags (64 = ephemeral), tts}` | follow-up message (204 or 200 with `wait=true`). The existing webhook-execute route detects an interaction token first. Tokens work for 15 minutes; only messages that belong to the interaction can be edited or deleted |

Interaction types a user client may create: `2` application command (`data: {id, name?, type?, options?, target_id?}`; `name`/`type` are taken from the stored command, the client's copy is ignored), `3` message component (`data: {custom_id, component_type, values?}`, `message_id` required; the message must be in the channel), `4` autocomplete (`data` like type 2 with one option `focused: true`), `5` modal submit (`data: {custom_id, components}`).

Callback types: `4` message with source, `5` deferred message, `6` deferred update (components only), `7` update message (components only), `8` autocomplete result (`data.choices`, <=25), `9` modal (`data: {custom_id, title (<=45), components}`), `1` pong. Callback `data` message fields: `content` (<=2000), `embeds` (<=10), `components`, `flags` (only `64` and `4` are honoured), `tts`.

Gateway payload to the bot (`INTERACTION_CREATE`): `{id, application_id, type, data, guild_id, channel_id, channel:{id,type}, member:{user, roles[], permissions, joined_at, nick, pending, flags}, token, version:1, message? (components), app_permissions, locale:"en-US", guild_locale, entitlements:[], context:0}`. `data.resolved` carries `users`, `members`, `roles`, `channels` for user/role/channel/mentionable options.

Client-facing dispatches to the invoking user's sessions: `INTERACTION_SUCCESS {id, nonce}` (the bot acknowledged: responded or deferred), `INTERACTION_FAILURE {id, nonce, reason_code: 2}` (no acknowledgement in 3 s), `INTERACTION_MODAL_CREATE {id, nonce, application_id, channel_id, guild_id, custom_id, title, components}`, and for ephemeral output `MESSAGE_CREATE`/`MESSAGE_UPDATE`/`MESSAGE_DELETE` addressed to that user only (flags 64, `id` is a fresh snowflake, `channel_id`, `author` the bot).

### 3.7 Errors

New codes: `10062` Unknown interaction (404), `10063` Unknown application command (404), `40060` Interaction has already been acknowledged (400), `50027` Invalid webhook token (401), `900120` App not released (403), `900121` Application suspended (403). Existing: `10002` Unknown Application, `20001` Bots cannot use this endpoint, `50001`, `50013`, `50035`, `900004` re-auth, `900010` conflict.

OAuth token endpoint errors (`/oauth2/token`, `/revoke`, `/introspect`): HTTP 400 (401 with `WWW-Authenticate: Basic` for `invalid_client` when Basic auth was tried) and `{"error": "invalid_request"|"invalid_client"|"invalid_grant"|"unauthorized_client"|"unsupported_grant_type"|"invalid_scope", "error_description": "..."}`.

### 3.8 Rate limits

All routes share the global `X-RateLimit-*` machinery: 50 requests per second per credential (each bot token is its own identity), `X-RateLimit-Global: true` and `X-RateLimit-Scope: global` on a global 429, `Retry-After`. Extra buckets: `POST /oauth2/token` 30/min per client id and 30/min per address, `POST /oauth2/authorize` 20/min, `POST /interactions` 20/10 s per user, `POST /interactions/{id}/{token}/callback` 5/5 s, `POST /applications` 5/hour per owner, `POST /developers/apply` 3/day. Secrets and token resets 5/hour per application.
