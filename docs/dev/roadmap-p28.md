# ROADMAP P28: The Study Reframe, The Arcade Elevation & Web-of-Trust Hardening (Waves 41–48)

**Baseline Commit:** `57b91ca9` (Wave 40 merged, all 27 CI gates green)  
**Authoritative References:** `docs/01-decisions-log.md`, `docs/13-roadmap-and-phases.md`, `route_matrix.tsv`

Written by the owner on 2026-10-31 and adopted as given, then revised after the commander's review the same day (see "Changes after review"). The commander's notes follow; the owner's text after them is unchanged except where a note or the change list says so.

## Commander notes (2026-10-31)

- **Decision numbers.** D-386 was already taken (the wave-35b whole-server bridge proposal). The owner's six defaults are renumbered to the next free numbers with their content unchanged: owner D-386 → **D-387** (mid-call Activity launch without reload), D-387 → **D-388** (deterministic host migration), D-388 → **D-389** (Activities read scoped BYOS), D-389 → **D-390** (emulator ROMs and BIOS), D-390 → **D-391** (bridge held channels and the `discord-bridge` audit actor), D-391 → **D-392** (unactivated invite). The review added **D-393–D-398**. Section 0 below uses the new numbers.
- **Migrations.** Blocks of 40 from 2000, one per wave: 41 → 2000–2039, 42 → 2040–2079, 43 → 2080–2119, 44 → 2120–2159, 45 → 2160–2199, 46 → 2200–2239, 47 → 2240–2279, 48 → 2280–2319. Inside a wave: worktree A takes the first ten, B the next ten, C the next ten, the integrator the last ten (41-A 2000–2009, 41-B 2010–2019, 41-C 2020–2029, integrator 2030–2039). The highest on the baseline is 1890.
- **Downloads (approved by the owner 2026-10-31, D-398).** Waves 41–45 add no dependencies. Wave 48 adds `tauri` **2.12.5** (Apache-2.0 OR MIT), the matching `@tauri-apps/cli` 2.x and `tauri-plugin-global-shortcut` 2.x (same licence). Wave 47 adds `jolt-physics` **1.1.1** (MIT; the official WebAssembly build of Jolt Physics, replacing the earlier `@dimforge/rapier3d-compat` 0.21.0 at the owner's request, change 11). Any other new crate or package still needs the owner's yes before the wave starts.
- **D-385 and Guest Links.** Wave 43's `/play/<token>` is the first page a person without a login can load beyond the entrance. It extends D-385 deliberately: only the isolated Arcade runner bundle, checked by `check-entrance-isolation.mjs` with a new surface. The guest relay it needs in `sr-voice`/`sr-gateway` is why it is its own wave.

### Changes after review (2026-10-31)

1. **41-C rewritten.** Zero-knowledge key wrapping dropped (per-browser P-256 identity keys, no persistent guild-role key, raw credentials would reach grantees, an encrypted catalog defeats per-object delisting). Replaced by server-side shelf access with per-object delisting (D-393, D-394).
2. **42-B guest links moved** to their own wave (43) after the sandbox work; the old 42-C is now 42-B.
3. **43 → 44, rewritten** as a graduated moderation toolset. The Tactical Nuke (automatic subtree purge, inviter sterilization, vouch fallout, platform-wide tree broadcast) is gone; a branch action remains for the Operator only, behind safeguards (D-395).
4. **46 split**: party game (47) and WebGPU starter + `Land.io` (48). Receipt Roulette cut (D-396). Guest rule set (D-397): guests get only what their game session produces; wave 43 designs the lobby sub-room as media + data with per-lobby keys so in-game voice can be added later, and reserves `ingame_voice`.
4a. **Developer freedom by default (D-399)** added as a global rule: new surfaces are manifest capability flags (`guest_play`, `lobby_channel`, `ingame_voice`, `byos_read`, `external_launch`, `asset_cache`), not one-off features.
5. **`TIER_3_FAN_IP` dropped**: the manifest has two tiers; fan-IP games are not accepted on any surface.
6. **41-A hides rather than deletes** storefront surfaces; the editorial look is a theme preset.
7. **45-A (crash telemetry)**: allow-list PII strip; an agent proposes, a human merges.
8. **46 (Tauri)**: `Esc` cannot be overridden (re-acquire on click); the install-folder check is a hint; "Minecraft" stays out of SIDE ROOM product names.
9. Wave numbers, migration ranges, route-matrix actors and gate proofs updated to match.
10. **Reorder (owner, 2026-11-01): the Tauri desktop shell goes last.** Old 47 (Sketch & Slander) is now **46**, old 48 (WebGPU starter + `Land.io`) is now **47**, old 46 (Tauri desktop shell + launcher) is now **48**. Migration blocks follow the new numbers (46 → 2200–2239, 47 → 2240–2279, 48 → 2280–2319) and the approved downloads move with their work (the physics engine in 47, `tauri` in 48). Items 4 and 8 above keep the numbers they had at review.
11. **Physics engine: Jolt (owner, 2026-11-01).** Wave 47 uses [Jolt Physics](https://github.com/jrouwe/JoltPhysics) through its official WebAssembly build, `jolt-physics` 1.1.1 (MIT), instead of `@dimforge/rapier3d-compat`. Use the single-threaded `wasm-compat` flavour: the multithreaded flavours need `SharedArrayBuffer` and cross-origin isolation, which the sandboxed Activity iframe does not have. The npm build is compiled without Jolt's `CROSS_PLATFORM_DETERMINISTIC` option, so the wave 47 gate proof (two clients, same inputs, same physics state hash) is what proves determinism across Chromium, Firefox and WebKit; if it fails, the agent stops and reports rather than switching engines (the fix would be a self-built Jolt WASM with that option, which needs the Emscripten toolchain and the owner's yes).

## 0. Global Invariants & Rules for All Agents (READ FIRST)
1. **Do NOT break the Immutable Rule (`P28-OPS-1`):** Mandatory MLS E2EE on `sr-voice`, the cross-tenant authorization matrix (`route_matrix.tsv`), and the hash-chained staff audit/access logs must remain green across all feature-switch permutations. Every new route added in Waves 41–48 **must** have a corresponding row in `route_matrix.tsv` and pass the 10-actor read/write matrix.
2. **Preserve Existing Working Plumbing:** Do not delete the underlying video player, HLS/remux pipeline (`ffmpeg.wasm`), EPUB/CBZ readers, or `str0m` data-channel relays. We are reframing the surface, stripping external metadata/storefront liability, elevating the Arcade, and adding cryptographic/social trust layers.
3. **Resolve the Open Checklist Defaults (`docs/dev/owner-checklist.md`):**
   - **D-387:** Mid-call Activity launch works **without** reloading the voice/UI context.
   - **D-388:** Deterministic Activity host migration: when the host leaves, authority transfers automatically to the longest-connected member in the session.
   - **D-389:** Activities may read assets/files from scoped BYOS buckets (read-only presigned/wrapped access) inside the sandbox.
   - **D-390:** Emulator ships **zero** ROMs (except a single CC0/MIT self-test binary for CI); PS1 core allows the built-in open-source HLE BIOS fallback when no user BIOS is mounted; SNES/Genesis remain strictly gated to GPL/MPL cores only.
   - **D-391:** Discord Bridge hides held channels by default and logs all synced mutations under a dedicated `discord-bridge` audit actor.
   - **D-392:** An "unactivated invite" is defined as an account with `0` messages and `0` voice seconds 7 days post-registration.
4. **Decisions from the review (owner, 2026-10-31):**
   - **D-393:** Shelf security is **server-side access control with per-object delisting**, not zero-knowledge wrapping. Per-object DMCA removal matters more than an encrypted catalog. What the server can and cannot see is stated in 41-C.
   - **D-394:** Shelf grants live on the server and follow the account: a friend keeps access across browsers and devices with nothing to re-wrap.
   - **D-395:** Moderation is a graduated toolset. The one branch-wide action is **Operator-only**, needs a second staff approver and a 2FA step-up, carries appeal and a 12-month expiry, and its public notice is anonymised (category and counts only).
   - **D-396:** **Receipt Roulette ("Who Said This?") is cut.** No game uses members' real messages or the Scrapbook. The party suite ships Sketch & Slander only.
   - **D-397:** **Guests see and hear only what their own game session produces**: state, scoreboard, and (later) optional in-game voice scoped to that `lobby_token`. Nothing from the server, channel or call: no messages, member lists, profiles, shelves, or the voice channel's MLS audio. In-game voice is a later capability with its own design; wave 43 must leave room for it. Every guest-facing game follows this (extends `P28-ARC-1`).
   - **D-398:** Downloads approved: `tauri` 2.12.5 (wave 48) and `jolt-physics` 1.1.1 (wave 47; replaces `@dimforge/rapier3d-compat`, change 11), as listed in the commander notes.
   - **D-399:** **Developer freedom by default.** The Arcade/Activity SDK lets game and app developers build most things (in-game voice and video, custom networking patterns, persistent saves to their own BYOS, leaderboards, mod loading, and so on) through **capability flags declared in the manifest and enforced by the platform**. The only limits are the security rules: the Immutable Rule, guests isolated to their game session (D-397), D-385, and no access to server data a member has not granted.
5. **Build capabilities, not features (D-399):** every new surface in these waves is designed as a general capability with a reserved manifest flag, not a single-purpose feature, so new kinds of games need no platform rework. Where a wave builds something narrow, its text names the general capability it should expose, and the manifest validator reserves the flag even if the capability ships later.

---

## Wave 41: "The Study" Reframe & Shelf Access Control
**Goal:** Replace the streaming-service storefront framing (*Cinema* + *Library*) with **The Study (The Shelves)**—a source-first, BYOS file and media space with zero external metadata scraping and server-side, per-object-delistable shelf access. Unify the Side Room editorial aesthetic (charcoal, cream, muted gold, serif display type) across The Lounge.

### Worktree 41-A: Navigation & Storefront Pruning (`web`, `sr-server`)
- **Top-Level Room Trio:** Restructure the primary navigation rail after The Entrance into three core rooms:
  1. **The Lounge** (Chat, E2EE Voice/Video, Screening Room, Built-in Modules)
  2. **The Arcade** (Instant-Play Games, Guest Links, Emulator, Lobbies)
  3. **The Study** (BYOS Shelves, Readers, Media & File Browser)
- **Hide Storefront & Inducement Surfaces (hide, do not delete):**
  - Switch off TMDB and external metadata scrapers, `Movies / Series / Anime` storefront categories, "Global Trending" rails, the global "Requests" board, and staff upload/ingest review queues. Lock the corresponding switches in the 73-switch registry to `off` by default and remove their public UI surfaces (routes return 404 while off).
  - The code, tables and switch rows stay; nothing is dropped from the schema. CI proves the surfaces are unreachable with the switches off, not that the code is gone.
- **Editorial Theme Preset:** Add the Side Room editorial token palette (warm charcoal surfaces, cream text, muted gold accents, serif headers) as a **theme preset** and make it the default for **The Lounge** shell so transitioning between Lounge, Arcade, and Study feels like moving between rooms of the same house. Existing themes remain selectable.

### Worktree 41-B: Source-First File & Shelf UX (`web`, `sr-media`)
- **Sidebar by Source (Not Genre):**
  - Organize **The Study** sidebar into: `My Files` (personal mount), `Friends' Shelves` (one entry per friend sharing with you), and `Guild Shelves` (one entry per guild where your role holds view grants). Entries come from the server-side grant table of 41-C (`GET /api/v1/shelves/visible`).
  - Every card and row **must** display a clear provenance badge (`@handle's bucket` or `Guild: <Name>`).
- **Local-Only Rich Metadata & Type Filters:**
  - Filter strictly by file type/extension: `Video`, `Audio`, `Reading (EPUB/CBZ/PDF)`, `Game & Mod Packs`, `Images`, and `Other`.
  - Extract covers and thumbnails **strictly from the file itself** client-side: ID3 art via `lofty` (WASM), page 1 of `.cbz`/`.epub`, and client-side `<video>` frame capture.
  - Allow the bucket owner to manually edit a custom title, cover image, or note stored in their own shelf index.
  - Keep personal local rails only: `Continue Watching/Reading`, `Recently Added to Shelves You Can See`, and `Pinned Folders`.
- **The Screening Room:** Move synced Watch Together video playback and scheduled Guild Live TV guides cleanly under the **Voice Channel / Screening Room** surface as a social activity over shared shelves rather than a global broadcast storefront.
- Object bodies are fetched with the short-lived per-object URLs of 41-C; the client never holds bucket credentials for a shelf it does not own.

### Worktree 41-C: Shelf Access Control & Per-Object Delisting (`sr-storage`, `sr-server`, `web`) — rewritten after review
- **Credentials stay with the server, never with grantees (`P28-SHELF-1`):**
  - When a user mounts an S3/R2 bucket, `sr-server` stores the read credentials envelope-encrypted at rest with a key held in `sr-storage` secrets (never in the database row). The mount UI tells the owner to supply a **read-only, prefix-scoped** key and links the IAM/R2 policy snippet; the mount check refuses keys that pass a `PutObject` dry-run.
  - Grantees read an object through `GET /api/v1/shelves/:shelf_id/objects/:object_hash/url`, which checks the grant and the delist flag and returns a **presigned GET URL scoped to that one object, valid 10 minutes**. No bucket key, prefix listing or long-lived URL ever reaches a grantee. Revoking a grant takes effect at the next URL mint.
- **Grants are server-side and device-independent (`P28-SHELF-2`, D-394):**
  - `shelf_grants(shelf_id, grantee_kind friend|guild_role, grantee_id, granted_by, created_at, revoked_at)`. A grant may be created only by the shelf owner (friend grants) or by the owner for a guild role they hold admin on. Every create/revoke is written to the hash-chained audit log.
  - The owner's shelf settings show the live grant list ("who can see this shelf") with the audit trail; the ghost-user protection is auditability and ownership checks in the route matrix, not cryptography.
- **Catalog is server-readable so delisting is per object (`P28-LEG-1`, D-393):**
  - `shelf_objects(shelf_id, object_hash, path, size, kind, owner_title, owner_cover, owner_note, delisted_at, delist_strike_id)`. The owner's client builds the index from the bucket listing and uploads it in clear.
  - Staff takedowns operate on `object_hash` (and, for whole-shelf cases, `shelf_id`/`bucket_id`): set `delisted_at`, log the strike, and the object disappears from every catalog response and can no longer be minted a URL. The owner is notified with the strike reference; the existing counter-notice route applies.
- **What this protects and against whom (say it plainly in the docs):**
  - Against other members: no grant, no listing, no URL.
  - Against grantees: they never hold credentials; a leaked URL is one object for 10 minutes.
  - Against the server, the Operator, or an attacker who gets `sr-server`: **not protected.** They can read credentials and catalogs. Mitigation is the read-only, prefix-scoped key the owner supplies. Object bodies do not pass through `sr-server` (presigned direct reads), so they are not on the server unless the owner enables the proxy fallback for buckets without CORS.
- Route-matrix rows: `/api/v1/shelves/*` (owner, friend-grantee, guild-role-grantee, staff), `/api/v1/shelves/:id/objects/:hash/url`, `/api/v1/admin/shelves/delist`.
- **General capability (D-399):** the same URL-mint path is the `byos_read` manifest capability for Activities (D-389): a game declares which shelf scope it reads, and the sandbox CSP allow-lists exactly those presigned hosts. A `byos_write` flag is reserved (persistent saves to the developer's own bucket, later design); the validator accepts it, the runner refuses it until it ships.

**Wave 41 Gate Proof:** CI proves zero outbound requests to TMDB/external metadata providers, that the storefront routes 404 with their switches off, that a grantee's session never contains a bucket credential (response body and local storage scan), that a delisted `object_hash` returns 404 on URL mint and is absent from catalog responses for every actor, and passes full E2E shelf browsing + playback through presigned per-object URLs from a second browser profile for the same friend account.

---

## Wave 42: The Arcade Elevation, Hardened Sandbox & Two-Tier Trust
**Goal:** Elevate **The Arcade** to a primary room, harden the game iframe sandbox, and implement the Two-Tier Verification & Build-Hash Vouch system. Guest links follow in wave 43 on top of this sandbox.

### Worktree 42-A: The Arcade Room & 2-Tier Content Policy (`web`, `sr-server`)
- **The Arcade Surface:**
  - **Featured / Front Page:** Strictly curated by the Operator. Displays Operator-built games (`Land.io` roguelike, Hold'em, Tabletop, Emulator, plus upcoming flagship games) and Operator-signed community games.
  - **Guild Cartridges (Unlisted):** Browser for unlisted games/mods hosted on Friends' or Guilds' BYOS buckets.
  - **Coming Soon Showcase:** Curated showcase of compliant in-development originals and clean-room reimplementations with repo links and "Notify Me" toggles.
- **Mandatory 2-Tier Self-Declaration on Game Manifests:**
  1. `TIER_1_ORIGINAL`: 100% original code, art, audio, and IP. Eligible for Browser play, Guest Links, and Featured Front Page.
  2. `TIER_2_REIMPLEMENTATION`: Clean-room engine code with **zero** bundled proprietary assets (e.g., *Horizon*, *OpenMW*). Desktop/Tauri only; requires the local base-game hint of 48-B. Eligible for Featured Page.
  - Games using external IP names, characters or assets are not accepted on any surface, including unlisted Guild Cartridges; the manifest validator rejects a third value.
- **Zero-Cut Creator Tip Badges:** Allow game manifests and `/~handle` profiles to attach optional peer-to-peer donation links (BTC/Lightning QR, Ko-fi, Patreon) with 0% platform intermediation.

### Worktree 42-B: Hardened Sandbox, Swarm Code-Signing & Hash Vouches (`web`, `sr-cli`, `sr-server`)
- **Separate-Origin Sandboxed Iframe:**
  - All Arcade games run inside an isolated `<iframe sandbox="allow-scripts allow-pointer-lock">` served from a cookieless sandbox origin with a strict CSP (`connect-src 'none'` or scoped strictly to presigned R2 asset URLs).
  - All networking and identity calls pass strictly through the parent window's validated `postMessage` Activity SDK bridge.
- **Split Verification (`[SWARM-SAFETY-SIGNED]` vs. `[OPERATOR-FEATURED]`):**
  - Add `sr-cli arcade sign-code --hash <code_sha256>`: signs **only** the executable code bundle (`.wasm`, `.js`) after automated static/sandbox audit, without ever downloading or inspecting the developer's external S3 asset bucket.
- **Hash-Pinned Community Vouches:**
  - Users in good standing can click **"Vouch for Build"** on any game, cryptographically pinned to `code_sha256`.
  - If the developer updates the game and `code_sha256` changes, the UI badge automatically flips to `⚠️ UNVERIFIED UPDATE (N users vouched for previous build vX; 0 vouches on current hash)`.

**Wave 42 Gate Proof:** Playwright launches `Land.io` inside a voice call from the sandbox origin, proves the iframe cannot reach any non-allow-listed origin, verifies the signed-code badge and the vouch flip on a hash change, and the manifest validator rejects a third tier.

---

## Wave 43: Shareable `/play/<token>` Guest Links
**Goal:** Instant unauthenticated `/play/<token>` Guest Links, scoped strictly to one lobby's game state, delivered over a new unkeyed guest relay. This is the second D-385 surface.

### Worktree 43-A: Guest Relay (`sr-voice`, `sr-gateway`)
- Add a **per-lobby sub-room** on `str0m` keyed only by `lobby_token`: a **general media + data scope** (not data-only), with **its own per-lobby keys**, no MLS group membership and no access to the room's keyed media or chat paths. Lobby tokens are random, expire with the lobby, and are revocable by the lobby host.
- `sr-gateway` gives a guest socket exactly one subscription: `game:<lobby_token>`. Every other topic returns `403/Drop`. The isolation rule is **enforced at the relay**, not in the SDK.
- **Design requirement (owner):** a game developer must be able to **add in-game voice later without major rework** (proximity chat inside the game, carried on this sub-room with its keys, never the voice channel's MLS audio or anyone's call outside the game). This wave does not build the voice path; it reserves the manifest capability flag `ingame_voice` (validator accepts it, runner refuses it until the capability ships) and leaves the media leg of the sub-room designed and documented, with its own design note before it is built.

### Worktree 43-B: Link Generation & Guest Runner (`web`)
- **Capability Link Generation:** Any member inside an active Arcade lobby can generate a `noindex` shareable link (`/play/<lobby_token>#key=<ephemeral_game_key>`). Mobile QR rendering so phones can act as controllers or drawing pads.
- **Strict Guest Isolation (Extending `D-385`):** Unauthenticated guests opening `/play/<token>` load **only** the minimal isolated Arcade runner bundle, checked by `check-entrance-isolation.mjs` with a new `guest-play` surface. Guests are assigned a randomized callsign (or lobby-approved nickname).
- **Guest-Link Security Rule (`P28-ARC-1`, extended by D-397):** A guest **sees and hears only what their own game session produces**: state, scoreboard, and (once the capability exists) in-game voice scoped to that `lobby_token`. Nothing from the server, channel or call: never the voice channel's MLS audio/video keys, hearing or speaking in VC, messages of any kind, member lists, `/~handle` profiles, shelves, lobby lists or other lobbies. The Activity SDK bridge exposes a reduced `guest` capability set that lacks these calls, so a game cannot leak them even if it tries. Every guest-facing game (Sketch & Slander in 46, `Land.io` in 47) is built against this reduced set and tested for it.
- **General capability (D-399):** the guest link is the `guest_play` manifest capability; the lobby sub-room is the `lobby_channel` capability (data now, media later via `ingame_voice`). Both are flags a developer declares, not features wired to one game.

**Wave 43 Gate Proof:** Playwright E2E test launches `Land.io` inside a voice call, opens the generated `/play/<token>` link in an incognito browser context, verifies bidirectional gameplay sync between the member and the guest, proves the guest receives `403/Drop` on the MLS voice stream, REST chat, member, profile and shelf routes **at the relay** (a guest socket that forges a second subscription is dropped before the SDK sees it), that a manifest declaring `ingame_voice` validates but is refused by the runner, and that the guest bundle contains no non-Arcade module (isolation check).

---

## Wave 44: Graduated Moderation & The "Heavy Crown" Vouch System
**Goal:** Turn the invite graph and `/~handle` profiles into a self-policing Web of Trust with proportionate, reviewable tools rather than one blast-radius button. Keeps T27, D-346 and doc 09 (the invite graph is kept; GDPR erasure applies to profile data as elsewhere). Replaces the Tactical Nuke (D-395).

### Worktree 44-A: Graduated Moderation Toolset & Subtree Cases (`sr-server`, `sr-cli`, `web`)
- **Per-member actions (staff):** warn, timeout, **invite freeze** (`invite_frozen_until`, chosen duration), **revoke unused invite codes**, ban with a required reason category. All existing appeal routes apply.
- **Subtree case on a serious ban:** when a ban carries a serious category (spam ring, fraud, ban evasion, CSAM/illegal content), the system opens a `moderation_case` on the target's `invited_by` subtree and shows staff each node with activity stats and lineage.
  - **Automatic:** only descendants that are **unactivated under D-392** are frozen (cannot post, invite or join voice; can log in and appeal). A frozen unactivated account is removed after 30 days unless a staff member clears it.
  - **Manual, per node:** activated descendants, the direct inviter and the inviter's inviter are listed for review with no automatic penalty. Staff clear, freeze or ban each one on its own evidence.
- **Inviter sterilization (`can_invite = false`, overrides Patron/server perks):** only inside a case, with a written reason, a **second staff member's approval** and a **2FA step-up** by both. It **expires after 12 months** (renewable only by a new case) and is appealable. No "permanent lineage mark"; the case reference is in the audit log, not on the profile.
- **Operator-only branch action (`POST /api/v1/admin/moderation/branch-action`, `sr-cli moderation branch-action`, D-395):** applies one chosen action (freeze or ban) to every node of a case subtree at once. Route-matrix actor `operator` only; a second staff approver and 2FA step-up for both; mandatory reason; every affected account gets its own appeal; freezes and sterilizations from it expire at 12 months. Hash-chained audit entry lists the case, the approver and the node count.
- **Anonymised notice:** a serious ban or branch action emits a collapsible MOTD showing **the offense category and counts only** (`N accounts actioned, M frozen pending review`), never handles, tree shape or inviter identity. A monthly transparency page aggregates these counts.
- **Drill:** `sr-cli moderation drill --synthetic` builds an isolated dummy invite tree and runs a case through to the notice **inside the Staff Server only**, to verify the pipeline. Nothing is broadcast platform-wide.

### Worktree 44-B: "Heavy Crown" Profile Vouches (`sr-server`, `web`)
- **Profile Vouches:**
  - Any non-sterilized member can vouch for another member on their `/~handle` profile (1 vouch per pair, revocable).
  - When a vouched member is banned for a serious category, their vouchers get a private notice and the vouch is removed. **No freeze or penalty for vouchers** (vouch fallout dropped).
- **Reverse-Privilege Accountability Multiplier:**
  - Codify vouch tiers in the moderation engine (`0–49: Citizen`, `50–499: Pillar`, `500+: Elder/High-Trust`).
  - Automated moderation timeouts, holds, and strike weights scale **upward** (`1.5x` for Pillar, `3.0x` for Elder) when a high-vouch user commits a rule violation, visibly documented on the Vouch tooltip so users never farm vouches casually.

**Wave 44 Gate Proof:** Integration test builds a 4-generation invite tree (with one D-392 unactivated node in Gen 4) + 3 profile vouchers, issues a serious ban on Gen 3, and asserts: a case opens, only the unactivated Gen 4 node is frozen, Gen 2 and Gen 1 are untouched and still pass `POST /invites`, vouchers are notified with no freeze, the MOTD carries category and counts only; sterilization of Gen 2 fails without a second approver and 2FA and succeeds with them, with `invite_frozen_until`/sterilization expiry at +12 months; `branch-action` is refused for every actor except `operator` in the 10-actor matrix.

---

## Wave 45: Self-Healing Crash Telemetry & Showroom Preview Mode
**Goal:** Wire up a privacy-preserving client crash ring-buffer for agent-assisted hotpatching, and build a one-click "Showroom / Populated Preview Mode" so early testers on the test box immediately experience a lived-in platform.

### Worktree 45-A: Deterministic Crash Ring-Buffer (`web`, `sr-server`)
- **Client-Side Ring Buffer:**
  - Maintain an in-memory rolling buffer in the browser/WASM client recording the last 60 deterministic Activity/Game ticks, WebGPU/WASM panic stack traces, route state, and browser capability flags.
  - **Allow-list Privacy Strip:** the crash card is built from an explicit allow-list of fields (commit SHA, stack frames with file/line, tick inputs as opaque enums, route name, capability flags). Anything not on the list is dropped, so message bodies, E2EE keys, S3 URLs, tokens and IPs can never appear by omission. CI greps the payload against a secret-pattern list.
- **Staff Server Triage Feed:**
  - On unhandled exception or WASM panic, post a deduplicated, structured JSON crash card into a dedicated `#crash-triage` channel in the Staff Server containing the commit SHA, stack trace, and replay buffer so a Claude worktree agent can reproduce the bug and open a fix PR. **An agent proposes; a human merges.** Nothing from `#crash-triage` is deployed without a staff review.

### Worktree 45-B: Showroom / Populated Preview Mode (`web`)
- **`[ 👁️ Preview Populated State ]` Client Toggle:**
  - Add a non-destructive toggle in the top bar (enabled for beta/test deployments via feature switch) that overlays rich, interactive fixture data onto the UI:
    - 10 populated Guilds in the rail with active voice statuses and unread indicators.
    - Multi-friend and multi-guild Shelves in **The Study** demonstrating source filtering and local metadata views.
    - A populated **Arcade** grid distinguishing clearly between **`[ PLAYABLE NOW ]`** live builds and **`[ PREVIEW / IN DEVELOPMENT ]`** showcase cards.

**Wave 45 Gate Proof:** Inject a synthetic WASM panic in a Playwright test, verify the allow-listed crash payload arrives in `#crash-triage` with zero sensitive tokens, and verify Showroom Mode toggles cleanly without mutating backend state.

---

## Wave 46: Arcade Launch Suite — "Sketch & Slander" Party Game
**Goal:** Ship the first post-pivot social party game, playable from the voice channel and from phones through wave 43 guest links.

### Worktree 46-A: "Sketch & Slander" (`web`, `sr-server`)
- Drawing and prompt-guessing game on the main VC canvas with phones as drawing pads via **QR Code `/play/<token>` Guest Links**. Custom, unhinged Guild Prompt Packs are loaded from the Guild's BYOS bucket through the 41-C per-object URLs (D-389).
- **No member data in the game (D-396):** prompts come from packs and the built-in set only. The game never reads chat history, the Scrapbook or profiles. Receipt Roulette is cut and is not to be rebuilt under another name.
- **Guests (D-397):** guests draw, guess and see the scoreboard. They see no member handles beyond lobby nicknames, no messages, no guild name beyond the lobby title the host chose. Built against the reduced `guest` SDK capability set of 43-B.

**Wave 46 Gate Proof:** Playwright runs a round with one member and one guest phone context; the guest's bundle and network trace contain no chat, member, profile or shelf route; the prompt pack loads only through a presigned per-object URL.

---

## Wave 47: WebGPU 3D Arena Starter & `Land.io` Roguelike Polish
**Goal:** Finish `Land.io` for guests and ship a reusable WebGPU 3D multiplayer starter template for the flagship.

### Worktree 47-A: `Land.io` Roguelike Polish (`web`)
- Finalize `Land.io` roguelike card-drafting mode with full `/play/<token>` guest link support under D-397, bot backfilling (`Lumen bot`, `Tofu bot`), and deterministic host migration (D-388).

### Worktree 47-B: WebGPU 3D Arena Starter (`packages/activity-starter-webgpu-3d`)
- Create a turn-key 3D multiplayer starter pack: WebGPU + Jolt Physics (`jolt-physics` 1.1.1, `wasm-compat` flavour, lazy-loaded) + client-side prediction + S3 `.glb`/`.ktx2` streaming (per-object URLs) + E2EE spatial voice coordinate hooks, ready for building the flagship low-poly FPS / Magnum Opus. Ships with the 45-A deterministic tick recorder wired in and a manifest that declares `lobby_channel`, `guest_play` and `byos_read`, with `ingame_voice` and `byos_write` shown commented out, so it doubles as the reference for D-399.

**Wave 47 Gate Proof:** The starter builds and runs a two-client deterministic replay in CI (same inputs, same physics state hash); `Land.io` guest E2E from wave 43 passes unchanged against the finished build.

---

## Wave 48: Tauri Desktop Shell & Clean-Room Engine Launcher
**Goal:** Wrap the web client in a lightweight **Tauri v2** desktop shell (`sr-desktop`) for OS-level gaming ergonomics and local-only detection/launching of Tier 2 clean-room reimplementations (e.g., *Horizon*).

### Worktree 48-A: Tauri v2 Wrapper & Gaming Ergonomics (`sr-desktop`)
- Package the React/Vite + WASM frontend inside a minimal Rust Tauri v2 binary (<15MB), `tauri` 2.12.5.
- Enable **Global OS Push-to-Talk** and global mute keybinds when the window is unfocused (`tauri-plugin-global-shortcut`).
- **Pointer lock and `Esc`:** the OS and webview release pointer lock on `Esc` and that cannot be overridden. The in-game menu opens on the lock-release event and the runner **re-acquires pointer lock on the next click** inside the game canvas, so leaving the menu does not need a second gesture.
- Provide a dedicated local SSD cache directory for multi-gigabyte BYOS game asset packs, exposed to games as the `asset_cache` manifest capability (D-399).

### Worktree 48-B: Zero-Telemetry Local Reimplementation Launcher (`sr-desktop`, `web`)
- **Local base-game hint (not an ownership check):**
  - For `TIER_2_REIMPLEMENTATION` Arcade entries, the manifest names a base-game folder to look for (e.g., *Horizon* declares `.minecraft/versions/26.3`). `sr-desktop` checks that scoped local path strictly on the user's machine. A folder is a **hint** that the base game is probably installed; it proves nothing about ownership and the UI must not call it a check.
  - **Privacy Invariant (`P28-DESK-1`):** The result of the local filesystem lookup **never** leaves the client machine over the network. If found, render a local badge: *"Recommended locally because <base game from manifest> looks installed on this PC."* The base-game name comes from the manifest; SIDE ROOM product, feature and package names never contain "Minecraft" or another third-party trademark.
- **Process Handoff + Social Wrapper:**
  - Trigger the external engine's official launcher/binary with direct-connect lobby CLI flags while keeping the player connected to SIDE ROOM's E2EE voice channel and updating their local lobby status. This is the `external_launch` manifest capability (D-399): any Tier 2 manifest can declare its binary, hint folder and CLI flag template; nothing here is specific to one engine.

