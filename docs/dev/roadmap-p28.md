# ROADMAP P28: The Study Reframe, The Arcade Elevation & Web-of-Trust Hardening (Waves 41–46)

**Baseline Commit:** `57b91ca9` (Wave 40 merged, all 27 CI gates green)  
**Authoritative References:** `docs/01-decisions-log.md`, `docs/13-roadmap-and-phases.md`, `route_matrix.tsv`

Written by the owner on 2026-10-31 and adopted as given. The commander's notes follow; the owner's text after them is unchanged except where a note says so.

## Commander notes (2026-10-31)

- **Decision numbers.** D-386 was already taken (the wave-35b whole-server bridge proposal). The owner's six defaults are renumbered to the next free numbers with their content unchanged: owner D-386 → **D-387** (mid-call Activity launch without reload), D-387 → **D-388** (deterministic host migration), D-388 → **D-389** (Activities read scoped BYOS), D-389 → **D-390** (emulator ROMs and BIOS), D-390 → **D-391** (bridge held channels and the `discord-bridge` audit actor), D-391 → **D-392** (unactivated invite). Section 0 below uses the new numbers.
- **Migrations.** Wave 41 uses 2000–2039 (41-A 2000–2009, 41-B 2010–2019, 41-C 2020–2029, integrator 2030–2039); later waves take 2040 onward in blocks of 40. The highest on the baseline is 1890.
- **Downloads.** Wave 45 (Tauri v2) and any WebGPU physics library of wave 46 are new dependencies: the owner is asked before those waves start. Waves 41–44 add none.
- **Wave 43 needs a design note before it is built.** The Tactical Nuke purges a whole invite subtree on one decision, sterilizes inviters who did nothing themselves, and broadcasts the dependency tree platform-wide. The commander writes a short note for the owner on what the public notice shows (handles or anonymised nodes), what "purged" does to the descendants' data (doc 09 keeps the invite graph forever; GDPR erasure still applies), and appeal, before agents start.
- **D-385 and Guest Links.** Wave 42's `/play/<token>` is the first page a person without a login can load beyond the entrance. It extends D-385 deliberately: only the isolated Arcade runner bundle, checked by `check-entrance-isolation.mjs` with a new surface.

## 0. Global Invariants & Rules for All Agents (READ FIRST)
1. **Do NOT break the Immutable Rule (`P28-OPS-1`):** Mandatory MLS E2EE on `sr-voice`, the cross-tenant authorization matrix (`route_matrix.tsv`), and the hash-chained staff audit/access logs must remain green across all feature-switch permutations. Every new route added in Waves 41–46 **must** have a corresponding row in `route_matrix.tsv` and pass the 10-actor read/write matrix.
2. **Preserve Existing Working Plumbing:** Do not delete the underlying video player, HLS/remux pipeline (`ffmpeg.wasm`), EPUB/CBZ readers, or `str0m` data-channel relays. We are reframing the surface, stripping external metadata/storefront liability, elevating the Arcade, and adding cryptographic/social trust layers.
3. **Resolve the Open Checklist Defaults (`docs/dev/owner-checklist.md`):**
   - **D-387:** Mid-call Activity launch works **without** reloading the voice/UI context.
   - **D-388:** Deterministic Activity host migration: when the host leaves, authority transfers automatically to the longest-connected member in the session.
   - **D-389:** Activities may read assets/files from scoped BYOS buckets (read-only presigned/wrapped access) inside the sandbox.
   - **D-390:** Emulator ships **zero** ROMs (except a single CC0/MIT self-test binary for CI); PS1 core allows the built-in open-source HLE BIOS fallback when no user BIOS is mounted; SNES/Genesis remain strictly gated to GPL/MPL cores only.
   - **D-391:** Discord Bridge hides held channels by default and logs all synced mutations under a dedicated `discord-bridge` audit actor.
   - **D-392:** An "unactivated invite" is defined as an account with `0` messages and `0` voice seconds 7 days post-registration.

---

## Wave 41: "The Study" Reframe & Zero-Knowledge Shelves
**Goal:** Replace the streaming-service storefront framing (*Cinema* + *Library*) with **The Study (The Shelves)**—a source-first, BYOS file and media space with zero external metadata scraping and optional client-side zero-knowledge key wrapping. Unify the Side Room editorial aesthetic (charcoal, cream, muted gold, serif display type) across The Lounge.

### Worktree 41-A: Navigation & Storefront Pruning (`web`, `sr-server`)
- **Top-Level Room Trio:** Restructure the primary navigation rail after The Entrance into three core rooms:
  1. **The Lounge** (Chat, E2EE Voice/Video, Screening Room, Built-in Modules)
  2. **The Arcade** (Instant-Play Games, Guest Links, Emulator, Lobbies)
  3. **The Study** (BYOS Shelves, Readers, Media & File Browser)
- **Kill Storefront & Inducement Surfaces:**
  - Remove/disable TMDB and external metadata scrapers, `Movies / Series / Anime` storefront categories, "Global Trending" rails, the global "Requests" board, and staff upload/ingest review queues.
  - Lock the corresponding switches in the 73-switch registry to `off` by default and remove their public UI surfaces.
- **Editorial Theme Unification:** Apply the Side Room editorial token palette (warm charcoal surfaces, cream text, muted gold accents, serif headers) to **The Lounge** shell so transitioning between Lounge, Arcade, and Study feels like moving between rooms of the same house.

### Worktree 41-B: Source-First File & Shelf UX (`web`, `sr-media`)
- **Sidebar by Source (Not Genre):**
  - Organize **The Study** sidebar into: `My Files` (personal mount), `Friends' Shelves` (one entry per friend sharing with you), and `Guild Shelves` (one entry per guild where your role holds view grants).
  - Every card and row **must** display a clear provenance badge (`@handle's bucket` or `Guild: <Name>`).
- **Local-Only Rich Metadata & Type Filters:**
  - Filter strictly by file type/extension: `Video`, `Audio`, `Reading (EPUB/CBZ/PDF)`, `Game & Mod Packs`, `Images`, and `Other`.
  - Extract covers and thumbnails **strictly from the file itself** client-side: ID3 art via `lofty` (WASM), page 1 of `.cbz`/`.epub`, and client-side `<video>` frame capture.
  - Allow the bucket owner to manually edit a custom title, cover image, or note stored in their own shelf index.
  - Keep personal local rails only: `Continue Watching/Reading`, `Recently Added to Shelves You Can See`, and `Pinned Folders`.
- **The Screening Room:** Move synced Watch Together video playback and scheduled Guild Live TV guides cleanly under the **Voice Channel / Screening Room** surface as a social activity over shared shelves rather than a global broadcast storefront.

### Worktree 41-C: Client-Side Zero-Knowledge Key Wrapping (`sr-e2ee`, `sr-storage`, `web`)
- **Envelope Encryption for Bucket Credentials & Catalogs:**
  - When a user mounts an S3/R2 bucket, generate a symmetric **Shelf Key ($K_{\text{shelf}}$)** in browser WASM (`sr-e2ee`).
  - Encrypt the bucket's read credentials and the shelf's catalog index (`catalog.enc`) client-side using $K_{\text{shelf}}$ before uploading to `sr-server`.
  - When granting access to a Friend or Guild Role, the owner's client wraps $K_{\text{shelf}}$ to the target friend's public identity key (or the Guild Role's MLS group key) and signs the grant with the owner's Ed25519 private key.
- **Anti-Ghost-User Invariant (`P28-CRYPTO-1`):**
  - `sr-server` stores only ciphertext `WrappedKey` blobs and `catalog.enc`.
  - Clients **must** verify the Bucket Owner's Ed25519 signature on every key-wrap row before wrapping/unwrapping keys, making it mathematically impossible for the server to inject an invisible/ghost user into a bucket's access list.
- **DMCA Delist Preservation (`P28-LEG-1`):**
  - Staff takedowns operate on `bucket_id` / `shelf_id` / `object_hash` by purging the encrypted `WrappedKey` and `catalog.enc` rows on the server and logging the strike.

**Wave 41 Gate Proof:** CI proves zero outbound requests to TMDB/external metadata providers, verifies Ed25519 signature checks on `WrappedKey` rows, and passes full E2E shelf browsing + playback using client-unwrapped keys.

---

## Wave 42: The Arcade Elevation, Guest Links & Two-Tier Trust
**Goal:** Elevate **The Arcade** to a primary room, enable instant unauthenticated `/play/<token>` Guest Links (scoped strictly to the game state, never E2EE voice), harden the game iframe sandbox, and implement the Two-Tier Verification & Build-Hash Vouch system.

### Worktree 42-A: The Arcade Room & 3-Tier Content Policy (`web`, `sr-server`)
- **The Arcade Surface:**
  - **Featured / Front Page:** Strictly curated by the Operator. Displays Operator-built games (`Land.io` roguelike, Hold'em, Tabletop, Emulator, plus upcoming flagship games) and Operator-signed community games.
  - **Guild Cartridges (Unlisted):** Browser for unlisted games/mods hosted on Friends' or Guilds' BYOS buckets.
  - **Coming Soon Showcase:** Curated showcase of compliant in-development originals and clean-room reimplementations with repo links and "Notify Me" toggles.
- **Mandatory 3-Tier Self-Declaration on Game Manifests:**
  1. `TIER_1_ORIGINAL`: 100% original code, art, audio, and IP. Eligible for Browser play, Guest Links, and Featured Front Page.
  2. `TIER_2_REIMPLEMENTATION`: Clean-room engine code with **zero** bundled proprietary assets (e.g., *Horizon* Rust Minecraft, *OpenMW*). Desktop/Tauri only; requires local base-game ownership check. Eligible for Featured Page.
  3. `TIER_3_FAN_IP`: Uses external IP names/characters. Restricted strictly to unlisted Guild Cartridges; blocked from Featured and Public surfaces.
- **Zero-Cut Creator Tip Badges:** Allow game manifests and `/~handle` profiles to attach optional peer-to-peer donation links (BTC/Lightning QR, Ko-fi, Patreon) with 0% platform intermediation.

### Worktree 42-B: Shareable `/play/<token>` Guest Links (`sr-gateway`, `sr-voice`, `web`)
- **Capability Link Generation:**
  - Any member inside an active Arcade lobby can generate a `noindex` shareable link (`/play/<lobby_token>#key=<ephemeral_game_key>`).
- **Strict Guest Isolation (Extending `D-385`):**
  - Unauthenticated guests opening `/play/<token>` load **only** the minimal isolated Arcade runner bundle.
  - Guests are assigned a randomized callsign (or lobby-approved nickname) and join a dedicated **Game-State WebRTC Data Channel / Sub-Room** scoped strictly to that `lobby_token`.
  - **Security Invariant (`P28-ARC-1`):** Guests on a `/play/<token>` link **never** receive the voice channel's MLS audio/video keys, cannot hear or speak in VC, and cannot read server chat or member lists outside the game scoreboard.

### Worktree 42-C: Hardened Sandbox, Swarm Code-Signing & Hash Vouches (`web`, `sr-cli`, `sr-server`)
- **Separate-Origin Sandboxed Iframe:**
  - All Arcade games run inside an isolated `<iframe sandbox="allow-scripts allow-pointer-lock">` served from a cookieless sandbox origin with a strict CSP (`connect-src 'none'` or scoped strictly to presigned R2 asset URLs).
  - All networking and identity calls pass strictly through the parent window's validated `postMessage` Activity SDK bridge.
- **Split Verification (`[SWARM-SAFETY-SIGNED]` vs. `[OPERATOR-FEATURED]`):**
  - Add `sr-cli arcade sign-code --hash <code_sha256>`: signs **only** the executable code bundle (`.wasm`, `.js`) after automated static/sandbox audit, without ever downloading or inspecting the developer's external S3 asset bucket.
- **Hash-Pinned Community Vouches:**
  - Users in good standing can click **"Vouch for Build"** on any game, cryptographically pinned to `code_sha256`.
  - If the developer updates the game and `code_sha256` changes, the UI badge automatically flips to `⚠️ UNVERIFIED UPDATE (N users vouched for previous build vX; 0 vouches on current hash)`.

**Wave 42 Gate Proof:** Playwright E2E test launches `Land.io` inside a voice call, opens the generated `/play/<token>` link in an incognito browser context, verifies bidirectional gameplay sync between the member and the guest, and proves the guest receives `403/Drop` if attempting to subscribe to the room's MLS voice stream or REST chat routes.

---

## Wave 43: Invite-Tree Radiation & The "Heavy Crown" Vouch System
**Goal:** Turn the invite graph and `/~handle` profiles into a self-policing Web of Trust with automated Upwind/Downwind blast-radius penalties, public detonation transparency reports, and reverse-privilege accountability scaling.

### Worktree 43-A: Tactical Nuke — Upwind Sterilization & Downwind Quarantine (`sr-server`, `sr-cli`, `web`)
- **Blast-Radius Execution Engine (`POST /api/v1/admin/moderation/nuke-branch` & `sr-cli drill nuke-branch`):**
  When executed against a malicious `target_user_id`:
  1. **Target:** Permanently banned, active sessions terminated, all owned buckets delisted.
  2. **Downwind (Descendants — `invited_by` subtree of Target):** Automatically **Purged or Quarantined** (assumed VPN sockpuppets or accomplices). All active invites revoked.
  3. **Upwind 1-Degree (Direct Inviter of Target):** Hit with **Permanent Invite Sterilization** (`can_invite = false` locked at DB level, overriding Patron/server perks; all unused invite codes revoked) + permanent lineage mark.
  4. **Upwind 2-Degree (Inviter's Inviter):** Placed on automatic 90-day invite freeze (`invite_frozen_until = now() + 90d`) with a system warning.
- **Public Detonation MOTD & Synthetic Drill Mode:**
  - Executing a branch nuke broadcasts a collapsible, high-visibility **System Detonation Notice (MOTD)** across the platform displaying the sanitized dependency tree (`Ancestor -> Inviter -> Target -> N Downwind Nodes`), the offense category, and the sterilization fallout.
  - `sr-cli drill nuke-branch --synthetic` generates an isolated dummy invite tree and triggers a live-fire MOTD drill to verify the pipeline and reinforce community opsec.

### Worktree 43-B: "Heavy Crown" Profile Vouches (`sr-server`, `web`)
- **Staked Profile Vouches:**
  - Any non-sterilized member can vouch for another member on their `/~handle` profile (1 vouch per pair, revocable).
  - **Vouch Fallout:** If a user is Tactical Nuked, every user who actively vouched for them receives a 90-day vouch/invite freeze and a radiation notice.
- **Reverse-Privilege Accountability Multiplier:**
  - Codify vouch tiers in the moderation engine (`0–49: Citizen`, `50–499: Pillar`, `500+: Elder/High-Trust`).
  - Automated moderation timeouts, holds, and strike weights scale **upward** (`1.5x` for Pillar, `3.0x` for Elder) when a high-vouch user commits a rule violation, visibly documented on the Vouch tooltip so users never farm vouches casually.

**Wave 43 Gate Proof:** Integration test builds a 4-generation invite tree + 3 profile vouchers, executes `nuke-branch` on Generation 3, and asserts: Gen 4 is quarantined, Gen 2 has `can_invite == false` (and fails `POST /invites` even with Patron flag), Gen 1 + vouchers have 90-day freezes, and the MOTD event is emitted.

---

## Wave 44: Self-Healing Crash Telemetry & Showroom Preview Mode
**Goal:** Wire up a privacy-preserving client crash ring-buffer for autonomous agent hotpatching, and build a one-click "Showroom / Populated Preview Mode" so early testers on the test box immediately experience a lived-in platform.

### Worktree 44-A: Deterministic Crash Ring-Buffer (`web`, `sr-server`)
- **Client-Side Ring Buffer:**
  - Maintain an in-memory rolling buffer in the browser/WASM client recording the last 60 deterministic Activity/Game ticks, WebGPU/WASM panic stack traces, route state, and browser capability flags.
  - **Strict Privacy Strip:** Never log message bodies, E2EE keys, raw S3 URLs, or user IPs.
- **Staff Server Triage Feed:**
  - On unhandled exception or WASM panic, post a deduplicated, structured JSON crash card into a dedicated `#crash-triage` channel in the Staff Server containing the commit SHA, stack trace, and replay buffer so a Claude worktree agent can reproduce and patch the bug immediately.

### Worktree 44-B: Showroom / Populated Preview Mode (`web`)
- **`[ 👁️ Preview Populated State ]` Client Toggle:**
  - Add a non-destructive toggle in the top bar (enabled for beta/test deployments via feature switch) that overlays rich, interactive fixture data onto the UI:
    - 10 populated Guilds in the rail with active voice statuses and unread indicators.
    - Multi-friend and multi-guild Shelves in **The Study** demonstrating source filtering and local metadata views.
    - A populated **Arcade** grid distinguishing clearly between **`[ PLAYABLE NOW ]`** live builds and **`[ PREVIEW / IN DEVELOPMENT ]`** showcase cards.

**Wave 44 Gate Proof:** Inject a synthetic WASM panic in a Playwright test, verify the PII-stripped crash payload arrives in `#crash-triage` with zero sensitive tokens, and verify Showroom Mode toggles cleanly without mutating backend state.

---

## Wave 45: Tauri Desktop Shell & Clean-Room Engine Launcher
**Goal:** Wrap the web client in a lightweight **Tauri v2** desktop shell (`sr-desktop`) for OS-level gaming ergonomics and local-only detection/launching of Tier 2 clean-room reimplementations (e.g., *Horizon* Rust Minecraft).

### Worktree 45-A: Tauri v2 Wrapper & Gaming Ergonomics (`sr-desktop`)
- Package the React/Vite + WASM frontend inside a minimal Rust Tauri v2 binary (<15MB).
- Enable **Global OS Push-to-Talk** and global mute keybinds when the window is unfocused.
- Enable **Persistent Raw Pointer Lock (`Esc` key override)** so pressing `Esc` inside full-screen WebGPU FPS games opens the in-game menu without breaking mouse capture.
- Provide a dedicated local SSD cache directory for multi-gigabyte BYOS game asset packs.

### Worktree 45-B: Zero-Telemetry Local Reimplementation Launcher (`sr-desktop`, `web`)
- **100% Local Base-Game Ownership Check:**
  - For `TIER_2_REIMPLEMENTATION` Arcade entries (such as *Horizon* targeting Minecraft Java `26.3`), check the scoped local OS path (e.g., `.minecraft/versions/26.3`) strictly on the user's machine.
  - **Privacy Invariant (`P28-DESK-1`):** The result of local filesystem checks **never** leaves the client machine over the network. If detected, render a local badge: *"Recommended locally because Minecraft: Java Edition (26.3) is installed on this PC."*
- **Process Handoff + Social Wrapper:**
  - Trigger the external engine's official launcher/binary with direct-connect lobby CLI flags while keeping the player connected to SIDE ROOM's E2EE voice channel and updating their local lobby status.

---

## Wave 46: Arcade Launch Suite — "Scrapbook Party" & Flagship Action Template
**Goal:** Ship the first post-pivot social party game leveraging SIDE ROOM's unique chat/Scrapbook data alongside the upgraded `Land.io` roguelike, plus a reusable WebGPU 3D multiplayer starter template.

### Worktree 46-A: "Receipt Roulette / Who Said This?" (Jackbox-Style Party Suite)
- Build a native voice-channel party game playable on desktop and via mobile **QR Code `/play/<token>` Guest Links** (using phones as controllers/drawing pads):
  1. **Receipt Roulette ("Who Said This?"):** Pulls real messages and quotes from the Guild's **Wave 34 Scrapbook** (or opt-in channel history), strips the author, displays it on the main VC canvas, and lets players vote on their phones or write convincing fake messages to fool the lobby.
  2. **Sketch & Slander:** Drawing and prompt-guessing game supporting custom, unhinged Guild Prompt Packs loaded directly from the Guild's BYOS bucket.

### Worktree 46-B: WebGPU 3D Arena Starter & `Land.io` Roguelike Polish
- Finalize `Land.io` roguelike card-drafting mode with full `/play/<token>` guest link support, bot backfilling (`Lumen bot`, `Tofu bot`), and deterministic host migration.
- Create `packages/activity-starter-webgpu-3d`: a turn-key 3D multiplayer starter pack (WebGPU + WASM physics + client-side prediction + S3 `.glb`/`.ktx2` streaming + E2EE spatial voice coordinate hooks) ready for building the flagship low-poly FPS / Magnum Opus.
