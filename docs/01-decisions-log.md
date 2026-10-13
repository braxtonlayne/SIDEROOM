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
