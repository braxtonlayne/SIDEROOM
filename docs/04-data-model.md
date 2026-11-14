# 04 — Data Model

PostgreSQL is the system of record. This is the **logical** model. Exact DDL lives in `crates/sr-db/migrations/` (forward-only, numbered; the newest, at wave 65, is 2830). Where the built table has another name than the logical one below, an "As built" note or section 14 says so. All IDs are Snowflakes (`BIGINT`) unless noted. Every table has `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`. Mutable tables also have `updated_at`.

Conventions:
- Soft states use enums, not deletion (accounts, messages).
- "Hold" columns support future moderation (D-126).
- Large or binary data goes to object storage. Postgres stores keys. One exception: a bug report's optional screenshot (at most 2 MiB) is a `bytea` column of `bug_reports`, so it is deleted with its row (section 14, D-650).

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

### `profile_media` (migration 1020, K-17, D-260)
| id | account_id fk | kind (`background`, `gallery`) | object_key (`avatars/<user>/<hash>`) | content_type | width, height | bytes | created_at |

At most one `background` and 12 `gallery` rows per account (checked in `sr-packages`). A profile document names a row by id; saving a document drops the rows it no longer names, and account deletion removes them with their files.

### `external_identities`
| account_id | provider (`discord`) | provider_user_id (unique per provider) | linked_at | raw_profile jsonb |

Unique (provider, provider_user_id) applies **across all accounts, including deleted ones**, among the live rows. That is how a returning user is matched to their Person (D-030). Migration 1021 (K-20, D-262) adds `unlinked_at` and `moved_to_account_id`: when a staff member moves an identity to another account of the Person, the old row stays as history (unlinked, naming the account it moved to) and a new live row is written, so the uniqueness is a partial index over rows with `unlinked_at IS NULL`.

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
Append-only. `(account_id, delta, reason enum{initial_grant, redemption, purchase, staff_grant, staff_revoke, account_deleted}, ref_id)`. Balance = SUM(delta). Initial grant = +5 (configurable).

### `invite_graph` (view)
`platform_invites` joined to accounts gives edges `issuer → redeemer`. A materialized view is used for staff graph rendering.

## 2. Guilds ("servers") and channels

### `guilds`
id, name, icon_key, banner_key, splash_key, owner_id, description, `features text[]` (e.g. `COMMUNITY`, `DISCOVERABLE`, `STAFF`), verification_level, default_message_notifications, explicit_content_filter, mfa_level, system_channel_id, rules_channel_id, public_updates_channel_id, afk_channel_id, afk_timeout, preferred_locale, vanity_code (citext, unique, 3 to 32 letters, digits or hyphens), `vanity_uses` (joins through the code), nsfw_level, `is_staff_guild bool` (exactly one row true, enforced by a partial unique index), boost_level (derived), `traits jsonb` (a JSON array of at most five `{emoji_id?, emoji_name?, label}` entries, migration 0962; the label rules are in `sr-chat` `guild_traits.rs`), settings jsonb.

### `guild_members`
(guild_id, user_id) pk, nick, avatar_key (per-server), banner_key, bio, joined_at, premium_since, communication_disabled_until, flags, pending (membership screening, set at join when the Community has `screening_enabled`; pending members are read-only), `temporary bool` (joined through a temporary invite; the row is dropped when the member goes offline holding no role). The server tag lives in its own tables (migration 0631, LNG-SSET-011/012, LNG-GLD-011): `guild_tags` (guild_id pk, tag 2 to 4 letters or digits, badge smallint 0 to 15, color 24-bit RGB, members_can_display, updated_at) and `user_primary_guilds` (user_id pk, guild_id, set_at), the one tag a person shows; it is shown only while the tag exists, `members_can_display` is on and the person is still a member. (Phase 10 integration kept this one implementation; 0620 no longer adds `guilds.tag/tag_badge` or `accounts.primary_guild_*`.)

Roles are a join table, `member_roles (guild_id, user_id, role_id)`, not an array column. Foreign keys cascade on role or member delete. The API still exposes `roles: [role ids]` on the member object.

### `guild_directory`
One row per server that asked to be listed (migration 0620, D-012): `guild_id` pk, `status` (`pending`, `approved`, `rejected`, `removed`), `category` (slug), `description` (blurb, 300), `featured`, `requested_by`, `requested_at`, `decided_by`, `decided_at`, `decision_note`. `guild_community` also holds the membership screening form: `screening_enabled`, `screening_description`, `screening_rules jsonb` (at most 16 strings). It also holds the Server Protection settings (migration 0795, enforced by D-224): `dm_spam_filter`, `explicit_media_filter_dms`, `hide_suspicious_links` (booleans, default off) and `warn_new_account_days` (0 to 365, default 0).

### `roles`
id, guild_id, name, color, secondary_color, tertiary_color (migration 0963; the gradient stops, a tertiary stop needs a secondary one, enforced by a check), icon_key, unicode_emoji, hoist, position, permissions (bigint, Discord bit layout), mentionable, managed, tags jsonb.

### `channels`
id, guild_id null (null = DM/group DM), type (Discord enum: 0 text, 1 DM, 2 voice, 3 group DM, 4 category, 5 announcement, 10/11/12 threads, 13 stage, 15 forum, 16 media), name, topic, position, parent_id, nsfw, rate_limit_per_user, bitrate, user_limit, rtc_region, video_quality_mode, default_auto_archive_duration, default_thread_rate_limit, available_tags jsonb, default_reaction_emoji, default_sort_order, default_forum_layout, last_message_id, owner_id (group DM/thread), icon_key (group DM), flags.

### `permission_overwrites`
(channel_id, target_id) pk, target_type (role/member), allow bigint, deny bigint.

### `thread_metadata`, `thread_members`
As in Discord.

### `channel_recipients`
For DMs and group DMs. Per-recipient columns:
- `request_state` (`accepted`, `pending`, `ignored`, `spam`; default `accepted`). A non-friend's first DM lands as `pending` on the recipient's row when their message-requests setting allows it (migration 0072).
- `request_reason` (null, `server_filter` or `new_account`; migration 0825, D-224): set with `request_state = pending` when a shared server's Protection settings, not the recipient's own, sent the conversation to Message Requests.
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

`emojis` (guild_id, name, key, animated, roles allowed, available), `stickers`, `sticker_packs`, `soundboard_sounds`, `guild_templates`, `guild_scheduled_events` (with `image_key`, `entity_id` and `empty_since`, migration 0820), `guild_scheduled_event_users`, `stage_instances`, `automod_rules`, `onboarding` (prompts, options, default channels), `welcome_screens`, `webhooks`, `invites` (built as `guild_invites`, migration 0030; server invites: code, guild_id, channel_id, inviter_id, max_age, max_uses, uses, temporary, single_use_external default per D-022).

## 6. Applications and developer platform

`developer_applications` (applicant account, status: pending / approved / rejected / suspended, reviewer, notes), `applications` (id, owner_id, team_id, name, icon, description, bot_user_id, `release_state enum{development, in_review, released, suspended}`, flags, interactions_endpoint_url, redirect_uris), `application_versions` (manifest jsonb, reviewed_by, review_state), `application_commands`, `application_installs` (guild or user scope), `bot_tokens` (hashed), `oauth2_grants`.

(As built in p15, migration 0970: `packages` has a globally unique `slug`, versions carry a semver and the zip in object storage, and `package_enablements` has one row per package and scope plus a theme `section`; activities are not packages. Customised profiles: `accounts.profile_doc` and `profile_guestbook`, migration 0971.) `extensions` / `themes` / `profile_layouts` / `activities` share `packages` (id, kind, author_id, name, latest_version_id) + `package_versions` (manifest jsonb, artifact_key, checksum, review_state, reviewed_by, reviewed_at) + `package_enablements` (scope: guild/user, scope_id, version_id, config jsonb).

## 7. Staff

`staff_appointments` (account_id, appointed_by, appointed_at, revoked_at, revoked_by, reason, revoke_reason, revoked_reason `left`|`kicked`|`revoked`, migration 0790). Staff status = active appointment **AND** current member of the staff guild (D-130/D-132). `appointment_delegations` (who may appoint). `staff_access_log` (append-only and hash-chained, migration 0130: seq (contiguous from 1), at_us, actor_id, kind `admin_action`|`read`|`private_read`|`denied`, action, target_kind, target_id, reason (required for `private_read`), ip, detail, prev_hash, hash). `audit_log_entries` (guild-scoped, Discord-compatible) + `platform_audit_log` (platform-scoped).

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

As built (migrations 0540 and 0541, `sr-payments`): `payment_wallets` (watch-only `descriptor`, refused if it looks like a private key; `network` mainnet/testnet/signet/regtest; `fingerprint`; `next_index`, which only ever increases; `gap_limit`; one active row), `payment_config` (one JSON document of operator settings: prices, thresholds, perks, confirmation depths), `payment_invoices` (`purpose` is `donate`, `patron`, `server_level` or `invite_credit`; USD cents, sats and the rate are all stored), `payment_txs`, `payment_credits`, `payment_audit`, `payment_watcher` (the poller's cursor). Entitlements are derived, not stored as one table: `entitlement_grants` are the source of truth, and Patron validity and a server's pool are computed from the live (not revoked) grants, so a reorg or a staff revocation is just another row. `guild_support_levels` holds a server's level and `user_patron_notices` the member's Patron notices. The logical names `wallets`, `invoices`, `entitlements` and `server_donation_pool` above are the concepts, not table names.

## 13. Indexing notes

- Migration-level additions beyond the tables above (the many later ones are listed in section 14): `login_events` and `accounts.age_confirmed` (section 1), the `jobs` queue (migration 0004; recurring singleton jobs use ids below 2^40, `SINGLETON_ID_LIMIT` in `sr-db::jobs`; the sequence `jobs_sql_id_seq` of migration 2560 starts at that limit for jobs enqueued from SQL, D-597), and `feature_enablements` with channel scope and nullable `enabled` for inherit (migration 0050).
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

## 14. Tables added after the first design

The sections above are the original logical model plus "as built" notes up to about migration 1020. These tables came later. Each line names the area, the migration that created them and the spec or decision to read; the DDL is the authority.

| Area | Tables | Migration | Read |
|---|---|---|---|
| Platform settings | `platform_settings`, `feature_state`, `entrance_copy` | 0003, 0680, 0843 | doc 11, doc 09 |
| Trusted devices and account hygiene | `trusted_devices`, `account_known_devices`, `account_delete_requests`, `deleted_accounts`, `data_exports`, `age_gate_resets` | 0043, 0880, 0131, 0660, 1140 | doc 09 |
| Staff and abuse signals | `staff_role_templates`, `staff_appointment_offers`, `staff_alerts`, `platform_announcements`, `person_bans`, `suspicion_events`, `evasion_signals`, `csp_report_counters` | 0130, 0131, 0650, 1150, 1230 | doc 07 |
| Servers (built names) | `guild_bans`, `guild_invites`, `guild_onboarding`, `guild_welcome_screens`, `guild_insights_daily`, `guild_insights_channel_daily`, `guild_guides`, `guild_application_forms`, `guild_applications`, `guild_appearance`, `guild_pack_items`, `guild_sticker_settings`, `group_dm_invites`, `dm_channels` | 0010, 0011, 0030, 0036, 0632, 0633, 0634, 0796, 1060, 1080 | doc 06, lounge-chat |
| Notifications and push | `notification_settings`, `channel_notification_overrides`, `user_connections`, `user_presence_prefs`, `push_subscriptions`, `push_vapid_key` | 0022, 0023, 0670 | LNG-NTF |
| Feature modules | `scheduled_messages`, `message_bookmarks`, `user_birthdays`, `birthday_announcements`, `member_xp`, `rss_feeds`, `poll_templates`, `suggestions`, `tickets`, `feature_cursors`, `message_self_deletes`, `automod_alerts` | 0760, 0966, 1102 | inventory 09 |
| Cinema social, media, downloads | `cinema_progress`, `cinema_watching`, `cinema_lists`, `cinema_ratings`, `cinema_reviews`, `cinema_comments`, `cinema_reports`, `video_renditions`, `video_audio_tracks`, `video_subtitle_tracks`, `worker_tokens`, `transcode_workers`, `transcode_uploads`, `cinema_downloads`, `cinema_media_gc`, `cinema_submission_uploads` | 0400 to 0403, 0420 to 0423, 1240 | cinema-media |
| Live TV | `tv_channels`, `tv_blocks`, `tv_reminders`, `tv_schedule_slots`, `tv_guide_imports`, `tv_favorites` | 0460, 0461, 0780, 1000 | cinema-live-tv |
| Watch parties | `watch_parties`, `watch_party_members`, `watch_party_bans`, `watch_party_queue`, `watch_party_audit`, `watch_party_cards`; party media kinds `cinema`, `embed`, `direct` | 0480, 1001, 1700 | watch-parties |
| Library social and ingest | `reading_ratings`, `reading_reviews`, `reading_lists`, `reading_comments`, `reading_downloads`, `reading_presence`, `reading_upload_multipart`, `library_cover_hashes`, `library_export_cache`, `ingest_sources`, `ingest_files`, `review_queue_cards`, `submission_upload_bytes` | 0690, 0700, 0810, 0830 to 0832, 0879, 0890 to 0892 | library-reader |
| Developer platform | `developer_requests`, `application_audit`, `application_command_permissions`, `oauth2_codes`, `oauth2_tokens`, `interactions`, `interaction_messages`, `application_emojis`, `application_assets`, `app_testers`, `message_webhook_authors` | 0500 to 0505, 0770, 0811, 0850, 0851 | developer-platform |
| Activities | `application_activities`, `activity_instances` (with `host_id`, D-388), `activity_participants`, `application_activity_bundles` (with capabilities), `activity_proxy_tokens`, `activity_guest_links`, `activity_guest_sessions` | 0930, 0965, 1760, 2070, 2080, 2200 | doc 11 section 3, D-408 to D-419 |
| Arcade | `arcade_featured`, `arcade_coming_soon`, `arcade_coming_soon_notify`, `arcade_guild_cartridges`, `arcade_signing_keys`, `arcade_code_signatures`, `arcade_build_vouches`; `code_sha256` on bundles | 2040, 2050, 2051, 2071 | D-406 to D-411 |
| Payments | see section 12 | 0540, 0541 | donations |
| Storage mounts and shelves (BYOS) | `storage_mounts`, `storage_mount_shares`, `guild_storage_grants`, `storage_mount_uploads`, `storage_mount_part_notes`, `storage_mount_attachments`, `storage_objects`, `storage_index_runs`, `storage_object_transcodes`, `storage_figures`, `attachment_multipart`; wave 41: `shelf_objects`, `shelf_grants`, `shelf_delist_strikes` | 1350, 1400, 1410, 1440, 1441, 1450, 1501, 1030, 2020 | D-393, D-394, D-401, D-402 |
| Music | `music_tracks`, `music_playlists`, `music_playlist_items`, `music_favorites`, `music_recent`, `jukebox_sessions`, `jukebox_queue`, `jukebox_votes`, `jukebox_listeners`, `listening_rooms` | 1450, 1550, 1570, 1580 | MUS-* |
| Utility drawer | `fx_rates`, `fx_fetch_state`, `scratch_notes` (ciphertext only), `loot_tables`, `tool_rooms`, `invite_cards`, `invite_card_claims` | 1620, 1630, 1660, 1670, 2320 | UTL-* |
| Profiles | `profile_privacy`, `scrapbook_targets`, `scrapbook_entries`, `scrapbook_objects`, `scrapbook_removals`, `status_rings`, `status_ring_guilds`, `status_ring_views`, `status_ring_posts`, `profile_vouches` | 1680, 1690, 1710, 2130 | PRF-*, D-382, D-425 |
| Discord bridge | `bridges`, `bridge_channel_maps`, `bridge_message_map`, `bridge_outbox`, `bridge_members`, `bridge_reactions`, `bridge_role_maps`, `bridge_sync_ops`, `bridge_sync_plans`, `bridge_conflicts` and others | 1720 to 1754 | BRG-*, D-386, D-391 |
| Invite controls | `invite_inactive_roots`, `invite_prune_runs` | 1890 | OPS-INV-001 |
| Moderation and compliance | `moderation_cases`, `moderation_reports`, `moderation_case_evidence`, `moderation_case_notes`, `moderation_case_events`, `moderation_appeals`, `takedown_notices`, `takedown_items`, `takedown_events`, `takedown_strike_reviews`, `blocked_objects`, `data_requests`; wave 44-A: `account_activity`, `moderation_subtree_cases`, `moderation_subtree_nodes`, `member_restrictions`, `moderation_requests`, `moderation_notices` | 1090, 1100, 1120, 2120 | moderation-compliance, D-395, D-420 to D-424 |
| Operations | `crash_reports` (deduplicated by fingerprint) | 2160 | OPS-CRASH-001 |
| Beta bug reports | `bug_reports` | 2790 | D-647 to D-650 |
| Activity package uploads | `activity_package_uploads` | 2830 | D-654 to D-657 |

### 14.1 Later changes to tables above

Waves 47 to 65 added two tables (`bug_reports`, `activity_package_uploads`, in the table above) and changed these existing ones. The DDL is the authority.

| Change | Migration | Read |
|---|---|---|
| `invite_cards.claimants_purged_at` and `going_final`: the list of who claimed a seat is deleted after the event (`invites.card_claimants_retention_days`, job `cards.purge_claimants`); the number that went is kept | 2320 | FU-8 |
| `moderation_cases.target_kind` and `moderation_reports.target_kind` accept `guestbook_note` | 2340 | D-506, D-507 |
| `watch_parties.media_page_url` and `media_format` (`file` or `hls`, only for `direct` media): parties started from a PeerTube link | 2360 | D-515, D-516 |
| `soundboard_sounds.format_type` 1 to 4 (4 = WebM/Opus) | 2380 | D-534 |
| Triggers `messages_held_delete` and `attachments_held_delete`: a message or attachment under a moderation hold cannot be deleted, directly or by cascade (`restrict_violation`); release the hold first | 2390 | D-122, D-212, D-213 |
| `arcade_guild_cartridges.manifest_checked_at`, `delisted_at`, `delist_reason`, then `manifest_defer_*` (a shelf that cannot be asked defers the re-check instead of delisting) | 2440, 2500 | D-568, D-579, D-582 |
| `storage_objects.content_tag`, `content_sha256`; `takedown_items.object_hash`, `object_size`, `object_sha256`, `derived_from`, `notified_at`: a storage-object takedown follows the bytes to other keys and mounts | 2450, 2510 | D-571, D-583, D-584 |
| `cinema_comments` and `reading_comments`: `deleted_at`, nullable `user_id`, no cascade on `parent_id`; a deleted comment with replies stays as a tombstone | 2451 | D-572 |
| Sequence `jobs_sql_id_seq`: the one id source for jobs enqueued from SQL, starting at 2^40 | 2560 | D-592, D-597 |
| `moderation_requests.state` accepts `executing`; statement triggers refuse `TRUNCATE` on every append-only audit table and `DELETE` on `application_audit` and `payment_audit` | 2590 | D-606 |
| Index `bridge_channel_maps_discord_channel_idx` | 2610 | D-609 |
| `application_activity_bundles.superseded_at` (set by trigger when a bundle becomes `superseded`), for the grace period before its files are removed | 2670 | D-630 |

`bug_reports` (D-647 to D-650) is personal data: `reporter_id` (cascade on account delete), `status` (`open` or `closed`), `closed_at`, `closed_by`, `commit_sha`, `route` (a route name, not a URL), `description` (1 to 2000 characters), `bundle` jsonb (an allow-listed debug bundle the server rebuilt; its gateway block carries the status, the reconnects since page load, at most 10,000, and the last heartbeat round trip in milliseconds, at most 600,000, both kept by `apps/web/src/gateway/client.ts`) and an optional `screenshot` bytea of at most 2 MiB. The screenshot is deleted when the report is closed or after 30 days and the row after 90 days (job `bugs.retention`, daily); account deletion removes the member's reports (D-617). Opening a report, its picture, and closing one each write a platform audit entry. The reporter itself is the platform switch `platform.beta_reports`, a platform feature switch (off by default) kept with the other switches, not in a table of its own.

`activity_package_uploads` (D-654) is one row per upload of an Activity package in parts: `application_id`, `created_by`, `expires_at`, `size`, `part_size`, `part_count`, `object_key` (`activities/{application}/uploads/{id}.zip`), `multipart_id`, `state` (`uploading`, `checking`, `ready`, `refused`, `expired`, `aborted`), `checked_bytes` (progress of the check), `bundle_id` (the bundle the check made, cleared if that bundle is deleted) and `error`. The daily sweep (`activities.package_sweep`) aborts unfinished rows past `expires_at` and deletes every row after seven days. The size tiers of D-655 are operator settings (`activities.package.max_mb`, `reviewed_max_mb`, `platform_max_mb`, `max_files`), not columns.

Account deletion (D-617) adds no table. `crates/sr-db/src/account_deletion.rs` runs in one transaction: the account row stays as the tombstone and `accounts.state` becomes `deleted`; what the member contributed for others (messages, reactions, poll votes, Cinema and Library reviews, ratings and comments, public lists) stays under "Deleted User"; personal state that served only the member is deleted (Cinema progress, reading progress and bookmarks, scratch notes, personal loot tables, saved, private and friends-only Cinema lists, unstarted invite card claims, bug reports); ready data exports expire and unbuilt ones fail.

The storage response-type rules (D-629, D-637, D-641 to D-645) add no column. The `response_type` row of a mount's connection test lives in `storage_mounts.last_test` (jsonb), and a mount whose stored test failed it hands out no presigned GET.
