# 04 — Data Model

PostgreSQL is the system of record. This is the **logical** model. Exact DDL lives in `crates/sr-db/migrations/`. All IDs are Snowflakes (`BIGINT`) unless noted. Every table has `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`. Mutable tables also have `updated_at`.

Conventions:
- Soft states use enums, not deletion (accounts, messages).
- "Hold" columns support future moderation (D-126).
- Large or binary data goes to object storage. Postgres stores keys.

## 1. Identity

### `persons`
The staff-side record of a real human. It may span several accounts (D-030).
| col | type | notes |
|---|---|---|
| id | bigint pk | |
| notes | text | staff notes |
| suspicion_level | smallint | 0 = none. Raised by the invite-graph rules (D-031) |
| created_at | timestamptz | |

### `accounts` (users)
| col | type | notes |
|---|---|---|
| id | bigint pk | user id (exposed as `user.id`) |
| person_id | bigint fk persons | always set |
| username | citext unique | Discord-style unique handle, 2–32 chars `[a-z0-9_.]` |
| display_name | text null | global display name |
| email | citext unique | required (D-027) |
| email_verified_at | timestamptz null | |
| password_hash | text | argon2id PHC string |
| state | enum `active`, `deactivated`, `deleted`, `platform_banned` | Only `active` counts toward the 20k cap (D-013) |
| state_changed_at | timestamptz | |
| age_confirmed_at | timestamptz null | D-032. Gates opening age-restricted channels and Library pages (D-184) |
| locale, timezone | text | |
| flags | bigint | public badges (staff, early member, donor, contributor…) |
| invited_by_invite_id | bigint fk platform_invites null | null only for the bootstrap Owner |
| avatar_key, banner_key, accent_color, bio, pronouns | | profile basics |
| profile_doc | jsonb | customization document (doc 10) |

### `external_identities`
| account_id | provider (`discord`) | provider_user_id (unique per provider) | linked_at | raw_profile jsonb |

Unique (provider, provider_user_id) applies **across all accounts, including deleted ones**. That is how a returning user is matched to their Person (D-030).

### `sessions`, `mfa_totp`, `webauthn_credentials`, `recovery_codes`, `email_tokens`
Standard. Tokens are stored as SHA-256 hashes. Migration 0180 renames `webauthn_credentials` to `passkey_credentials` (`credential_id` unique, `cose_key`, `alg`, `sign_count`, `backup_eligible`, `backup_state`, `aaguid`, `name` 1 to 64 characters, `last_used_at`) and adds `accounts.user_handle` (32 random bytes, the WebAuthn `user.id`, never the account id).

### `login_events` (migration 0001)
Login IP history, kept 180 days (doc 09 §1). Visible to staff with `MANAGE_ACCOUNTS`.

### `accounts.age_confirmed` (migration 0001)
Nullable boolean beside `age_confirmed_at`. `true`/`false` records the age-gate answer (doc 06 §7); `age_confirmed_at` records the yes.

### `platform_invites`
| col | type | notes |
|---|---|---|
| id | bigint pk | |
| code | text unique | 10-char base32 (Crockford), case-insensitive |
| issuer_account_id | bigint fk | |
| state | enum `open`, `revoked`, `redeemed` | |
| redeemed_by_account_id | bigint null | |
| redeemed_at, revoked_at | timestamptz null | |
| note | text | issuer's private label |

### `invite_credits_ledger`
Append-only. `(account_id, delta, reason enum{initial_grant, redemption, purchase, staff_grant, staff_revoke}, ref_id)`. Balance = SUM(delta). Initial grant = +5 (configurable).

### `invite_graph` (view)
`platform_invites` joined to accounts gives edges `issuer → redeemer`. A materialized view is used for staff graph rendering.

## 2. Guilds ("servers") and channels

### `guilds`
id, name, icon_key, banner_key, splash_key, owner_id, description, `features text[]` (e.g. `COMMUNITY`, `DISCOVERABLE`, `STAFF`), verification_level, default_message_notifications, explicit_content_filter, mfa_level, system_channel_id, rules_channel_id, public_updates_channel_id, afk_channel_id, afk_timeout, preferred_locale, vanity_code (citext, unique, 3 to 32 letters, digits or hyphens), `vanity_uses` (joins through the code), nsfw_level, `is_staff_guild bool` (exactly one row true, enforced by a partial unique index), boost_level (derived), traits jsonb, settings jsonb.

### `guild_members`
(guild_id, user_id) pk, nick, avatar_key (per-server), banner_key, bio, joined_at, premium_since, communication_disabled_until, flags, pending (membership screening, set at join when the Community has `screening_enabled`; pending members are read-only), `temporary bool` (joined through a temporary invite; the row is dropped when the member goes offline holding no role). The server tag lives in its own tables (migration 0631, LNG-SSET-011/012, LNG-GLD-011): `guild_tags` (guild_id pk, tag 2 to 4 letters or digits, badge smallint 0 to 15, color 24-bit RGB, members_can_display, updated_at) and `user_primary_guilds` (user_id pk, guild_id, set_at), the one tag a person shows; it is shown only while the tag exists, `members_can_display` is on and the person is still a member. (Phase 10 integration kept this one implementation; 0620 no longer adds `guilds.tag/tag_badge` or `accounts.primary_guild_*`.)

Roles are a join table, `member_roles (guild_id, user_id, role_id)`, not an array column. Foreign keys cascade on role or member delete. The API still exposes `roles: [role ids]` on the member object.

### `guild_directory`
One row per server that asked to be listed (migration 0620, D-012): `guild_id` pk, `status` (`pending`, `approved`, `rejected`, `removed`), `category` (slug), `description` (blurb, 300), `featured`, `requested_by`, `requested_at`, `decided_by`, `decided_at`, `decision_note`. `guild_community` also holds the membership screening form: `screening_enabled`, `screening_description`, `screening_rules jsonb` (at most 16 strings).

### `roles`
id, guild_id, name, color, secondary_color, icon_key, unicode_emoji, hoist, position, permissions (bigint, Discord bit layout), mentionable, managed, tags jsonb.

### `channels`
id, guild_id null (null = DM/group DM), type (Discord enum: 0 text, 1 DM, 2 voice, 3 group DM, 4 category, 5 announcement, 10/11/12 threads, 13 stage, 15 forum, 16 media), name, topic, position, parent_id, nsfw, rate_limit_per_user, bitrate, user_limit, rtc_region, video_quality_mode, default_auto_archive_duration, default_thread_rate_limit, available_tags jsonb, default_reaction_emoji, default_sort_order, default_forum_layout, last_message_id, owner_id (group DM/thread), icon_key (group DM), flags.

### `permission_overwrites`
(channel_id, target_id) pk, target_type (role/member), allow bigint, deny bigint.

### `thread_metadata`, `thread_members`
As in Discord.

### `channel_recipients`
For DMs and group DMs. Per-recipient columns:
- `request_state` (`accepted`, `pending`, `ignored`, `spam`; default `accepted`). A non-friend's first DM lands as `pending` on the recipient's row when their message-requests setting allows it (migration 0072).
- `hidden_at` (the recipient closed the DM; a new message reopens it) and `seq` (join order, used for ownership hand-over). Migration 0021.

## 3. Messages

### `messages` (range-partitioned by `id`, i.e. by time, monthly)
| col | type | notes |
|---|---|---|
| id | bigint pk | |
| channel_id | bigint | index (channel_id, id desc) |
| guild_id | bigint null | |
| author_id | bigint | user or webhook/app |
| webhook_id, application_id | bigint null | |
| type | smallint | Discord message types |
| content | text | ≤ 2,000 chars by default (operator-configurable, up to 4,000 for everyone. Long messages don't affect cost, so they aren't a donor perk) |
| content_ast | jsonb null | cached parse (optional) |
| flags | int | Discord message flags |
| edited_at | timestamptz null | |
| reference | jsonb null | reply/forward/crosspost reference |
| embeds, components | jsonb | |
| sticker_ids | bigint[] | |
| poll | jsonb null | poll definition. Votes in `poll_votes` |
| mention_everyone | bool | |
| mention_user_ids, mention_role_ids, mention_channel_ids | bigint[] | from `sr-markdown` |
| pinned | bool | |
| **visibility** | enum `visible`, `user_deleted`, `staff_removed` | D-121 |
| deleted_at | timestamptz null | set when user deletes |
| purge_after | timestamptz null | `deleted_at + 30 days`, null while on hold |
| **hold_id** | bigint null fk moderation_holds | D-122. Blocks purge |

`message_edits` (message_id, edited_at, previous_content, previous_embeds) keeps edit history, visible to high-level staff only. It is purged along with the message.

### `attachments`
id, message_id, uploader_id, filename, content_type, size_bytes, width, height, duration_secs, waveform (voice messages), storage_key, `is_large bool` (size > standard 30 MB), `expires_at` (large only: uploaded_at + 7 days, D-043), `state enum{pending, ready, expired, purged}`, flags (spoiler), blurhash.

### `reactions`
(message_id, emoji_key, user_id) pk, burst bool, created_at. Counts are cached on the message in jsonb.

### `pins` (or the `pinned` flag + pinned_at), `poll_votes`, `read_states` (user_id, channel_id, last_read_message_id, mention_count, last_pin_timestamp).

## 4. Social

`relationships` (user_id, target_id, type: friend / blocked / pending_incoming / pending_outgoing, nickname). Codes follow Discord (1 to 4). Nicknames exist only on `friend` rows, up to 32 characters (migration 0020).

`user_ignores` (user_id, target_id, created_at; primary key on the pair). Ignore is its own table, not a relationship type, so ignoring a friend keeps the friendship. Ignoring hides messages, activity and DM notifications. Relationship type 6 ("ignored") is retired and its rows moved here (migration 0070).

`user_notes` (user_id, target_id, note up to 256 characters, created_at, updated_at; primary key on the pair). Private notes on any user (migration 0071).

`user_privacy` (user_id primary key; booleans `fr_everyone`, `fr_friends_of_friends`, `fr_server_members`, `dm_server_members`, `dm_message_requests`). The server enforces these rules, so they keep typed columns (migration 0020). When the Discord-shaped `user_settings` blob lands, these fields are projected into it on read and written back to this table.

`user_settings` (jsonb blob, versioned like Discord's proto settings; a client-owned merge-patch document, D-179). Keys the web client uses today: `guild_positions` (ordered guild ids for the server rail) and `guild_folders` (`[{id, name, color, guild_ids}]`), both Discord's names; plus `collapsed_categories`, `last_channel_by_guild` (the remembered channel per server) and `panel_widths` for UI state that should follow the member across devices. The client writes a debounced merge patch after a change, applies `SR_USER_SETTINGS_UPDATE` from other sessions, and keeps `localStorage` only as an offline cache and first-paint source. The server never interprets these keys. The Lounge also stores `bookmarks` and `reminders` here, each an object keyed by message id (`{message_id, channel_id, guild_id, preview, author, saved_at}`, plus `due_at` for a reminder), so a merge patch adds or removes one entry; reminders fire in an open client only, there is no push delivery yet. User Settings preferences live under `user_prefs`: one object with camelCase fields (`theme`, `sectionThemes`, `messageDisplay`, `messageGap`, `fontScale`, `zoom`, `saturation`, `allowServerTheming`, `reducedMotion`, `highContrast`, `underlineLinks`, `roleColors`, `dyslexiaFont`, `stickerAnimation`, `emojiAnimation`, `showSendButton`, `linkPreviews`, `showEmbeds`, `inlineAttachments`, `showReactions`, `convertEmoticons`, `spoilers`, the notification switches, the activity-privacy switches, `locale`, `timeFormat`, `keybinds`, `developerMode`). Every field is validated by the client on the way in, so unknown or stale values fall back to the default; `sectionThemes` and `keybinds` are records where a removed entry is sent as `null` (merge patch). The account copy wins when it loads; a browser with local choices and an account with none pushes them up once. `load_remote_images` stays a separate top-level key. `guild_member_settings` (notification overrides), `presences` (memory only).

## 5. Expression

`emojis` (guild_id, name, key, animated, roles allowed, available), `stickers`, `sticker_packs`, `soundboard_sounds`, `guild_templates`, `guild_scheduled_events` (with `image_key`, `entity_id` and `empty_since`, migration 0820), `guild_scheduled_event_users`, `stage_instances`, `automod_rules`, `onboarding` (prompts, options, default channels), `welcome_screens`, `webhooks`, `invites` (server invites: code, guild_id, channel_id, inviter_id, max_age, max_uses, uses, temporary, single_use_external default per D-022).

## 6. Applications and developer platform

`developer_applications` (applicant account, status: pending / approved / rejected / suspended, reviewer, notes), `applications` (id, owner_id, team_id, name, icon, description, bot_user_id, `release_state enum{development, in_review, released, suspended}`, flags, interactions_endpoint_url, redirect_uris), `application_versions` (manifest jsonb, reviewed_by, review_state), `application_commands`, `application_installs` (guild or user scope), `bot_tokens` (hashed), `oauth2_grants`.

`extensions` / `themes` / `profile_layouts` / `activities` share `packages` (id, kind, author_id, name, latest_version_id) + `package_versions` (manifest jsonb, artifact_key, checksum, review_state, reviewed_by, reviewed_at) + `package_enablements` (scope: guild/user, scope_id, version_id, config jsonb).

## 7. Staff

`staff_appointments` (account_id, appointed_by, appointed_at, revoked_at, revoked_by, reason, revoked_reason `left`|`kicked`|`revoked`, migration 0790). Staff status = active appointment **AND** current member of the staff guild (D-130/D-132). `appointment_delegations` (who may appoint). `staff_access_log` (append-only: id, staff_account_id, action, target_type, target_id, reason, created_at, prev_hash, hash). `audit_log_entries` (guild-scoped, Discord-compatible) + `platform_audit_log` (platform-scoped).

`moderation_holds` (id, reason, created_by, case_ref). Phase 7 expands these into cases and reports.

## 8. Libraries (shared)

### Review pipeline (`sr-review`)
**As built (migration 0290):** `submissions` (kind `library.series` / `library.items`, submitter_id, state: draft / submitted / in_review / changes_requested / approved / rejected / published / withdrawn, title, summary, ref_id, payload, flags, claimed_by + claim_expires_at (reviewer lease), decided_by, decision_reason, submitted_at / decided_at / published_at), `submission_items` (ref_id, label, position, state pending / approved / rejected / published, note; unique per submission and ref), `submission_events` (kind, actor_id, body, internal, data jsonb), `content_requests` + `content_request_votes`. The text below is the original plan; `contribution_credits` is still to do.

`submissions` (id, kind: video_title / video_items / reading_series / reading_items / app / theme / extension / profile_layout, submitter_id, state: draft / processing / pending_review / changes_requested / approved / rejected / withdrawn, payload jsonb, reviewer_id, decision_note, decided_at), `submission_items` (per file/episode/chapter, so reviewers can approve individual items within a batch), `submission_events` (timeline/comments).

### Contribution counters
`contribution_credits` (account_id, kind: movie / episode / chapter, item_id, submission_id, granted_at, revoked_at). Unique on (kind, item_id), so replacements and duplicates don't double count (D-074).

### Social on content
`ratings` (user_id, target_kind, target_id, score 1–10, created_at), `reviews` (text + rating link + spoiler flag), `comments` (threaded, on title / episode / chapter), `lists` + `list_items` (user lists, shareable), `follows` (series follow → update notifications), `activity_events` (watched/read events, honouring privacy, D-120).

"People you know" (D-089) = `SELECT DISTINCT user_id FROM guild_members WHERE guild_id IN (my guilds)`. This is cached per user in moka and refreshed on membership change.

## 9. Cinema

`video_titles` (id, kind: movie / series / anime_series / anime_movie, title, original_title, sort_title, year, synopsis, tagline, runtime, content_rating, genres text[], tags text[], `age_restricted bool`, poster_key, backdrop_key, logo_key, external_ids jsonb, published bool, added_by, published_at), `seasons` (title_id, number, name, poster_key), `episodes` (season_id, number, absolute_number, title, synopsis, air_date, runtime, still_key), `video_assets` (owner: title for movies / episode, `variant` e.g. "Director's Cut", original_key, original_size, probe jsonb, `state: uploaded / queued / transcoding / ready / failed`), `renditions` (asset_id, height 1080/720/480, codec, bitrate, hls_prefix_key, segment_count, duration_ms, `segment_index` bytea/jsonb with segment durations, used by the TV stitcher), `audio_tracks`, `subtitle_tracks` (lang, label, format → WebVTT key, forced, sdh), `trailers`, `chapters/markers` (intro/credits for "skip intro"), `watch_progress` (user, asset, position_ms, completed, updated_at), `transcode_jobs`.

As built, catalog side (migration 0370, `sr-cinema`): `video_titles` (kind `movie` / `series` / `anime`; an anime title is episodic when it has seasons; `cast_list`, `directors`, `writers`, `studios`, `crew` are free text arrays, D-089; `alt_titles`, `languages`, `subtitle_languages`, `sort_title`; `asset_id` is a plain reference to the playable asset of a film; denormalised `season_count`, `episode_count` and `last_content_at` are refreshed in the transaction that changes the episodes; `merged_into` is the redirect left by a merge), `video_seasons`, `video_episodes` (`asset_id` plain reference, `published`), `video_title_variants` ("Director's Cut"; the title row is the default cut), `video_trailers`, `video_markers` (intro / credits / recap windows per title or episode, one of each kind), `video_collections` (curated rows, hero spotlights and banners placed on `surfaces`) and `video_collection_items`. The asset, rendition, track and transcode tables, `watch_progress`, lists, ratings and reviews are owned by the ingest and social work and refer to titles and episodes by plain id columns without foreign keys. HTTP API: cinema-media spec section 8.

## 10. Live TV

`tv_channels` (id, number, call_sign, name, era_label, logo_key, `kind: imported_guide / rule_based / manual`, timezone, `filler_policy`, `rules jsonb`, sort, enabled), `tv_schedule_slots` (channel_id, starts_at (local time in the channel tz + recurrence or absolute), duration, `ref: episode_id / title_id / series_ref (pick next) / filler`, source_label e.g. "Toonami 2003-09-13"), `tv_guide_imports` (raw import, mapping report). The schedule is resolved to concrete `(start, end, asset)` blocks by `sr-tv`. Missing content is skipped (D-102).

## 11. Library (reading)

`reading_series` (id, kind: manga / manhwa / manhua / webtoon / comic / novel / book, title, alt_titles text[], authors, artists, status, year, synopsis, genres, tags, `reading_direction default rtl/ltr/vertical`, age_restricted, cover_key, published), `reading_editions` (series_id, language, edition label e.g. "Official English", "Scanlation X", is_default), `volumes`, `chapters` (edition_id, volume_id null, number numeric, title, page_count, `format: images / pdf / epub`, release_group, published_at), `pages` (chapter_id, index, key (webp), width, height, is_double_spread bool, blurhash), `reading_progress` (user, chapter, page_index, percent, updated_at), `bookmarks`.

As built (migrations 0200 and 0201): the tables carry the `reading_` prefix, `reading_volumes` / `reading_chapters` / `reading_pages` / `reading_bookmarks` / `reading_follows`, and `reading_pages` is keyed `(chapter_id, idx)` with a `variants` jsonb of sized WebP keys (`{"720": key, ...}`). A chapter is readable only when it and its series are `published`; a merged-away series keeps its row with `merged_into` set. `reading_series` also keeps `chapter_count`, `last_chapter_at` and `follower_count`, refreshed in the transaction that changes them. `reading_follows` holds the shelf (reading / plan_to_read / completed / on_hold / dropped) and the notify flag. Pages, covers and variants live under the object key prefix `library/{series}/{chapter}/` and are served through signed `/cdn/library/...` URLs.

Phase 4 adds migrations 0210 and 0211. `reading_gc_keys` (key, chapter_id, queued_at) is the storage collector queue: the repository queues the objects of replaced pages, replaced covers and sources, and deleted chapters, editions, volumes and series in the same transaction as the change, and the `library.gc` job deletes each one once no page, cover, source or live upload references it. `reading_uploads` (id, chapter_id, series_id, user_id, filename, kind archive / epub / pdf, size_bytes, object_key, status awaiting_upload / queued / running / done / failed / expired, error, warnings, page_count, metadata, timestamps) is one row per uploaded chapter file and the status the client polls. Re-ingested pages live under `library/{series}/{chapter}/r{upload}/NNNN.webp` so a replacement never overwrites objects readers still load. `reading_progress.updated_at` doubles as the version of the cross-device conflict check.

## 12. Donations

`wallets` (operator config: xpub/descriptor, network, current derivation index), `invoices` (id, account_id, purpose: personal_tier / server_contribution / invite_credit, target_id (guild for server), amount_usd_cents, amount_sats, btc_rate, address, derivation_index, state: open / seen / confirmed / expired / underpaid / overpaid, expires_at, confirmations, txids text[]), `entitlements` (account_id or guild_id, kind, level, starts_at, ends_at, grace_ends_at, source_invoice_id), `server_donation_pool` (guild_id, period_month, total_usd_cents), `donor_preferences` (hide_name, hide_amount).

## 13. Indexing notes

- Migration-level additions beyond the tables above: `login_events` and `accounts.age_confirmed` (section 1), the `jobs` queue (migration 0004; recurring singleton jobs use reserved ids below 1024), and `feature_enablements` with channel scope and nullable `enabled` for inherit (migration 0050).
- `messages`: (channel_id, id DESC), partial index `WHERE visibility='visible'`, and (purge_after) WHERE purge_after IS NOT NULL.
- `attachments`: (expires_at) WHERE is_large AND state='ready'.
- `guild_members`: (user_id) for "my guilds".
- `accounts`: partial count index for `state='active'` used by the cap check (run inside the invite-redemption transaction with `SERIALIZABLE` or an advisory lock to prevent cap races).

### 13.1 Hot-path review (XC-PERF-004)

`crates/sr-db/tests/explain_hot_paths.rs` (`just explain-hot-paths`, needs `SR_TEST_DATABASE_URL`) seeds a scratch database (20k users, one 20k-member server plus 199 small ones, 5,000 channels, 400k messages, 200k read states; `SR_EXPLAIN_SCALE` multiplies the volume), runs `EXPLAIN (FORMAT JSON)` on the statements below and fails on any sequential scan of a table estimated above 1,000 rows (except those listed as allowed). Result on PostgreSQL 17: no failures, so no index migration was added (range 0570-0579 is unused).

| Path | Plan |
|---|---|
| Message list (latest, before cursor) | `messages_channel_visible_idx` index scan per partition (merge append, `LIMIT 50`) |
| Pins | bitmap scan on the populated partition; empty partitions are trivial seq scans |
| Channel list of a server | `channels_guild_idx` |
| Member list page | `guild_members` primary key range, `member_roles` primary key |
| My servers | `guild_members_user_idx` |
| Unread: read states, newer-than-read join, mention total | `read_states` primary key prefix, `channels` primary key |
| Member count | index-only scan over the server's rows (allowed; cache the count if it becomes hot) |
| Member search (substring `ILIKE`) | bounded by the server's members through the primary key; a leading wildcard cannot use a btree, accepted |

Findings that are not index gaps:

- Message queries without an `id` bound touch every monthly partition of `messages` (pruning needs an `id` range). Each probe is a cheap index lookup, but the partition count grows by one per month; pass an id lower bound (for example the channel's creation snowflake) when listing the first page if this shows up in profiles.
- Full-text message search runs in Tantivy (`sr-search`), not Postgres, so it has no EXPLAIN here.
