# Handoff: feat/p11-userset-be (phase 11, backend of user settings and profiles)

Worktree `SIDEROOM_wt/p11-userset-be`, base `integration/phase11`, local commits only. One migration: `0660_user_settings_backend.sql`.

## Done
| Item | What |
|---|---|
| LNG-USET-007 | `POST /sr/auth/account/delete {password, username, mfa?}` (session only, 204, clears cookie). Needs password, typed username, second factor when enabled. One transaction (`sr-db/src/account_deletion.rs`): account scrubbed to "Deleted User" (`deleted.<id>`, placeholder email, no profile fields, unusable password, state `deleted`), original identifiers archived in `deleted_accounts` (staff), username held 30 days (checked by register and username change), sessions/devices/2FA/passkeys/tokens removed, friendships, blocks, ignores, notes, server and group-DM memberships removed (group DM ownership passes on), open platform invites revoked, credits forfeited by a negative ledger row (`account_deleted`), bot tokens of owned apps revoked, Discord link kept (D-030). Refused with field error `guilds` / `owns_guilds` while the account owns a server. Friends get `RELATIONSHIP_REMOVE`, servers `GUILD_MEMBER_REMOVE`, own tabs `SR_ACCOUNT_DELETED`. |
| LNG-USET-020 | `POST /users/@me/data-export` (202), `GET /users/@me/data-export` (`state` none/pending/building/ready/failed/expired, `download_url`), `GET /users/@me/data-export/{id}/download` (zip, owner session, 7 days). Job `user.data_export` builds a zip of JSON files (account, settings, relationships, servers, connections, invites, invoices, activity, security, attachments index, `messages/<channel>.json`, README); no secrets. One open request, one per 24 h (a failed build does not count). Hourly sweep deletes lapsed packages. |
| LNG-USET-024 | See "Connections API". Providers: GitHub, Discord, Spotify, Twitch, YouTube, Reddit (OAuth2, PKCE where supported), Steam (OpenID, verified with Steam). Others are link-only. Hourly job refreshes GitHub/Reddit stats. |
| LNG-PRF-001 | `accounts.avatar_decoration`, `profile_effect`, `theme_primary/accent`; `PATCH /users/@me` takes `avatar_decoration`, `profile_effect` (slugs) and `theme_colors: [primary, accent]` (null clears). Per-server overrides already existed on `guild_members` (nick, avatar, banner, bio). |
| LNG-PRF-002 | `GET /users/{id}/profile` adds `badges`, `member_since`, `guild_member_profile` (with `?guild_id=`, only for a shared server); `user_profile` adds the three new fields; `connected_accounts` adds `url`, `show_activity`, `stats`, `visible`. Mutuals and `friends_since` already existed. |
| LNG-PRF-014/015 | Fan-out already existed. Now per-session: op 3 / IDENTIFY presence is stored per session; `afk: true` (client sends after 10 min) makes that session idle; the user shows idle only when every attached session is idle; dnd/invisible from any session or REST apply to all. `PRESENCE_UPDATE` carries `client_status {desktop,web,mobile}` (platform from IDENTIFY `properties`; empty when invisible). A closing session announces the changed merge. |
| LNG-PRF-013 (BE half) | Activities are rebuilt from a whitelist, max 6, types 0-5 as Discord plus 6 = Reading (SIDE ROOM). Custom status stays first. FE half left to the shell agent, so the box stays unticked. |

## Connections API (for the settings FE)
- `GET /users/@me/connections` -> `{connections: [{type,id,name,verified,visible,show_activity,url?,stats}], providers: [{id,name,verified,available,requires_url}]}`. Show a provider button only when `available`.
- `POST /users/@me/connections/{provider}/authorize` -> `{url}`; navigate the browser there. The callback (`GET /api/v1/connections/{provider}/callback`) redirects to `/settings/user/connections?connected=<provider>` or `?error=<code>&provider=<p>`; codes: `denied state exchange profile taken limit signed_out failed`.
- `PUT /users/@me/connections/link {provider, name?, url?}` unverified link (`website` needs https `url`). `PATCH /users/@me/connections/{provider}/{id} {visible?, show_activity?, name?}` (name only for unverified). `DELETE` same path. `POST .../{id}/refresh` (GitHub and Reddit only, once a minute).
- Operator config `[connections.github|discord|spotify|twitch|youtube|reddit|steam]` with `client_id`, `client_secret` (also `SR_CONNECTIONS__GITHUB__CLIENT_SECRET[_FILE]`), optional `enabled`. Redirect URI to register: `<public_url>/api/v1/connections/<provider>/callback`. Discord falls back to the `[discord]` app (its redirect URI must be added). Steam needs no secret (an optional Web API key in `client_secret` adds the persona name).
- All provider calls use the new `sr_net::SafeClient::call` (pinned, validated, no redirects, capped). No token is stored.

## Not done / reasons
- LNG-USET-006/009/013/017-019/023 etc. are FE items (not mine). LNG-USET-009 still needs a catalogue of decorations/effects (server only checks slugs).
- Per-server pronouns: not added (would thread through the chat member model).
- Xbox, TikTok, Roblox have no usable public OAuth here: link-only.
- Stats refresh for providers other than GitHub/Reddit would need stored tokens (declined for privacy).
- Activity privacy toggles (`share detected activities`) live in the client-side prefs blob; the gateway does not read them yet.
- No deletion/export notice emails.
- Flow state (OAuth `state`) is in memory, single node, like the Discord sign-in.

## Tests run
cargo fmt, clippy `-D warnings` (sr-net, sr-core, sr-chat, sr-db, sr-gateway, sr-api, sr-server, all targets). Unit/route tests: sr-api 465, sr-chat, sr-gateway 140, sr-net 17, sr-core, sr-auth, route_matrix. Postgres (`--include-ignored --test-threads=1`): sr-db account_deletion, profile_be, data_export, social_repo, registration; sr-server `userset` (export job end to end, failure after last attempt). Web not touched (api-client regenerated).

## Decisions / owner questions
- Early Member/Contributor/Bug Hunter/Honorary badges come from `accounts.flags` bits 8-11, App Developer bit 1, Patron from live grants (hidden by the donor's `hide_badge`).
- Owner question: should deletion also be blocked for accounts holding the Owner/staff role? Today it is blocked only while the account owns a server (the staff server counts).
- Owner question: reuse the `[discord]` OAuth app for linking (needs a second redirect URI registered) or a separate one?
