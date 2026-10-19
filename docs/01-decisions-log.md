# 01 — Decisions Log

This is the **authoritative** record of product decisions made by the owner during scoping (October 2026). Each entry has an ID. Specs cite these IDs. If a spec contradicts this log, this log wins. Fix the spec.

Status key: **Decided** = owner's explicit answer. **Default** = engineering default chosen by the spec authors under the owner's delegation ("however you believe best"). The owner may change a Default without breaking anything else.

---

## Identity and naming

| ID | Decision | Status |
|---|---|---|
| D-001 | The platform is named **SIDE ROOM** (stylized `SIDE ROOM.`). Earlier working name "Fable" is retired. | Decided |
| D-002 | Fresh project from scratch. No legacy code. | Decided |
| D-003 | Sections: Entrance, Lounge (chat), Cinema (media), Library (reading), Staff Server/admin. **"The Board" is permanently cut.** | Decided |

## Topology and scale

| ID | Decision | Status |
|---|---|---|
| D-010 | One installation hosts many Discord-style servers ("guilds"). Accounts are global to the installation. | Decided |
| D-011 | Any user can create servers. | Decided |
| D-012 | Servers may enable **Community** mode once they meet configurable requirements (rules channel, verification level, explicit-content filter, etc., like Discord). Appearing in the platform-internal **Server Directory** additionally requires staff approval. The directory is visible only to logged-in members. | Decided |
| D-013 | Capacity ceiling: **20,000 counted accounts**. Counted = active accounts. Deleted, deactivated, and platform-banned accounts do **not** count. | Decided |
| D-014 | When the counted total reaches the ceiling, **creating** and **redeeming** platform invites is blocked. Server invites keep working. | Decided |
| D-015 | Build quality target: correct for 20k accounts / ~2k concurrent users from day one, even though production will be ~100 users. | Decided |

## Entrance, accounts, invites

| ID | Decision | Status |
|---|---|---|
| D-020 | **No invite, no entry.** Platform invites are single-use, one-time codes that permit creating one account. | Decided |
| D-021 | Platform entry and server membership are separate. A server invite link (`/invite/:code`) requires login. If the visitor has no account, the page offers nothing else. They must go to the landing page manually. | Decided |
| D-022 | Server invites are configurable like Discord's (expiry, max uses, temporary membership). Any invite intended to leave the platform defaults to **single use**. | Decided |
| D-023 | **Invite graph:** who invited whom is recorded permanently and is visible to platform staff as a graph. | Decided |
| D-024 | Each user gets **5 platform-invite credits** (lifetime). A credit is consumed **only when a code is redeemed** and the account is created. Unredeemed codes sit indefinitely and can be revoked until the moment they are redeemed. | Decided |
| D-025 | After credits are exhausted, extra credits cost **USD 20 each (operator-configurable)**, paid in **Bitcoin** to the operator-configured wallet. | Decided |
| D-026 | Outstanding (unredeemed) codes a user can hold at once ≤ their remaining credits. | Default |
| D-027 | Login: username/email + password **and** "Continue with Discord" (OAuth). Users who sign up through Discord must also set a username, email, and password. | Decided |
| D-028 | Additional auth: TOTP 2FA, passkeys (WebAuthn), and recovery codes. Recovery never depends on Discord. | Default |
| D-029 | **Deleting an account** makes it permanently un-loginable. The account row remains so old messages still render (shown as "Deleted User"). Messages are **not** removed unless the user deleted them individually beforehand. | Decided |
| D-030 | A person who deleted their account needs a **new platform invite** to return. They get a **new account**, which staff link to the same **Person record**, so moderation history follows them. If they come back through the same Discord identity (or other matching identifiers), the link is made automatically. | Decided |
| D-031 | If an invitee is banned for **serious cause**, their inviter is flagged "under suspicion." Repeat occurrences escalate (configurable thresholds). Ordinary bans do not affect the inviter. | Decided |
| D-032 | Age gate: one-time "Are you 18 or older?" dialog when first entering age-restricted spaces or media. The answer is remembered permanently on the account. | Decided |

## Lounge (chat)

| ID | Decision | Status |
|---|---|---|
| D-040 | Default look matches Discord (layout, spacing, menus, profiles, settings) as of the reference screenshots. Nitro, Shop, and Quests are removed. | Decided |
| D-041 | Feature scope = Discord parity, including threads, forum channels, announcement channels, stage channels, scheduled events, polls, voice messages, soundboard, stickers, onboarding, AutoMod, and server templates. | Decided |
| D-042 | Chat upload limit: **30 MB** default per file. Donors get up to the system maximum of **500 MB** per file. | Decided |
| D-043 | Attachments are part of the message and live as long as the message. **Exception:** files larger than the 30 MB standard limit ("large attachments," donor-only) **expire 7 days** after upload. The message remains with an "expired" placeholder. | Decided |
| D-044 | Discord's paid cosmetics (animated avatars, banners, profile themes, custom emoji anywhere, etc.) are **free**. | Default (follows D-070) |
| D-045 | No noise suppression feature. | Decided |
| D-046 | **Voice input works like Discord: Voice Activity or Push-to-Talk, the user chooses.** Push-to-talk is an option, not a requirement. Each user picks Voice Activity (automatic sensitivity or a manual threshold) or Push-to-Talk (keybind, release delay) in User Settings → Voice & Video. As on Discord, a channel can require push-to-talk by denying the `USE_VAD` permission. Corrected 2026-10-13 by the owner; the earlier row said push-to-talk was required. | Decided |

## Voice and video

| ID | Decision | Status |
|---|---|---|
| D-050 | Free allowance: up to **5 simultaneous video senders** (webcam or screen share) per voice channel. One person sharing both webcam and screen counts as 2. | Decided (number from cost math) |
| D-051 | More senders, higher quality, and bigger audiences are unlocked by server donation levels or a personal donation. See doc 08. | Decided |
| D-052 | A personally donating user's video never counts against a server's sender allowance. | Decided |
| D-053 | **End-to-end encryption** for voice and video calls (DMs, group DMs, voice channels, streams), similar to Discord's DAVE. Text is hop-by-hop (TLS) only. The server can read text. | Decided |

## Bots, developers, extensions

| ID | Decision | Status |
|---|---|---|
| D-060 | SIDE ROOM has its own bot/app API, intentionally shaped like Discord's (routes, objects, gateway, permissions) so Discord bots port with minimal changes. | Decided |
| D-061 | Two kinds of developers: **platform developers** (people working on SIDE ROOM's own codebase) and **app developers** (everyone else, equivalent to Discord's developer portal). | Decided |
| D-062 | App developers must apply. Until approved, their apps' tokens work only in **test servers with ≤ 25 members**. Releasing an app for general installation needs staff approval of that app and each new version. | Decided |
| D-063 | Customization (themes, profile styling, MySpace-level personalization) is declarative and **never executes user code**. Packages come from trusted sources and pass staff review. | Decided |
| D-064 | Once staff approve an extension, theme, or app version, any server owner can enable it from settings with no further approval. Updates need fresh review. | Default |
| D-065 | Activities (embedded apps in voice/text) are reviewed web apps running in a sandboxed, cross-origin iframe. They are app-developer code, reviewed by staff. This is not user customization. | Default |

## Donations

| ID | Decision | Status |
|---|---|---|
| D-070 | Most features are free. Only features that raise hosting cost are behind donations. | Decided |
| D-071 | **Payments are crypto only.** BTC is required. Lightning and Monero are optional later. | Decided |
| D-072 | Two kinds of donation: **personal** (perks follow you everywhere; target ≈ $5/month) and **server** (members pool donations through the server's settings to reach monthly levels, like Discord boosts). | Decided |
| D-073 | Benefits require **monthly** support, with a configurable grace period (default 3 days). | Decided |
| D-074 | Public leaderboards: top donors (individuals; donors can hide name and/or amount), and top contributors of approved content, counted separately as **movies, episodes, and manga chapters**. Replacements and duplicates earn nothing. | Decided |
| D-075 | **Downloading** library media (video) is a donor-only perk. Browser streaming is free. | Decided |

## Content libraries

| ID | Decision | Status |
|---|---|---|
| D-080 | Libraries (Cinema, Library) are **global** to the installation. | Decided |
| D-081 | Regular users can upload content and fill in its metadata, then submit for review. Reviewers can preview, edit metadata, and approve or deny. Authorized uploaders publish directly. | Decided |
| D-082 | Upload size for library submissions is **not** limited by the chat cap. Regular users may be rate-limited. Staff are not. | Decided |
| D-083 | Every upload source is supported: browser files, drag-and-drop folders, batch import, server-side watched folders, and import from an existing storage location. A published file-naming convention drives automatic metadata detection. | Decided |
| D-084 | Manga pages arrive as JPEG/PNG/WebP and are stored as **WebP**. | Decided |
| D-085 | Cinema scope: movies, series, anime ("Netflix-level features that are easy to implement with code"). | Decided |
| D-086 | Video ceiling is **1080p**. All video is **pre-transcoded** into an adaptive ladder by GPU workers in batches, so no GPU is needed at runtime. | Decided |
| D-087 | Reading scope: manga, manhwa/webtoons, Western comics, PDFs, EPUB, and other ebooks. Multiple languages, editions, and releases per series. | Decided |
| D-088 | Reader: RTL/LTR, single and double page, vertical scroll, zoom, bookmarks, cross-device progress, downloads (native clients later). | Decided |
| D-089 | Ratings, reviews, comments, lists, update notifications, recommendations. Trending and ratings can be filtered by **Friends**, **People you know** (the deduplicated union of members of all your servers), or **Global**. | Decided |
| D-090 | Bounties on requested content are **documented as future only**. Not built. | Decided |

## Live TV

| ID | Decision | Status |
|---|---|---|
| D-100 | The Live TV page has a retro TV-guide look. Channels are **assembled from the library** and follow a **continuous, wall-clock schedule**. If the guide says an episode is 17 minutes in, joining viewers start 17 minutes in. | Decided |
| D-101 | Nothing streams when nobody is watching. Delivery starts when someone tunes in and stops when the last viewer leaves. | Decided |
| D-102 | Imported historical lineups (e.g. "Adult Swim, Fall 2003"): a slot whose content isn't in the library is **skipped**. Channels without a guide follow their configured rules (genre shuffle, etc.). | Decided |
| D-103 | Channel schedules are defined in a **channel time zone** (default: the operator's time zone). The guide shows times in the viewer's local time, with a toggle for "channel time." | Default |

## Watch parties

| ID | Decision | Status |
|---|---|---|
| D-110 | Synchronized watch parties with host controls, optional shared controls, queue, chat, and voice. They can be started from server text and voice channels. Voice continues while navigating between sections. | Decided |

## Privacy and retention

| ID | Decision | Status |
|---|---|---|
| D-120 | Watching/reading activity is **public by default**, visible only to **friends and people who share a server**. Users can turn it off. Reviews are public independently of history. | Decided |
| D-121 | A user-deleted message disappears from all user views immediately and is **purged after 30 days**. Until then, high-level platform staff can view it. | Decided |
| D-122 | Messages under a moderation hold are never purged from staff view. Users can still delete them from user view. | Decided |
| D-123 | Undeleted messages, text, and normal attachments are retained indefinitely. | Decided |
| D-124 | Account data is never deleted (see D-029/D-030). | Decided |
| D-125 | Only **high-level platform staff** can read private content (DMs, private channels). All such access is written to a tamper-evident access log visible to the Owner. | Decided / Default (logging) |
| D-126 | Moderation and legal-compliance tooling is a **later phase**. Schemas reserve hold flags and case links now. | Decided |

## Staff and administration

| ID | Decision | Status |
|---|---|---|
| D-130 | A single **Staff Server** with a fixed identity exists in every installation. Membership in it grants platform-staff status. Its roles map to platform permissions. Leaving or being removed revokes staff powers immediately. | Decided |
| D-131 | The **host/operator** owns the Staff Server and holds the highest platform authority (**Owner**). | Decided |
| D-132 | Ordinary invites and ordinary role grants cannot make someone staff. Staff are appointed through an **Owner-authorized appointment** action. The Owner may delegate appointment power to senior staff. | Decided |
| D-133 | Everything staff need to do can be done inside the platform (Staff Server channels plus admin pages). | Decided |

## Clients and style

| ID | Decision | Status |
|---|---|---|
| D-140 | Clients: desktop browser + mobile PWA now. Native desktop, Android, and iOS later (out of scope). | Decided |
| D-141 | Default styles per section: Lounge looks like Discord. Cinema, Library, and Entrance use the Side Room palette (deep olive-black, cream text, muted gold accents, serif display type). Users can override per section, including a "Side Room everywhere" theme. | Decided |
| D-142 | Customization is maximal but safe: settings, approved theme packages, approved profile layouts. No executable user code. | Decided |

## Engineering

| ID | Decision | Status |
|---|---|---|
| D-150 | **Rust for all backend code.** If a good Rust implementation of a component exists (database, codec, media tool, etc.), prefer it. Use Rust/WASM on the frontend where it helps. Don't reinvent the wheel. | Decided |
| D-151 | Frontend: **TypeScript + React** (Option A), with Rust → WASM modules. | Decided |
| D-152 | Docs-first, agent-friendly repo: per-section specs, a component inventory, an AGENTS.md handoff guide, and small PRs. | Decided |
| D-153 | Hosted on a rented server. Must be installable by another operator from the docs. | Decided |
| D-154 | Delivery order: Lounge first, then Library (manga), then Cinema (media). | Decided |
| D-155 | Repository is on GitHub; visibility and license are at the owner's discretion. | Decided |
| D-156 | **CI and local tooling run Node 24** (`.nvmrc`; `engines` stays `>=22`). The web test setup bridges jsdom abort signals past Node 24 undici's brand check. | Decided |

## Phase 2 defaults (open-question answers, 2026-10-13)

Adopted from the "Phase 2 answers" section of [open-question-answers.md](open-question-answers.md). Entries marked Default were chosen under the owner's delegation; D-171 carries the owner's pending confirmation.

| ID | Decision | Status |
|---|---|---|
| D-157 | **Emergency operator invites.** `sr-cli invite create` consumes no credit and writes no ledger row; the invite is flagged emergency so redemption spends nothing; a `platform_audit_log` row `invite.emergency_create` (actor = system user 1, target = the recorded issuer) is the audit trail. The issuer is stored, so the invite graph stays complete. (OQ-1) | Default |
| D-158 | **Guild audit-log retention is 45 days**, from the platform setting `moderation.audit_retention_days` (`0` = forever), enforced by the `audit.purge` job. The platform audit log and the staff access log are kept forever. (OQ-2) | Default |
| D-159 | **AutoMod ships no built-in word lists.** Preset triggers resolve against operator-curated lists in Admin → Features → AutoMod; shipping a list is a separate opt-in decision with its own licence review. (OQ-3) | Default |
| D-160 | **Channel listings are permission-filtered.** `GET /guilds/{id}/channels` returns only channels the caller can view; owners, `ADMINISTRATOR` and bots with it see all. Recorded in the porting checklist (doc 05 §7). (OQ-4) | Default |
| D-161 | **Passkeys: pure Rust first, ADR fallback.** ENT-AUTH-010 starts with a two-day spike on pure-Rust relying-party crates (`none` attestation only; ES256, EdDSA, RS256). If none passes, ADR-0009 accepts `webauthn-rs` with vendored `openssl` and doc 03 §4 gains the row. Passkeys stay deferred until one of the two lands. (OQ-5) | Default |
| D-162 | **Default Staff Server role bits** are the table in doc 07 §4 (Support 10241, App Reviewer 769, Library Curator 85, Media Curator 203, Moderator 7169, Senior Staff 1818623, Owner bit 63) with its grant rules; roles are ordinary editable roles after bootstrap. (OQ-6) | Default |
| D-163 | **Postgres-backed tests run in CI.** `rust.yml` runs the `#[ignore]` repository tests against its `postgres:17` service container (`cargo nextest run --workspace --run-ignored only -j1`, `SR_TEST_DATABASE_URL` set); every such test provisions or tolerates a reused database. (OQ-7) | Default |
| D-164 | **Webhook tokens are never readable after creation.** They are stored as SHA-256 hashes (doc 09 §2); `GET /webhooks/{id}` omits the token and `POST /webhooks/{id}/reset-token` issues a fresh URL shown once. No sealed copy. (OQ-25) | Default |
| D-165 | **Notification level precedence:** channel → category → the member's per-server choice → the member's global default (`/users/@me/notification-defaults`) → the server's `default_message_notifications`. Servers only; DMs keep their own scope. (OQ-26) | Default |
| D-166 | **Prune uses last-seen plus messages.** `accounts.last_seen_at` is kept by the gateway (IDENTIFY/RESUME, then at most every 10 minutes) and login; a member is inactive for N days when they joined more than N days ago, were last seen more than N days ago (or never) and sent no message in the server within N days. The timestamp is never exposed in user objects. (OQ-27) | Default |
| D-167 | **Email change needs the second factor when one is enabled.** `POST /sr/auth/email/change` requires the password plus a fresh second-factor proof (TOTP, recovery code, or passkey once they exist; a re-prompt within 15 minutes counts); the link to the new address stays; the old address receives a notice with a "This wasn't me" link that cancels a pending change or revokes all sessions after a completed one. (OQ-28) | Default |
| D-168 | **Manga pages are lossy WebP at quality 85** via the `webp` crate (libwebp, already accepted by ADR-0005); PNG sources are also encoded lossless and the smaller file is kept. Variants follow the same rule. (OQ-29) | Default |
| D-169 | **AVIF pages are not accepted.** Archives containing `.avif` pages are rejected at ingest with the file names; the naming spec drops `avif`. Revisit with `rav1d` when it offers a stable still-image decode path; `dav1d` (C) is not approved. (OQ-30) | Default |
| D-170 | **Archive formats:** ZIP/CBZ and 7z/CB7 (`sevenz-rust2`, pure Rust) under the shared hardening limits; RAR/CBR through the `unrar` bindings ADR-0005 already accepts, behind the platform setting `library.ingest.rar_enabled` (default on), extracted only inside the ingest job. (OQ-31) | Default |
| D-171 | **(Superseded by D-184.) Age-restricted library titles are shown by default to confirmed adults** (`AgeFilter::Include`), with "hidden" and "only" as filter options; unconfirmed accounts never see them. User setting `library.show_age_restricted` (default on) and platform setting `library.age_restricted_default_visible` (default true) flip the default without code. (OQ-32) | Default (owner to confirm) |
| D-172 | **Library publisher permissions:** `PUBLISH_READING` creates, edits, publishes and changes pages; `DELETE_CONTENT` deletes series, editions, chapters and volumes; merge needs both; unpublish accepts either. A Library Curator can unpublish but not delete. (OQ-33) | Default |
| D-173 | **Library browse sorts** are `updated`, `newest`, `title`, `followers` until LIB-SOC-001/004 add `rating` and `trending`; unknown sorts are 400 and the UI offers only what the API has. (OQ-34) | Default |
| D-174 | **The composer stays a textarea through Phase 2**; mentions, channels, emoji and timestamps are readable text mapped to wire tokens at send. LNG-CMP-001 becomes "rich composer on Lexical", scheduled after the LNG-MSGUI gaps. (P2-A) | Default |
| D-175 | **Chat uploads stay presigned PUTs to the bucket.** The bucket carries a CORS rule for the app origin (`PUT, GET, HEAD`, `Content-Type`, expose `ETag`) applied by `sr-cli storage ensure-cors` and called from bootstrap/install; the presigner signs method and path only, so the client's `Content-Type` header is advisory and the server sniffs the real type on finalize. (P2-B) | Default |
| D-176 | **The client never loads third-party image URLs directly.** Until the image proxy (LNG-MSG-014) exists, embeds render text and offer "Load image" per embed; once it exists, the unfurler fills `proxy_url` (`/cdn/external/…`, SSRF-safe, size/time limits, re-encoded, cached 30 days, operator allow/deny lists) and the client loads only `proxy_url`. (P2-C) | Default |
| D-177 | **Spoiler attachments use Discord's `SPOILER_` filename prefix.** The upload-slot `spoiler: true` field stays as input; the server prefixes the stored filename, so `AttachmentObject` needs no extra field. (P2-D) | Default |
| D-178 | **The client computes channel-level permissions** with the shared `sr-permissions-wasm` `compute` (roles, overwrites, timeout rule) and uses them for every per-channel capability; pin requires `PIN_MESSAGES`. The server remains the boundary. (P2-E) | Default |
| D-179 | **Rail order and folders persist in the user settings blob** (`/users/@me/settings`) under Discord's keys `guild_positions` and `guild_folders`, written as a merge patch after a drag, applied from `SR_USER_SETTINGS_UPDATE`; `localStorage` is the offline cache. Collapsed categories, remembered channel and panel width go in the same blob. (P2-F) | Default |
| D-180 | **Create-from-template is a server operation.** `POST /guilds/templates/{code}` and `POST /guilds {template}` instantiate in one transaction; the eight built-in templates live in Rust as Discord-compatible template JSON and are listed by `GET /guilds/templates/builtin`; the client's sequential POSTs are removed. (P2-G) | Default |
| D-181 | **READY `private_channels` carry full `recipients` user objects** (and DM `CHANNEL_CREATE` the same); the private `recipient_ids` field and the client's boot-time `GET /users/@me/channels` workaround are removed. (P2-H) | Default |
| D-182 | **Invite defaults:** "Copy link" and any pasted URL create an external invite (single use, 7 days, D-022); "Invite friends" DMs use the server defaults (7 days, unlimited) because they never leave the platform. The dialog's pickers edit either. (P2-I) | Default |
| D-183 | **Friends update live from `RELATIONSHIP_ADD/UPDATE/REMOVE`** (already dispatched per user by the backend); the client reducer applies them and the Friends page stops re-fetching after each action. (P2-J) | Default |

## Owner overrides (2026-10-13)

| ID | Decision | Status |
|---|---|---|
| D-184 | **Age-restricted library titles are visible to everyone; opening them needs age confirmation.** Browse, search, detail, chapter lists, feeds, follows, covers and metadata include age-restricted series for every account, confirmed or not, and nothing is hidden or answered 404 for being unconfirmed. Opening the pages (`GET /library/chapters/{id}/pages`, which also yields the signed page URLs) of an age-restricted series needs `accounts.age_confirmed_at`; without it the API answers 403 with code `900011` (`AGE_CONFIRMATION_REQUIRED`) and the client shows the same "Are you 18 or older?" dialog as age-restricted Lounge channels (D-032). `POST /sr/age-gate {over_18:true}` records the yes permanently and the retried request succeeds; the question is never asked again. The `age=` query filter defaults to `include` (`hide` and `only` stay as optional filters). The optional user setting `library.hide_age_restricted` (default false) leaves restricted titles out of listings, search, updates and follows for members who prefer that. The platform setting `library.age_restricted_default_visible` and the user setting `library.show_age_restricted` from D-171 are dropped. Overrides OQ-32 and D-171. | Decided (owner) |

## Phase 8 to 11 handoff answers (2026-10-15)

Adopted from the "Phase 8 to 11 handoff questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Entries marked Default were chosen under the owner's delegation and can be changed without breaking anything else; the implementation work is in [dev/answers-followups-p12.md](dev/answers-followups-p12.md).

| ID | Decision | Status |
|---|---|---|
| D-185 | **The interactions-endpoint signing seed is sealed.** `applications.interactions_signing_seed` is encrypted with `server.secret_key` like TOTP secrets and the VAPID private key; the public key stays derivable; `secret_columns.rs` lists it as sealed, never `PLAINTEXT_OK`. (P11-DEV-1) | Default |
| D-186 | **Failing interaction endpoints are disabled automatically.** 25 consecutive failed deliveries within 10 minutes mark the endpoint `disabled`; interactions fall back to the gateway, the owner gets a system DM, and the portal offers "Verify and re-enable" (PING check). Discord parity. (P11-DEV-3) | Default |
| D-187 | **Feature-module actions are attributed to the system actor.** Audit entries written by a module name the system user (id 1) with `options.module = "<feature id>"`, as Discord attributes AutoMod; never the server owner. (P11-FM-2) | Default |
| D-188 | **The system user is not a visible member.** It keeps a membership row where it posts but is excluded from member lists, counts, the lazy list, prune, mention autocomplete and `@everyone` fan-out; its messages carry the `SYSTEM` tag; it cannot be kicked, banned or given roles. (P11-FM-4) | Default |
| D-189 | **Bots are denied on user-scope feature routes** (reminders, scheduled messages, bookmarks, birthdays) in `auth/policy.rs`. (P11-FM-5) | Default |
| D-190 | **Activity sharing is a server-side setting and drives the scoped aggregates.** The D-120 toggles live in user settings (`activity.share_cinema`, `activity.share_library`, `activity.share_tv`, `activity.hide_age_restricted`, `activity.paused`); Friends and "People you know" trending and rankings count only members who share that section, exclude paused members, and exclude age-restricted titles for members hiding them. No new setting. (P11-LIB-1) | Default |
| D-191 | **Push is suppressed while a session is active.** No Web Push while any of the user's gateway sessions is connected and not idle; it resumes when every session is idle or gone. Discord parity. (P11-MSG-3) | Default |
| D-192 | **Platform announcements.** `critical` announcements cannot be dismissed and are always sent as a system DM; ordinary ones are dismissable and are DM'd only when the author asks and the recipient's `notifications.platform_dm` (default on) allows. No scheduling. (P11-CLI-2, P10-ADM-7) | Default |
| D-193 | **Activity type 6 = Reading** is SIDE ROOM's one extension to Discord's activity types 0 to 5; the fields match type 3. (P11-SHL-1) | Default |
| D-194 | **Staff cannot self-delete.** An account with an active staff appointment must leave the Staff Server first; the Owner cannot delete until `sr-cli staff transfer-owner` has run. (P11-USB-1) | Default |
| D-195 | **Copy ID is gated on Developer Mode**, as on Discord; the earlier "always visible" choice is overridden. (P11-USF-1) | Default |
| D-196 | **The daily submission upload allowance is a platform setting**, `submissions.daily_upload_bytes` (default 50 GiB, `0` = unlimited), in the feature registry; staff are exempt (D-082). (P11-REV-1) | Default |
| D-197 | **Direct appointment kept, Owner only, behind re-auth (owner decision 2026-10-15).** `POST /admin/staff` (appoint without acceptance) stays but refuses everyone except the Owner, including staff holding `APPOINT_STAFF`, and still needs the 15-minute second-factor window. The offer flow (ADM-STF-004) is the default path for everyone; `sr-cli staff appoint` (audited as `staff.appoint_cli`) covers bootstrap and emergencies. Supersedes the earlier "remove it" default. (P10-ADM-1) | Owner |
| D-198 | **Staff re-auth stays on by default, and accepting an appointment requires an enrolled second factor.** Nobody holds staff powers without TOTP or a passkey; `SR_ADMIN_REQUIRE_REAUTH=0` is for development and e2e only. (P10-ADM-4) | Default |
| D-199 | **The gateway enforces the age gate.** Message, reaction, typing, pin and thread dispatches from an `nsfw` channel (or its threads) are not delivered to sessions of accounts without `age_confirmed_at`; the channel object itself still is. (P10-LBE-1) | Default |
| D-200 | **Temporary members get a 60-second offline grace** before removal (Discord removes at once), so a reload or network blink is not an eviction; a startup sweep removes those whose last session ended before a restart. Parity exception. (P10-LBE-2) | Default |
| D-201 | **No server-side Opus encoding, no libopus.** Soundboard sounds and voice messages are stored as uploaded (MP3/Ogg/WAV, 512 KiB, 5 s) and decoded by the browser; libopus is not on the doc 03 §4 list and no ADR is written for it. Revisit only with a maintained pure-Rust encoder. (P10-SBE-1) | Default |
| D-202 | **`USE_EXTERNAL_SOUNDS` and `START_WATCH_PARTY` are backfilled** into every existing guild's `@everyone` role by migration (both are free: D-044, D-110). (P10-VBE-3) | Default |
| D-203 | **Every outbound HTTP request to a non-constant address goes through `sr_net::SafeClient`**; `cargo deny` bans a direct `reqwest` dependency outside `sr-net` and the fixed-URL exceptions (Discord OAuth, Tenor). (P9-SEC-4) | Default |
| D-204 | **App testers.** Up to 25 accepted testers per application; a `development` application installs where the owner or a tester holds `MANAGE_GUILD`, still only in guilds of at most 25 members (doc 11 §2.1). (P8-DEV-1) | Default |
| D-205 | **The bot install permission cap stays**: the managed role gets `requested AND the installer's own permissions`; no escalation past the installer. Parity exception, recorded in doc 05 §7. (P8-DEV-2) | Default |
| D-206 | **Channel-visible interaction responses need the bot's `SEND_MESSAGES`**; ephemeral responses are always allowed. Parity exception, recorded in doc 05 §7. (P8-DEV-3) | Default |
| D-207 | **Bitcoin derivation stays in-house** (`secp256k1`, `bitcoin_hashes`, `bech32`), no `bdk_wallet` and no `MITNFA` exception; ADR-0006 gets an addendum naming the substitution and the revisit trigger. (P8-DON-1) | Default |
| D-208 | **No dedicated watch-party channel type.** A persistent party bound to a voice channel is the "watch party room"; the channel editor offers it as a preset. (P8-WP-1) | Default |
| D-209 | **Report routing and reporter visibility** per moderation spec §3.4: guild content to the guild's moderators, escalation and the always-escalated categories (`illegal_content`, `child_safety`), DMs, users, servers and library/cinema items to platform staff; the reporter is hidden from server moderators unless they opt in, always visible to platform staff, access-logged. (MOD-1) | Default |
| D-210 | **Platform bits `MANAGE_CASES` (22) and `MANAGE_TAKEDOWNS` (23)** are added; Moderator gains `MANAGE_CASES`, Senior Staff both (D-162 table updated by migration). (MOD-2) | Default |
| D-211 | **Serious-by-default ban reasons** are `illegal_content`, `child_safety`, `fraud`, `platform_abuse`; `harassment` and `ban_evasion` default to standard and can be raised in the ban dialog; the stored `category` remains what D-031 reads. (MOD-3) | Default |
| D-212 | **Closed cases, evidence and takedown notices are kept forever** (D-124); holds never expire by themselves but are flagged for review after `moderation.hold_review_days` (default 90); releasing needs a reason. (MOD-7) | Default |
| D-213 | **Holds are placed by platform staff only** (`MANAGE_CASES`); server moderators use guild actions. (MOD-8) | Default |
| D-214 | **Report snapshots are readable by case staff**: the reported message plus ten before and after, taken at report time; opening one is access-logged with the case id; anything beyond needs `READ_PRIVATE_CONTENT` (D-125). (MOD-9) | Default |
| D-215 | **Appeals ship with cases**: one appeal per decision, from the ban screen, reviewed by a different staff member (the Owner when nobody else holds the bit). (MOD-10) | Default |
| D-216 | **Clients read platform flags from `GET /sr/features/flags`** (flags marked `client_visible`, any signed-in human), replacing build-time seeds such as `VITE_FEATURE_MODERATION_REPORTS`. (MOD-11) | Default |

## Phase 12 handoff answers (2026-10-15)

Adopted from the "Phase 12 handoff questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). The direct-appointment owner decision of the same day (keep `POST /admin/staff`, Owner only, behind re-auth) is recorded by the amended D-197 on `feat/p12-admin-mod-be` and gets no new number here. Implementation work is G-68 onward in [dev/answers-followups-p12.md](dev/answers-followups-p12.md).

| ID | Decision | Status |
|---|---|---|
| D-217 | **PDFs are read in the browser with `pdfjs-dist`; no server rasterizer.** `pdfjs-dist` (6.4.299) is an approved `apps/web` dependency, loaded lazily as its own chunk with a real worker; the server only validates PDFs (`pdf.rs`), the cover of a PDF submission is rendered client-side and submitted, and reading progress is the page index. `pdfium-render` stays approved under ADR-0005 but is not used or bundled; LIB-ING-004's image mode is closed, not deferred. Closes open-questions item 2 and P11-LIB-2. (P12-WFU-1) | Decided (owner) |
| D-218 | **`/rank`, `/suggest` and `/ticket` are offered in every server.** When the matching module is off, the Community dialog says so and links people who manage the server to Server Settings → Features; the command set does not change per server. (P12-FUI-1) | Decided (owner) |
| D-219 | **The birthday card lives on User Settings → Profiles → Servers.** Birthdays are announced per server, so the card sits with the per-server profile; the main profile tab keeps Discord's layout. (P12-FUI-2) | Decided (owner) |
| D-220 | **App tester slots count invitations, and testers may install private development apps.** The 25-tester cap of D-204 counts pending invitations together with accepted testers (a developer frees a slot by withdrawing an invitation); an accepted tester may install a `development` application whether or not it is public, still only where they hold `MANAGE_GUILD` in a guild of at most 25 members. Amends D-204. (P12-DPB-1, P12-DPB-2) | Default |

## Phase 12 backend handoff answers (2026-10-15)

Adopted from the "Phase 12 backend handoff questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is G-89 onward in [dev/answers-followups-p12.md](dev/answers-followups-p12.md).

| ID | Decision | Status |
|---|---|---|
| D-221 | **Group DM invite links use Discord's invite object.** `InviteObject.type` is 0 for a server and 1 for a group DM; `guild` is optional (absent for groups) and `channel` carries the group's id, type 3, name and icon. Links last 24 hours with unlimited uses, are made and revoked by the group's owner, stay in-platform (D-022's single-use rule does not apply), and joining is refused when the joiner and the group owner block each other or the group already has 10 recipients. (P12-LCB-1) | Default |
| D-222 | **Scheduled events start like Discord's.** External events go `ACTIVE` at their start time and `COMPLETED` at their end time by the clock. Voice and stage events stay `SCHEDULED` until someone with `MANAGE_EVENTS` starts them or any member joins the linked channel at or after the start time; one still `SCHEDULED` 24 hours after its start time is marked `CANCELED`, and an `ACTIVE` one ends at its end time or, without one, after the channel has been empty for 15 minutes (a stage event also ends with its stage instance). Starting a stage event opens the stage instance (`entity_id`); interested members are notified when an event starts; events carry a cover `image`; no recurrence. (P12-LCB-2, P12-LCB-6) | Default |
| D-223 | **The server 2FA requirement covers Discord's elevated permissions, and bots are exempt on Discord's condition.** With `mfa_level` 1, every action behind `KICK_MEMBERS`, `BAN_MEMBERS`, `ADMINISTRATOR`, `MANAGE_CHANNELS`, `MANAGE_GUILD`, `MANAGE_MESSAGES`, `MANAGE_ROLES`, `MANAGE_WEBHOOKS`, `MANAGE_THREADS`, `MANAGE_GUILD_EXPRESSIONS` and `MODERATE_MEMBERS` (AutoMod, community and safety settings and invite deletion included; invite creation and a member's own content excluded) needs a second factor proved in the last 15 minutes: 403 `900004` for a session that has one and has not proved it, 403 `900123 MFA_ENROLLMENT_REQUIRED` for an account with no TOTP and no passkey. Bots are exempt when the application's owner account has a second factor enrolled, and get `900123` otherwise. (P12-LCB-3) | Default |
| D-224 | **Server Protection settings route and mask; nothing scans.** `dm_spam_filter` sends a DM from a non-friend to Message Requests when every shared server has it on (it can only tighten the recipient's own DM setting); `warn_new_account_days` does the same for accounts younger than the largest shared threshold and marks the request `sr_request_reason: "new_account"`, with the age read from the snowflake; `explicit_media_filter_dms` delivers attachments in message-request conversations with the spoiler flag set, and no image classifier ships (LNG-SSET-054's scanner hook stays unbuilt until an operator asks); `hide_suspicious_links` renders a link whose text names another host as its raw target with a warning, and the setting is visible to members on the guild object's community block. No message is deleted and no sender is blocked by these settings. (P12-LCB-5) | Default |

## Phase 12 and 13 remaining answers (2026-10-15)

Adopted from the "Phase 12 and 13 remaining questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). D-225 and D-226 are owner decisions given in chat the same day and are recorded as given. Implementation work is H-01 onward in [dev/answers-followups-p13.md](dev/answers-followups-p13.md).

| ID | Decision | Status |
|---|---|---|
| D-225 | **`foliate-js` is the EPUB reader engine (owner decision 2026-10-15).** It is an approved `apps/web` dependency (MIT, pinned, lazy chunk under the library budget); the server only validates EPUBs with the `epub` crate and serves the kept file; reading progress for a book is the EPUB CFI stored beside the page index on the chapter-progress route. Closes LIB-RDR-010's dependency question. (P13-LIB-1) | Decided (owner) |
| D-226 | **"Share to Lounge" posts a card into a channel the person picks (owner decision 2026-10-15).** Library series and Cinema titles are shared as the `<library:id>` / `<cinema:id>` rich card in an ordinary message sent by the person from their own session (optional comment, channel picker limited to channels where they hold `SEND_MESSAGES` and `USE_LIBRARY_EMBEDS`, DMs and groups included); "Copy link" stays as a second item. No poster port, no system user. (P13-LIB-2) | Decided (owner) |
| D-227 | **A watch party room may be empty.** A persistent party bound to a voice channel can have no media (`media: null`, state `idle`); the room preset no longer asks for a title; the first person to join an idle room hosts and picks the media, and the play gate, limits and age rule run then. Ad-hoc parties still need media at creation. (P12-TVW-2) | Default |
| D-228 | **The creator of a scheduled event is marked Interested on creation**, as on Discord, and can un-mark themselves; start notifications therefore reach the creator through the ordinary interested path. Events created by bots mark nobody. (P13-EVT-1) | Default |
| D-229 | **Reading presence has the Cinema's three audiences.** `library.share_reading` is `off`, `friends` (default) or `everyone` (friends and people who share a server, never strangers), under the D-190 master switches; age-restricted series are masked beyond friends. (P13-LIB-3) | Default |
| D-230 | **User-level safety settings exist and act on what the platform knows; nothing scans.** On `/users/@me/privacy`: `sensitive_media {friends, others, servers}` with `show`, `blur` or `block` (defaults show, blur, show; `others` acts on image and video attachments in DMs from non-friends, `servers` on those in age-restricted channels, `friends` has no signal until a scanner hook exists; `blur` is the spoiler flag for that viewer, `block` delivers the attachment without a URL and with `sr_filtered: true`, nothing is deleted); `dm_spam_filter` is `off`, `non_friends` (default) or `all`, and server settings can only tighten it (D-224); `server_dm_overrides` is Discord's per-server "allow DMs from server members"; `GET|DELETE /users/@me/age-confirmation` shows and resets the age confirmation behind re-auth. (P13-WFE-3) | Default |
| D-231 | **Burst (super) reactions are not built; the reaction object carries Discord's fields as a shim** (`burst: false`, `me_burst: false`, `count_details {normal, burst: 0}`, `burst_colors: []`; `?type=1` is accepted and treated as a normal reaction). The picker variant and animations are closed, not deferred. (P13-WFE-4) | Default |
| D-232 | **Custom emoji images are drawable by every signed-in member wherever the emoji appears** (`GET /sr/emojis/{id}/url` signs the variant for any session); the emoji popout names the source server only when the viewer is a member or the server is listed in the Server Directory, and "Join server" goes through the directory. (P13-WFE-5) | Default |
| D-233 | **Oversize uploads are refused in the tray before upload.** The effective limit for the caller is `sr_max_upload_bytes` on the guild object (the larger of the member's perk and the server's entitlement level) and on DM and group channels (the perk); the client blocks a larger file with the Patron hint and keeps the 413 path for races. (P13-WFE-2) | Default |

## Phase 13 developer platform and settings answers (2026-10-15)

Adopted from the "Phase 13 developer platform and settings questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is H-28 onward in [dev/answers-followups-p13.md](dev/answers-followups-p13.md).

| ID | Decision | Status |
|---|---|---|
| D-234 | **`sr-sdk` is the one library crate with a plain `reqwest` dependency (an exception to D-203, not a change to it).** The SDK runs in an operator's bot, never in the server; it contacts only the base URL its operator configured and a gateway or resume URL whose host equals that base host or a host the operator allow-listed (anything else falls back to the configured gateway URL); it never fetches an address found in message content. `deny.toml` cites this decision; `SafeClient` stays mandatory inside the server. (P13-DEV-1) | Default |
| D-235 | **A webhook message shows the webhook as its author and keeps its creator in the data model.** `author.id` and `webhook_id` are the webhook id, the name is the per-call override or the webhook name, `bot: true`; `messages.author_id` stays the creator whose permissions and second-factor state applied at send time, and the rows outlive the creator and the webhook. Staff views, moderation cases and data exports list such messages under the creator with a "via webhook" marker; an avatar override is shown only through the image proxy, and never without a configured signer; `message_webhook_authors` rows are purged with their message. (P13-DEV-2, P13-DEV-6) | Default |
| D-236 | **The App Directory lists `released`, non-suspended applications with `bot_public` under their own tags (at most 5); there is no separate category field, listing edits after release are not re-reviewed, and suspension is the only staff lever.** (P13-DEV-3) | Default |
| D-237 | **Application emoji are bot-manageable and renderable; Rich Presence assets are owner-session only.** `/applications/{id}/emojis` accepts the owner's session or the bot token of that application (nobody else); `<:name:id>` of an application emoji renders in messages and interaction responses sent by that application's bot and stays text elsewhere; `GET /sr/emojis/{id}/url` resolves it for any session (D-232) and the popout names the application. `/applications/{id}/assets` writes stay owner-session, the list is readable by any session. (P13-DEV-4) | Default |
| D-238 | **The effective upload limit includes the server level, and the number is REST-only (amends D-233).** The chat upload check and `sr_max_upload_bytes` read `Entitlements::limits(user, guild)` from `sr-payments`, so the limit is `max(free, server level perk, Patron perk)` in a server and the perk alone in DMs and groups; `GUILD_CREATE`, `GUILD_UPDATE` and READY `private_channels` do not carry the field; the client reads `GET /guilds/{id}` and `GET /users/@me/channels` and refetches on `SR_ENTITLEMENT_UPDATE` (both shapes). (P13-SA-2) | Default |
| D-239 | **`MANAGE_ENTRANCE` (platform permission bit 24) covers the entrance copy keys and the hero image**, behind a fresh second factor; `MANAGE_FEATURES` no longer grants them; `OWNER` implies it. (P13-SA-3) | Default |
| D-240 | **Join source is stored and two member signals are defined; platform suspicion never reaches server moderators.** `guild_members.join_source` (`invite`, `vanity`, `directory`, `application`, `creator`, `staff`, `bot`; null = unknown), `join_invite_code` and `join_ref` are set by every join path and shown as `sr_join {source, invite_code, inviter_id}` to members with `MANAGE_GUILD`. Signals are `new_account` (snowflake age under the server's `warn_new_account_days`, default 7) and `rejoined` (a prior remove, kick or ban-then-unban audit row in this server within audit retention); "unusual activity" means "has any signal". Person suspicion, invite freezes and link state stay staff-only. (P13-SA-5) | Default |

## Phase 13 final answers (2026-10-16)

Adopted from the "Phase 13 final questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is H-44 onward in [dev/answers-followups-p13.md](dev/answers-followups-p13.md); the round's one migration is `0876`.

| ID | Decision | Status |
|---|---|---|
| D-241 | **`USE_LIBRARY_EMBEDS` (guild bit 58) is enforced and granted by default.** A message whose content carries a `<library:id>` or `<cinema:id>` card token is refused with `50013` at send when the author lacks the bit in that channel (the same check `embeds` get for `EMBED_LINKS`); the bit is in the default @everyone permissions of a new server and in the DM and group permission set, and migration `0876` grants it once to every existing @everyone role; the Share to Lounge picker (D-226) offers only channels where the person holds `SEND_MESSAGES` and `USE_LIBRARY_EMBEDS`. (P13-HC-1) | Default |
| D-242 | **Gateway op 8 `REQUEST_GUILD_MEMBERS` is served with Discord's contract.** `guild_id`, `query` or `user_ids` (at most 100), `limit`, `presences`, `nonce` (at most 32 bytes); the caller must be a member (a bot: installed in the guild); the whole list (`query: ""`, `limit: 0`) needs the `GUILD_MEMBERS` intent and `presences: true` the `GUILD_PRESENCES` intent, else close code 4014; user sessions may request by `query` and `user_ids` only and get the search-route member objects; the answer is `GUILD_MEMBERS_CHUNK` events of at most 1000 members with `chunk_index`, `chunk_count`, `not_found` and the echoed `nonce`, within the connection's existing op budget. (P13-HC-3) | Default |

## Phase 14 answers (2026-10-16)

Adopted from the "Phase 14 questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is J-01 onward in [dev/answers-followups-p14.md](dev/answers-followups-p14.md); the round's migrations are `0960` to `0965`. Nothing here decides whether `openmls` may be downloaded (asked of the owner directly).

| ID | Decision | Status |
|---|---|---|
| D-243 | **Security Actions are Discord's incident actions, and the join-rate alert pauses invites by itself.** `PUT /guilds/{id}/incident-actions {invites_disabled_until, dms_disabled_until}` (`MANAGE_GUILD`, D-223 gate; each `null` or at most 24 hours ahead) and `incidents_data {invites_disabled_until, dms_disabled_until, raid_detected_at}` on the guild object, next to the indefinite `invites_paused`. While DMs are paused, a DM between two members whose every shared server has DMs paused is refused with `50007` unless they are friends (a pause blocks; it does not file into Message Requests, which stays the standing `dm_spam_filter` of D-224). When `raid_alert_joins_per_minute` is exceeded the server sets `raid_detected_at`, pauses invites for 24 hours unless `raid_auto_pause_invites` (default on) is off, writes the audit row as the system user and posts the alert message with "Resume invites". Nothing pauses DMs automatically. (P14-LNG-2) | Default |
| D-244 | **AutoMod alerts are messages of type 24 in the alert channel, authored by the system user, with moderation buttons.** Fields: rule, trigger, channel, member, the matched excerpt (at most 1,000 characters) and whether the message was blocked; buttons Delete message, Timeout (Discord's six durations), Kick, Ban and Mark resolved. A press is checked at press time against the presser's own permissions and the second-factor gate, writes the ordinary audit row naming the presser, and rewrites the alert. The `AUTO_MODERATION_ACTION_EXECUTION` event stays for bots. (P14-LNG-3) | Default |
| D-245 | **Guild members carry Discord's `flags`, and onboarding completion is `COMPLETED_ONBOARDING` (bit 2).** `guild_members.flags` (default 0) is on the member object; `POST /guilds/{id}/onboarding-responses` sets the bit and dispatches `GUILD_MEMBER_UPDATE`; the other Discord bits stay 0 until they have a setter. The client offers the onboarding modal only to a member without the bit who joined within 3 days or just accepted the rules; everyone else without it gets the quiet "Channels & Roles" entry. No browser-side memory. (P14-LNG-4) | Default |
| D-246 | **Activities are reviewed per Activity submission, separately from the application-version review (amends doc 11).** The version manifest governs what an application may do; the Activity review governs the code served in the sandbox; any change to the Activity withdraws its approval; both appear in Review Queues and the staff access log. An approved Activity is served outside development mode only while the application's latest version is `released` and the application is not suspended. (P14-DEV-1) | Default |
| D-247 | **An Activity is an uploaded bundle that sr-server hosts and serves only in the reviewed form; the entry URL survives only as a development-mode live URL.** A zip of static files (at most 32 MiB unpacked, 2,000 entries, allow-listed extensions, `index.html` at the root) is unpacked into object storage under its SHA-256; staff approve that hash; the proxy host serves the approved bundle and nothing else, with the proxy's existing CSP, headers and path rules; a new upload is a new `draft`. URL mappings keep going through `SafeClient`. A `development` application may set `dev_entry_url`, served live and unpinned only while it is in development (D-204 audience). (P14-DEV-2) | Default |
| D-248 | **Activity authorization and reach.** Scopes an Activity may request: `identify` and `guilds` only. `https://<application-id>.activities.<host>/` is an implicit, unstored redirect URI of any application with an Activity. Starting needs `VIEW_CHANNEL`, `CONNECT` (voice) and `USE_EMBEDDED_ACTIVITIES` in the channel, a released, non-suspended application with an approved bundle, and no install. Instances may be bound to voice or text channels (one running per channel); in a voice channel participants follow the call (a voice session ending removes the participant, the last removal ends the instance); the Embedded App SDK gets voice-state events and `setActivity` through the Rich Presence path; URL mappings accept `POST`, `PUT`, `PATCH` and `DELETE` to a mapping's target only (body at most 1 MiB, nothing relayed either way but the body and `Content-Type`), while the shell stays `GET`/`HEAD`. (P14-DEV-3, P14-DEV-4, P14-DEV-6) | Default |
| D-249 | **Voice E2EE uses the MLS ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256` (DAVE's), and the built frame and relay design is the spec.** Identity key ECDSA P-256 in WebCrypto, non-extractable; per-session MLS signature keys certified by it. Frame format v1 as built: per-sender encryptor shared by all tracks, a random 4-byte instance id per encryptor mixed into the media key derivation, 8-byte truncated GCM tag checked through the CTR keystream, codec-aware clear ranges. Relay: op 101 upward, `E2EE_MLS` dispatch downward (sequenced, replayed on RESUME), `ready`/`execute` key switch with a 5-second fallback, ordering state per gateway node like voice states. Member-committed removals are temporary; the server as external sender remains the target once `openmls` is in. `voice.e2ee` and `VITE_SR_VOICE_E2EE` stay off until MLS lands, then default on. (P14-E2E-1, P14-E2E-3) | Default |
| D-250 | **Privacy codes follow DAVE: a stable pairwise verification code per participant and a per-call code.** The pairwise code is derived from the two identity public keys and user ids only (30 decimal digits in groups of five), persists across calls until an identity key changes, and backs the per-identity "verified" toggle; the call code is derived from the current MLS epoch authenticator and is compared live, never stored. Replaces the spec's per-epoch participant code. (P14-E2E-2) | Default |
| D-251 | **Lurk mode is closed: a non-member never reads a server's channels.** The Server Directory listing is the preview; LNG-GLD-029 becomes "won't build". (P14-LNG-6) | Default |

## Phase 14 and 15 remaining answers (2026-10-16)

Adopted from the "Phase 14 and 15 remaining questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is K-01 onward in [dev/answers-followups-p15.md](dev/answers-followups-p15.md); the round needs no migration. The JASSUB licence (P15-RST-2) is with the owner and has no row here yet.

| ID | Decision | Status |
|---|---|---|
| D-252 | **Activity approval lifecycle and serving gate (amends D-246 and D-247).** A new bundle upload is a draft and changes nothing served; submitting a bundle leaves the approved one serving until staff decide, and a rejection leaves it serving; an approval replaces the served hash and supersedes the old bundle; a change to the URL mappings withdraws the approval at once (the Activity is paused until a bundle is approved with the new mappings); a change to `dev_entry_url` withdraws nothing. The Activity is served outside development mode while the application has at least one released version and is not suspended (`in_review` with a prior release keeps serving); suspension stops serving at once. (P15-ACT-2, P15-ACT-3) | Default |
| D-253 | **System cards are closed by the server; the client keeps no answered state.** A card posted by the system user with buttons (staff offers, tester invitations, review-queue cards, AutoMod alerts, incident "Resume invites", future cards) is rewritten by the server when it is answered by any path (press, admin page, portal, expiry): buttons removed, the outcome written as a last embed field "Answer" (or a last line), through one helper that closes a card exactly once; clients get the ordinary `MESSAGE_UPDATE`; a press on a closed card answers "Already decided" and closes a stale card. The client renders what the message says and remembers nothing per page view. (P15-INT-1) | Default |
| D-254 | **Platform defaults are edited in Admin; secrets never are.** The default theme per section (ADM-PLT-004) is a platform setting in Admin → Appearance → Defaults through the feature registry (a member's own choice wins). Of ADM-PLT-005, secrets and their endpoints (`smtp.*`, `discord.client_secret`, `gif.api_key`, `lookup.*_api_key`) stay file-based operator config, shown only as configured-or-not with the test send; the non-secret policy (unfurl allow and deny hosts and per-host rate, GIF `country` and `locale`, which lookup providers are offered) is a database setting edited on the page, with the config key as the bootstrap value. The Setup Wizard checks and tests SMTP and Discord OAuth from the config and writes no secret. (P15-MSC-2, P15-RST-1) | Default |
| D-255 | **Reviewers edit a submission in place through the submission routes, never the publisher routes.** The draft metadata routes (`PUT /submissions/{id}/library/series`, `.../chapters/{id}`, the cover, and the other kinds' equivalents) accept the holder of the kind's review permission who holds the live claim on a submission in `in_review`, besides the submitter in `draft` or `changes_requested`; each reviewer edit posts a timeline event naming the fields, visible to the submitter; `PUBLISH_*` stays required for `/admin/...` writes to published content; credits stay the submitter's. (P15-RST-3) | Default |
| D-256 | **Patron status is a computed field of the public user object, and one toggle hides badge and ring.** `sr_patron: true` when the Patron entitlement is active or in grace (permanent counts) and `hide_badge` is off, else `false` (no third state); the toggle becomes "Show my Patron badge and ring"; the avatar ring is drawn from the flag wherever Discord draws one; the member object's `premium_since` stays `null` (no boosts). Flag changes dispatch `GUILD_MEMBER_UPDATE` to shared servers besides the user's own `SR_ENTITLEMENT_UPDATE`. (P15-RST-5) | Default |
| D-257 | **No `bitcoind` or `electrs` in CI (DON-PAY-009 becomes "won't build in CI").** The Esplora client is covered by `FakeChain` and the BIP-84 vectors in CI, and by an opt-in `#[ignore]` live test against a public signet Esplora (`SR_TEST_ESPLORA_URL`, a known funded watch-only signet address) run by hand before a release that touches `sr-payments`, in the style of the `s3_live` tests. (P15-RST-6) | Default |

## Wave 15 answers (2026-10-17)

Adopted from the "Wave 15 questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is K-15 onward in [dev/answers-followups-p15.md](dev/answers-followups-p15.md); the round's one migration is `1020`. The JASSUB licence (P15-RST-2) stays with the owner and has no row here.

| ID | Decision | Status |
|---|---|---|
| D-258 | **Developer-platform reviews keep their state on the versioned object; `sr-review` is the content pipeline.** Application versions, Activity bundles and package versions are reviewed through their own `review_state` (apps, Activities, `package_versions`), under their own permissions (`REVIEW_APPS`, `REVIEW_PACKAGES`), and share with the content pipeline the Review Queues page, the staff access log and the conduct of a review: nobody reviews their own, a rejection needs a note, every decision is logged with the decider, and a pending item is a `#review-queue` card closed per D-253. The `submissions` tables stay Cinema and Library only; the submissions spec's "one pipeline" sentence is corrected. (P15-DEV-1) | Default |
| D-259 | **Profile pages load no third-party frame or image directly.** Song blocks open their provider in a new tab after one confirmation; `frame-src` stays `'self'`. Gallery pictures given as URLs are rendered through the D-176 image proxy (`/cdn/external/{hmac}/{url_b64}`), never from the picture's host, with a placeholder when the proxy refuses; `img-src` stays `'self' data: blob:`. (P15-DEV-2) | Default |
| D-260 | **Profile media: a person may upload one background and up to 12 gallery pictures for their page.** Uploaded and validated like the banner (sniffed, re-encoded, metadata stripped, SVG refused, the banner's size and animation caps), stored under the account and served from `'self'`; the document refers to them by id and the validator refuses an id the person does not own; removing a picture from the document deletes the file; account deletion removes the set. Not Patron-gated; no age-restricted material on a profile; moderated like the avatar. (P15-DEV-3) | Default |
| D-261 | **XC-TEST-001's Garage helper and XC-CI-005 are won't build.** Per-test object storage is `storage.endpoint = "memory:"` next to `TestDb` and the test clock; Garage is covered by the live-S3 suite in `storage.yml`. Preview deployments are replaced by the `live` e2e job (real sr-server with voice, E2EE and Activities, driven by Playwright). (P15-RB-1) | Default |
| D-262 | **A Discord identity moves between accounts only by a staff action inside the Person chain.** `POST /admin/persons/{id}/identities/{identity}/move {to_account_id, reason}` for `MANAGE_ACCOUNTS` behind a fresh second factor: the target account must be (or is thereby linked) in the same Person chain, nothing moves when the Person or either account is suspended or banned, the audit row carries the reason, the old account gets a system DM (and email when verified), the old row keeps `moved_to_account_id`. Self-service stays unlink and relink on the same account (D-030 unchanged). (P15-RB-2) | Default |
| D-263 | **A client that cannot do E2EE is refused from a call when `voice.e2ee` is on; there is no downgrade.** Op 4 `VOICE_STATE_UPDATE` carries the SIDE ROOM field `sr_e2ee: true`; without it the gateway answers `SR_VOICE_ERROR` `e2ee_required` and issues no voice token, and a session that has not completed the op 101 `hello` within 10 s of `VOICE_SERVER_UPDATE` is dropped with the same code. Bots declare it through the SDK (spec section 5 item 6). Amends the voice spec's section 5 item 3 from a client-side error to a server rule. (P15-BRW-1) | Default |

## Phase 16 backend answers (2026-10-19)

Adopted from the "Phase 16 backend questions" section of [open-question-answers.md](open-question-answers.md) (question ids in parentheses). Implementation work is L-10 onward in [dev/answers-followups-p16.md](dev/answers-followups-p16.md) (L-01 to L-09 belong to the other Phase 16 round); the round needs no migration. D-264 and D-265 are taken by that round and are not here.

| ID | Decision | Status |
|---|---|---|
| D-266 | **An off-air channel plays the slate; `Tune` says so with `slate: true` and keeps `state: off_air`.** The spec's `off_air` filler policy ("colour bars and tone") is a viewer-facing choice, and a wall-clock-synced HLS loop is the built form of it: the player plays `master_url` whenever it is present, draws the existing "Off the air, up next" panel over the picture while `slate` is true, and shows the off-air card only when there is no `master_url` (no slate configured). `state` keeps its guide meaning (`current` and `offset_ms` stay `null` off air); the guide, agenda and presence count are unchanged. The slate's tone is mixed at -20 dBFS, not full scale. One instance-wide slate asset serves both the `slate` and the `off_air` policies until a styled "We'll be right back" card exists (artwork, the owner's). Amends the live TV spec's "no media; the client shows its own slate or colour bars". (P16-BE-1) | Default |
| D-267 | **One media-sequence domain per channel: the slate is numbered as a block in the chain, and the 2^32 offset goes.** RFC 8216 section 6.2.2 forbids a decreasing media sequence number and hls.js and Safari treat one as a new stream (re-alignment by PROGRAM-DATE-TIME at best, a stall or error at worst), so a drop from above 2^32 back to a wall-clock seed on the return to a programme is not acceptable even behind a discontinuity. The slate chains from the programme before it (first slate segment after the programme ends = last programme number + 1, discontinuity sequence + 1, both computed from the stored bases of that block, nothing stored for the slate) and a programme with no contiguous predecessor chains from the slate on air at its start (slate number + 1, discontinuity sequence + 1); the wall-clock seed is used only when a channel has neither. The window that spans a slate-programme boundary in either direction lists both sides with `#EXT-X-DISCONTINUITY`, a new `#EXT-X-MAP` and a `#EXT-X-PROGRAM-DATE-TIME`, exactly as a programme boundary does; a wrap of the loop inside the slate stays as built. Invariant, property-tested over random schedules: the media sequence number and the discontinuity sequence number of one channel's media playlist never decrease between two reloads, whatever the schedule does. The slate is encoded in the standard ladder with the standard segment length, so the master offered off air is the standard rung set and a client never changes rung set at the boundary. (P16-BE-2) | Default |
| D-268 | **`sr-sdk` keeps `voice-e2ee` and `voice-transport` on by default; a bot without voice opts out with `default-features = false`.** Confirms the second half of P15-DEV-5 now that the transport exists: by D-263 a bot cannot be in a call without MLS, so the default build must be able to join a call, and `default-features = false` is the ordinary Cargo idiom (tokio, reqwest). The two features stay separate (`voice-transport` without `voice-e2ee` is valid on an instance with `voice.e2ee` off); the README names the exact opt-out line and what it drops (openmls, str0m, p256); CI builds `sr-sdk` with `--no-default-features` and with each feature alone so the opt-out keeps compiling; the `Cargo.toml` comments stop saying "off by default". The TypeScript SDK is unchanged (P15-DEV-5). (P16-BE-3) | Default |
| D-269 | **The slate asset is produced on the host that runs the transcoder by `sr-cli tv slate`, which registers it as the platform setting `tv.slate_asset`.** Production needs ffmpeg for every transcode already, so the operator makes the slate there with one command: `sr-cli tv slate [--picture bars\|moving] [--seconds 12]` runs `sr_transcoder::slate::slate_source_args`, ingests the MP4 through the ordinary transcode job as a system-owned asset outside the Cinema catalogue (no title, no guide entry, not listable, not deletable from Admin while it is the slate), waits for the job, writes the asset id to `tv.slate_asset` and reports it; running it again replaces the asset and deletes the previous one. The server reads `tv.slate_asset` first and `SR_TV_SLATE_ASSET` as the bootstrap fallback (D-254 pattern); Admin, Cinema, Live TV shows the slate as configured or not with the command to run (no upload in Admin). `sr-cli seed` makes the slate when ffmpeg is on PATH and says why it skipped otherwise, like the library samples. Tests need no ffmpeg (synthetic assets, `memory:` storage); installing ffmpeg on a development machine is a download and stays the owner's call (human action 5 in [open-questions.md](open-questions.md)). A branded slate (`--picture <file>`) waits on artwork, the owner's. (P16-BE-4) | Default |
