# Follow-up decisions (Fable, owner-delegated, 2026-11-02), for the follow-up integrator

Scope: the open rows of `docs/dev/owner-checklist.md` section 4b for waves 41 to 46, section 4c, and the parked items the owner named. Rules applied: the decisions log wins where it already decides; Discord parity by default; E2EE, authorization and the audit logs are never toggleable (IMMUTABLE RULE); nothing app-side before sign-in (D-385); no new dependency or download (those are owner-only); privacy-preserving defaults. Repo read at `main`, nothing edited.

Key findings that change the picture:
- The QR encoder the checklist says "is not a dependency" already exists in-house: `packages/ui/src/util/qr.ts` (`encodeQr`, `QRCode`), and the Arcade already draws Bitcoin and Lightning tips as QR codes (`apps/web/src/arcade/TipBadges.tsx`). Only the profile page lacks it.
- The system-DM seam the Notify delivery needs already exists: `SystemDm` in `crates/sr-api/src/routes/developer/mod.rs` (used by appointment offers and data-request notices), and `ArcadeService::watchers` is already written and tested but has no caller.
- The PS1 HLE conflict is owner text (D-390, "Owner default (roadmap-p28)") against an integrator default (P37-EMU-2, "Default"). The log wins with the owner's row.
- A shelf delisting tied to a takedown notice already counts toward the repeat-infringer counter through that notice (`takedown_items` of kind `storage_object` carry `subject_person_id`; `takedowns.rs` counts notices by person). The "separate" default only concerns a bare staff delisting with no notice.

Format per question: id, the question, the decision, why, and for a change what the implementing agent does.

## 1. The Study (wave 41)

### FU-STU-1 (D-401): write-capable mount keys
**Keep the default** (write keys allowed; the mount UI recommends a prefix-scoped key). Uploads, conversions and emulator saves write with the mount key, and grantees never hold any key (D-393), so the write power never leaves the owner and the server. Split keys stay an option for a later wave.

### FU-STU-2 (D-402): who edits titles, covers and notes on a server's shelf
**Keep the default** (the bucket owner and the server's managers, `scopes::may_manage`). Discord parity: a server's managers manage the server's content; the owner-only option would make a manager ask the bucket owner for every typo.

### FU-STU-3 (D-403): Continue Watching and Continue Reading across devices
**Keep the default** (per browser, `localStorage`, never sent). A server table is a new record of what each member watched; the privacy-preserving default is not to keep one. If the owner wants it later it is an opt-in setting, never a backfill.

### FU-STU-4 (D-404): the member submission API behind the ingest switch
**Keep the default** (`/submissions*` answers 404 while `platform.ingest_queues` is off). Nobody uploads into a queue nobody reviews, and a 404 tells a visitor nothing (D-385 spirit).

### FU-STU-5: the PlayStation HLE BIOS, D-390 against P37-EMU-2
**Change: follow D-390.** The log already decides it, and the deciding row is the owner's (roadmap-p28, 2026-10-31); P37-EMU-2 is the integrator's conservative default of the same day. The HLE BIOS is PCSX-ReARMed's own open-source code, already inside the pinned core; nothing of Sony's is shipped, stored or cached, so the BIOS rule of `bios.ts` is untouched. No download: the core is in `deploy/emu-cores.lock` already.

Implementation (web only, S):
- `apps/web/src/emu/cores.ts`: the PS1 entry's `needsBios: true` becomes `bios: 'optional'` (others `'none'`); keep the field name `needsBios` for the panel if easier, but the launch rule must distinguish "required" from "better with".
- `apps/web/src/emu/useEmuGame.ts` (~130): when the core is PS1 and `biosRef.current` is null, do not `fail({ kind: 'bios' })`; start the session without a BIOS file and set a `hle: true` flag on the session status. `session.ts` writes nothing to the core's FS in that case (the core falls back to HLE by itself).
- `apps/web/src/emu/BiosPanel.tsx` and `EmuPage.tsx`: while `hle` is true show an inline notice "Running on the core's built-in HLE BIOS; some games need a real BIOS from `emu/bios/` of your own bucket" with the panel still offered. The refusal copy for a wrong-digest file stays.
- Vitest `apps/web/src/emu/__tests__`: a PS1 ROM with no BIOS starts and reports `hle: true`; with a checked BIOS the file is written and `hle` is false; a refused BIOS still refuses (unchanged test).
- First step for the agent: confirm on the pinned `pcsx_rearmed` build that it starts with no BIOS file (its `psxbios`/HLE path). If the pinned build refuses, stop and report; do not patch the core.
- Docs: `deploy/EMULATOR.md` BIOS section, lawyer brief Q23 one sentence ("the built-in open-source HLE BIOS is used when the member mounts none"), and the P37-EMU-2 row marked superseded by D-390 (see the rows at the end).

### FU-STU-6: shelf strikes and the repeat-infringer counter
**Keep the default** (a bare delisting does not count), with the rule stated. A delisting made from a takedown notice already counts through that notice, because the notice's `storage_object` item names the subject person; a delisting with no notice is a staff act for any reason (safety, policy, a mistake being fixed), and feeding it to a counter whose wording the lawyer is reviewing (section 4a) would make a policy removal a copyright strike. Legal judgement on the counter itself stays with the owner and the lawyer.

Implementation (proof only, S, Rust test): in `crates/sr-moderation` Postgres tests, `a_shelf_delisting_without_a_notice_never_counts_as_a_strike`: delist with `notice_id: None`, run the strike count for the owner's person, expect 0; delist with a notice whose item names the person, expect 1 after the notice is upheld. One sentence in `docs/specs` (the takedown spec, strikes section): "shelf delistings count only through the notice they belong to".

## 2. The Arcade and the sandbox (wave 42)

### FU-ARC-1 (D-405): vouch "good standing"
**Keep the default** (active, non-bot, 3 days old, 30 a day). Wave 44 aligned the profile vouches on the same age (D-425); two rules with one number are easier to explain.

### FU-ARC-2 (D-406, D-407): OPERATOR-FEATURED only while signed; the 8-character previous build
**Keep both.** A featured badge on unsigned code would be the platform vouching for code it has not checked; the hash prefix is what the vouches are pinned to (D-409), and a developer version number would be a self-declared string that proves nothing.

### FU-ARC-3 (D-408, D-409, D-410, D-411): URL-mapping writes, trust on the code hash, cartridges on shelf objects, the Arcade link
**Keep all four.** D-408 is the only way a sandboxed `Origin: null` page can write without an origin allowance; the rest are the shipped shape and nothing argues against them.

### FU-ARC-4: Notify-me delivery
**Change: build it, as a system DM when the Operator releases the entry.** A stored opt-in that never fires is worse than no toggle; the seam (`SystemDm`) and the query (`ArcadeService::watchers`) exist, so it is a small wiring job with no dependency. Discord has no equivalent, so the quietest channel is right: one DM from the system user, no announcement, no e-mail.

Implementation (M, Rust, no migration):
- `crates/sr-activities/src/arcade.rs`: `release_coming_soon(id, application_id: Option<i64>) -> ActivityResult<Option<Vec<i64>>>`: in one transaction read `watchers(id)`, delete the entry (the `arcade_coming_soon_notify` rows go with it), return the watcher ids; `None` when unknown.
- `crates/sr-api/src/routes/arcade.rs`: `POST /admin/arcade/coming-soon/{entry_id}/release` (same guard as `remove_coming_soon`: staff plus `OPERATOR`), body `{ application_id?: string }`. After the commit, for each watcher send a `SystemCard` through the `SystemDm` extension: title "<title> is out in the Arcade", one link button to `/games` (or to the application when `application_id` is given), no other button. The DM is best effort: a member who blocked the system user or closed DMs is skipped, as P32-UTL-3's reminders are; the response reports `{ notified, skipped }` counts only.
- Keep `DELETE .../coming-soon/{entry_id}` as the silent removal.
- Admin UI `apps/web/src/arcade/ArcadeAdmin.tsx`: a "Release and notify" button beside Remove, with the optional application picker from the Featured candidates list; i18n keys under `arcade.admin.release*`.
- Tests: `crates/sr-activities/tests/arcade.rs` (`a_release_returns_the_watchers_and_removes_the_entry`); an sr-api route test with a recording `SystemDm` fake (two watchers, one DM each, the entry gone, a second call 404); `route_matrix` row (staff+OPERATOR); OpenAPI, api-client and MSW regenerated; Vitest for the button.
- Privacy: the DM names the game and nothing about who else opted in.

### FU-ARC-5: profile Bitcoin and Lightning QR codes
**Change: draw them on the signed-in profile page with the encoder already in `@sideroom/ui`.** The checklist's premise (no QR library) is wrong since wave 42: `QRCode`/`encodeQr` are in-house (also used for TOTP) and the Arcade cards already use them for the same tip kinds. Not on the public `/~handle` surface: D-385 keeps the public build to its allowed module list and the public profile shows no tip links today; that stays.

Implementation (S, web only):
- Move `QrTip`/`QrBox` from `apps/web/src/arcade/TipBadges.tsx` into a shared component (`apps/web/src/tips/QrTip.tsx` or export from `TipBadges.tsx`) and use it in `apps/web/src/profilepage/Inspector.tsx`'s Links block for links whose `tip` is `btc` or `lightning`: QR (size 160, ecc `L`) plus the existing link and text beside it; the QR is omitted when the payload exceeds what the encoder takes (as the Arcade does).
- Vitest `apps/web/src/profilepage/__tests__/prf-pro.test.tsx`: a `bitcoin:` link renders an `svg` with the QR label and the text; an `https:` link renders no QR.
- `apps/web/src/public/allowed.ts` unchanged (a test that the public profile bundle does not include `util/qr` already fails the build if it did; keep it so).

### FU-ARC-6: Bitcoin address checksums on the server
**Change: verify bech32/bech32m on the server now; base58check only if `sha2` is already a workspace dependency of `sr-core`.** A tip link with a mistyped address sends a tipper's money nowhere; bech32 needs only integer arithmetic (about 40 lines), no dependency. Base58check needs a double SHA-256: `crates/sr-core/Cargo.toml` lists no `sha2` (and no bitcoin crate); if `sha2` is in the workspace already (it is used elsewhere for digests) adding it to `sr-core` is a Cargo edge, not a download; if it is not, base58check is owner-only.

Implementation (S, Rust):
- `crates/sr-core/src/tips.rs`: `bech32_checksum_ok(addr) -> bool` (BIP-173 polymod, constant 1 for bech32, `0x2bc830a3` for bech32m; `bc1q` v0 must be bech32, `bc1p` v1+ bech32m); `bitcoin_ok` requires it for `bc1` addresses. `base58check_ok` behind the `sha2` condition above, for `1` and `3` addresses. Update the module comment ("the address checksum is not verified here").
- Tests beside `bitcoin_and_lightning_uris`: a known-good `bc1q` and `bc1p`, one with a flipped character (refused), the Lightning invoice path unchanged (an `lnbc` invoice's checksum is bech32 too; verify it if cheap, else leave).
- Error copy: the existing manifest/profile validation error for a bad tip link; no new code needed.

### FU-ARC-7: drop `sr_p42_arcade_room`
**Owner-only** (the owner's development machine; listed below).

## 3. Guest links (wave 43)

### FU-GST-1 (D-412): guest names
**Keep the default** (random callsigns). D-397: nothing a guest types reaches other players; an approved nickname needs a two-sided flow and reopens that rule.

### FU-GST-2 (D-413, and D-413 with D-444): who bridges the guests; guests lose the relay when the creator leaves
**Keep both.** The link key is the URL fragment and never reaches the server; sharing the bridge means passing the key to other members over the room's E2EE lane, which is a design wave, not a default. The successor can make a new link.

### FU-GST-3 (D-414, D-419): text-channel games; a members-only lobby
**Keep both.** No E2EE data lane exists for text-channel instances, and nobody has asked for a lobby without guests.

### FU-GST-4 (D-416): rekey on revoke
**Keep the default.** A revoke closes guests (4410); a rekey needs the host to rotate `#key=`, which the server cannot do for it.

### FU-GST-5 (D-415, D-417, D-418): limits and the single-node relay
**Keep all.** Beta-sized limits; a second node is a scaling item with no instance that needs it.

## 4. Graduated moderation and vouches (wave 44)

### FU-MOD-1: the CLI branch action asks for no TOTP code
**Keep the default.** `sr-cli` runs on the host with the database; a code prompt there is theatre against anyone who already has that shell, and the two-person rule plus the web step-up (D-422) are where the control is. The audit entry is written either way (never toggleable).

### FU-MOD-2 (D-395, D-422): who is the Operator
**Keep the integrator's shape** (the Owner or a holder of the Owner-only `OPERATOR` bit, plus `PLATFORM_BAN` for a branch ban). It is the same Operator the infrastructure routes use; two definitions of one word would be a bug waiting.

### FU-MOD-3 (D-420, D-421): the "serious" set; young accounts never auto-frozen
**Keep both.** D-211 already decides the default severities and D-420 only adds the owner's four to it; the 7-day rule is the privacy-preserving side of D-392 (a fresh account is not judged by a machine).

### FU-MOD-4 (D-426): a member cannot remove a vouch given to them
**Keep the default.** A per-vouch "decline" needs the recipient to see who vouched, which D-429 withholds on purpose; a blanket "do not count vouches for me" would let a member switch the multiplier off just before misbehaving. The cap and age rule (D-425) bound the pile-on risk, logged in T44.

### FU-MOD-5 (D-425, D-428, D-429): vouch limits; the voucher notice names the category; vouching needs a visible profile
**Keep all.** The limits are the reason no one member can mint a crown; the notice rule and the visibility rule are the privacy-preserving choices.

## 5. Crash telemetry and the Showroom (wave 45)

### FU-CRS-1 (D-432): crash reports opt-out and Global Privacy Control
**Change: honour the browser's GPC signal; no separate member setting.** A report holds no personal data (D-430: no account id, address, user agent, URL or message text) and nothing is sent before sign-in, so a settings toggle would suggest the report is personal data when it is not; but a member who has set GPC has stated a preference, and honouring it costs nothing and is the privacy-preserving default.

Implementation (S, web only):
- `apps/web/src/crash/report.ts`: before `apiFetch('/crash-reports', ...)`, return when `navigator.globalPrivacyControl === true`. The `CrashBoundary` still shows the local error page and the ring buffer still records (nothing leaves the tab).
- Vitest `apps/web/src/crash/__tests__`: with `globalPrivacyControl` true no POST is made; with it absent the POST is made (existing test).
- One sentence in the privacy section of the house-notes template (`apps/web` legal templates, every locale falls back to English): crash reports exist, hold no personal data, and a browser with Global Privacy Control sends none.

### FU-CRS-2 (D-433): audit reading the crash list
**Keep the default** (the replay read is audited, the list is not). The list carries no personal data and is read on every page open of a triage client; auditing it would bury the entries that matter. Nothing toggleable is involved.

### FU-CRS-3: crash limits as platform settings
**Keep the constants.** The limits protect the server from a crash loop, not a product choice an operator tunes; the limiter is built once at start-up (`CrashLimiter::new(CrashLimits::default())`) and a live setting would need a reload seam for no case anyone has hit. Revisit if an instance ever reports a 429 on a real crash.

### FU-CRS-4 (D-433): who sees `#crash-triage`
**Keep the default** (roles that carry a platform permission; cosmetic staff roles do not). The channel is the agent's work queue, not a staff lounge; any appointed staff member can still read the two JSON routes.

### FU-SHW-1 (D-435): the Showroom switch in production builds
**Keep the default** (the lazy chunk ships, `platform.showroom` OFF decides). The toggle is only rendered for signed-in members (D-385 holds) and the chunk is not fetched until a member turns it on; a build flag would double the image matrix for a switch that is already off.

### FU-SHW-2 (D-436): the Showroom over the Lounge
**Keep the default** (rail, Study and Arcade replaced; Lounge, Cinema and Library real). The preview replaces screens, never data; a store-level overlay would put fixtures into the real guild and message stores, exactly the mixing D-436 forbids. If a populated Lounge is wanted for a demo, it is a Lounge-page replacement like `PreviewStudy` (a separate fixture chunk), scheduled as its own item, not an overlay.

## 6. The 3D starter and Land.io (wave 46)

### FU-A3D-1 (D-441): `connect: ["self"]` for a built-in Activity
**Keep the default.** The reviewer sees the connect list with the bundle; shipping models as JS modules would only move bytes to work around a declared, reviewed rule.

### FU-A3D-2 (D-440): the two Jolt string rewrites
**Keep the default.** A self-built Jolt needs Emscripten (owner-only); the counted rewrites with a pinned input hash stop the build on any change.

### FU-A3D-3: reserved capabilities as a commented example; arena3d netcode choices; bot backfill (D-443); Draft Night (D-442)
**Keep all.** Shipped shapes with proofs; nothing argues against them.

### FU-A3D-4: names like "Lumen bot"
**Keep the default** (allowed). Display names are platform-wide; refusing a " bot" suffix would be a new name rule for one game's scoreboard. Discord does not forbid the word either; it badges real bots, which Land.io already does by naming its bots on the seeded RNG.

### FU-A3D-5 (D-444): host handoff as a capability; snapshot bandwidth
**Keep both.** Hold'em, Whiteboard, Tabletop and arena3d keep host-only state a successor would not have; 4 to 8 KB/s per member is below one voice stream.

### FU-A3D-6 (D-446): shelf objects for Activities and the CSP
**Keep the built shape** (one object per ask, the game fetches it only when its reviewed `connect` names that host). Adding mount hosts to the CSP per instance or relaying bytes through the page are both design waves (M/L); neither is needed by a shipped game.

### FU-A3D-7: KTX2/Basis textures; arena3d open points
KTX2 needs a transcoder download: **owner-only**. The open points (hidden-tab lag, GPU loss, touch controls) **stay documented, not built**.

## 7. Earlier questions, defaults standing (section 4c)

### FU-MUS-1 (wave 30): favorites cap; hidden unreachable tracks; hand edits win; where the library opens
**Keep all four as built** (P30-MUS-1). Favorites sharing `music.max_items` needs no second number; hiding a track the viewer cannot reach discloses nothing about another member's bucket; hand edits winning is what every tag editor does.

### FU-MUS-2 (wave 30): the `deny.toml` ignore of RUSTSEC-2024-0436 (paste, through lofty)
**Owner-only**: it closes only with a `lofty` bump (a cargo download). The ignore stands until then.

### FU-JB-1 (wave 31): P31-JB-2 to JB-5
**Keep all** (integer percent; a hidden track shows its length; titles visible to everyone who can view the channel; rooms never from a member's bucket). JB-5 is the privacy-preserving one: a 24/7 room must not disclose or depend on a member's bucket.

### FU-UTL-1 (wave 32): a second drawer key; Go to outside the Lounge; secret dice; loot tables; shared time zones; the ECB runbook line; English-only phrases; card reminders versus DM settings; "Open in studio" in the message menu
**Keep all.** P32-UTL-3 to UTL-5 already decide three of them; the rest are nice-to-haves with no shipped need, and shared time zones would be a new profile field (privacy) nobody asked for.

### FU-UTL-2 (wave 32): WebM voice messages
**Change: accept `audio/webm` (Opus) beside `audio/ogg` for voice messages.** Chromium's `MediaRecorder` cannot produce Ogg, so without it no Chromium member can record a voice message at all; Discord parity is "voice messages work in every browser". The file is still one Opus track of at most two minutes with a waveform.

Implementation (S, Rust):
- `crates/sr-api/src/routes/messages/rich_create.rs` (~227): the content type check accepts `audio/ogg` or `audio/webm`; `attachments.rs` (~395) the same for the voice-message size branch; the sniffer must confirm a WebM/Matroska container (`1A 45 DF A3`) as it does Ogg (`OggS`).
- `crates/sr-api/src/routes/cdn/content.rs`: `webm`/`audio/webm` served as media with the same headers as `audio/ogg` (check that the existing `video/webm` branch does not swallow it).
- Web: `apps/web/src/tools` audio tool (utl-audio) records with `audio/webm;codecs=opus` when `audio/ogg` is unsupported; the player uses `<audio>` which plays both.
- Tests: an sr-api route test posting a WebM voice message (accepted) and a WebM with a video track or a wrong magic (refused `VOICE_MESSAGE_SHAPE`); Vitest for the recorder's type choice.

### FU-UTL-3 (wave 32): purging finished invite cards and their claimant lists
**Change: purge claimant lists 30 days after a card finishes or is cancelled; the card message stays.** A claimant list is personal data (who said they would come) with no use after the event; the privacy-preserving default is to drop it on a clock. Discord's own events vanish after they end.

Implementation (S/M, Rust, migration only if the cards table lacks an end timestamp):
- `crates/sr-server/src/jobs.rs`: a daily job `invite_cards.purge` beside the retention jobs, deleting claimant rows of cards whose finished/cancelled time is older than `invites.card_claimants_retention_days` (catalog, group `limits`, default 30, 1..365; 0 = never). The card keeps its final counts (claimed N) as numbers.
- Test: a Postgres job test (one finished card 31 days old loses its claimants and keeps its count; one 10 days old is untouched; a live card is untouched); the settings catalog test gains the key.
- `docs/dev/retention` (the wave 40 retention table) gains the row.

### FU-ENT-1 (wave 30b): an OAuth2 bearer reading `openapi.json`
**Keep the default** (session or bot only). It is one matrix cell either way; the smaller surface wins until an SDK needs it.

### FU-ENT-2 (wave 30b): the gate's `429` bucket as a setting
**Keep the fixed default** (240 `401`s per address per minute). A rate limit on the sign-in gate is not something to loosen from an admin page; an operator with a real need changes the constant.

### FU-ENT-3 (wave 30b): the `house-notes` copy key reaches no page
**Change: `sr-server` substitutes the key into the rendered house-notes template.** D-239 decides that `MANAGE_ENTRANCE` owns the entrance copy keys, `house-notes` among them; the 30b build-time templates orphaned it. Dropping the key would contradict a decided row; substituting it honours it.

Implementation (S, Rust): `crates/sr-server/src/web.rs` (`GET /house-notes`, ~676) fills a `{{house_notes}}` slot in the per-locale template with the key's value (escaped as text, line breaks to paragraphs, no HTML) the same way it fills country and contact; the template gains the slot above the rules section; a web test renders a value and proves it is escaped; the entrance admin page's hint for the key says where it appears.

### FU-ENT-4 (wave 30b): `/app` remembering the last section
**Change: write `lastSection` so `/app` reopens where the member left.** Discord reopens the last server and channel; the store field and the reader exist (`apps/web/src/state/session.ts`, `AppDoor.tsx`) and only the writer is missing. Signed-in only, nothing before sign-in.

Implementation (S, web only): in the app router's navigation effect (`apps/web/src/app/main.tsx` or the router subscription), call `setLastSection(pathname)` for in-app paths that are sections (`/channels/...`, `/cinema`, `/study`, `/games`, `/library`, `/music`), never for settings, auth or error pages, and never with a query string; keep it in `localStorage` per account like the player settings. Vitest: navigate to `/study`, reload the store, `/app` resolves to `/study`; a settings path is not remembered.

### FU-BETA-1 (wave 34): Caddy proxying `/~*`
**Change: proxy `/~*` to `sr-server`.** A signed-in member opening `/~handle` behind Caddy sees the public view, which is a parity bug (Discord shows you the full profile when you are signed in); `sr-server` can read the session and choose the view, Caddy cannot.

Implementation (S, deploy): the Caddyfile template (`deploy/`) routes `/~*` to `sr-server` beside `/api/*`; `sr-server`'s `/~` handler serves the public page when no session is present and the signed-in app entry when one is (it already does on direct hosting; confirm with a web test); `deploy/UPGRADE.md` one line; the deploy render test gains the route.

### FU-BETA-2 (wave 34): Now playing only for signed-in viewers; the rolling allowance of 20; scrapbook grants, reactions in private channels and a per-server cap; `media-src https:`
**Keep all** (P34-PRF-2, P34-RNG-1, P34-SCR-1, P34-UNF-1 already record them). Rings on by default was answered by the owner at the checkpoint (off), so it is not open.

### FU-BETA-3 (wave 34): `platform.public_profiles` for the beta; PeerTube parties and non-Safari HLS; which player-skin extras come next
`platform.public_profiles` is the owner's exposure choice: **owner-only**. hls.js is a download: **owner-only**. Skin extras: **keep unscheduled**; when scheduled, sleep timer and tape speed first (no assets), the ambience mixer last (sound files and a licence, owner-only).

### FU-OTHER-1: game servers (D-383), the node agent's address notice, CurseForge and the EULA click
D-383 decides the deferral; the notice and the EULA are legal judgement: **owner-only**.

## (a) Implementation list, by user impact

| # | Item | Size | Rust | Migration |
|---|---|---|---|---|
| 1 | FU-UTL-2 WebM voice messages (Chromium members can record at all) | S | yes | no |
| 2 | FU-ARC-4 Notify-me delivery by system DM on release | M | yes | no |
| 3 | FU-ENT-4 `/app` reopens the last section | S | no | no |
| 4 | FU-BETA-1 Caddy proxies `/~*` (signed-in members see their full profile) | S | no (deploy) | no |
| 5 | FU-STU-5 PS1 HLE BIOS fallback (D-390) | S | no | no |
| 6 | FU-ARC-5 profile Bitcoin/Lightning QR with the in-house encoder | S | no | no |
| 7 | FU-CRS-1 honour Global Privacy Control for crash reports | S | no | no |
| 8 | FU-ARC-6 bech32/bech32m checksums (base58check if `sha2` is already in the workspace) | S | yes | no |
| 9 | FU-UTL-3 purge invite-card claimant lists after 30 days | S/M | yes | maybe (end timestamp) |
| 10 | FU-ENT-3 `house-notes` copy substituted into the legal page | S | yes | no |
| 11 | FU-STU-6 proof test: a bare shelf delisting never counts as a strike | S | yes (test) | no |

Everything else keeps its default and needs no code.

## (b) Owner-only

- Drop `sr_p42_arcade_room` on the development machine (`DROP DATABASE sr_p42_arcade_room;`).
- `deny.toml` ignore of RUSTSEC-2024-0436: closes only with a `lofty` bump (download).
- KTX2/Basis textures for arena3d (transcoder download).
- A self-built Jolt (Emscripten) if the two string rewrites are ever unwanted.
- hls.js for non-Safari HLS and PeerTube in Watch Together (download).
- `platform.public_profiles` on for the beta (exposure choice).
- Player-skin ambience mixer (sound files and a licence); the owner's 20 skin references are not scheduled.
- Base58check for legacy Bitcoin addresses if `sha2` is not already a workspace dependency (one Cargo edge, but a new crate).
- The node agent's address notice and the CurseForge EULA click (legal, with D-383's deferral).
- Everything in section 4a (lawyer brief, minimum age, country, contact, N-35, D-336 (h), repeat-infringer wording, TMDB, the GPL source offer) and section 5 (downloads), untouched here.
- Unchanged owner items of waves 37 and 39: SNES/Genesis cores, homebrew ROM picks, `extract7z.js` licence, the first real whole-server bridge run, confirming O-6.

## (c) Decisions-log rows

Append under `## Follow-up (2026-11-02, owner-delegated)` with the 3-column header.

| ID | Decision | Status |
|---|---|---|
| FU-1 | **The PS1 core runs on PCSX-ReARMed's built-in open-source HLE BIOS when the member mounts none, as D-390 says; P37-EMU-2's "not offered" is superseded.** The BIOS rule of `bios.ts` is unchanged: a real BIOS comes only from `emu/bios/` of the member's own bucket, digest-checked, in memory, never stored; the shell says when a game is running on HLE and offers the panel. D-390 is the owner's row and the log wins with it. (wave 41 checklist) | Adopted (owner-delegated, Fable) |
| FU-2 | **A shelf delisting counts toward the repeat-infringer counter only through the takedown notice it belongs to; a delisting with no notice never counts.** A notice's `storage_object` item already names the subject person and the counter reads notices by person, so nothing changes in code; a bare staff delisting is a policy act, not a copyright strike, and the counter's wording is with the lawyer. Proof: a Postgres test in `sr-moderation`. (wave 41 checklist) | Adopted (owner-delegated, Fable) |
| FU-3 | **"Notify me" delivers: `POST /admin/arcade/coming-soon/{entry_id}/release` (staff + `OPERATOR`, optional `application_id`) removes the entry and sends each watcher one system DM with a link to the Arcade, best effort under the member's DM settings (P32-UTL-3); `DELETE` stays the silent removal.** The opt-in existed with nothing behind it; `SystemDm` and `ArcadeService::watchers` were already built. No announcement, no e-mail, nothing names other watchers. (wave 42 checklist) | Adopted (owner-delegated, Fable) |
| FU-4 | **Bitcoin and Lightning tip links on the signed-in profile page are drawn as QR codes with the in-house encoder (`@sideroom/ui` `QRCode`), as the Arcade cards already are; the public `/~handle` surface keeps text only (D-385).** The checklist's "no QR library" premise was outdated: the encoder has been in `packages/ui` since the TOTP enrolment. (wave 42 checklist) | Adopted (owner-delegated, Fable) |
| FU-5 | **`sr-core` tips verify the bech32/bech32m checksum of `bc1` addresses (and base58check of legacy addresses where `sha2` is already a workspace dependency; otherwise that half waits for the owner).** A mistyped address sends a tipper's money nowhere; bech32 needs no dependency. (wave 42 checklist) | Adopted (owner-delegated, Fable) |
| FU-6 | **Crash reports honour the browser's Global Privacy Control: a browser with `navigator.globalPrivacyControl` set sends none; there is no separate member setting.** A report holds no personal data (D-430) and nothing is sent before sign-in, so a toggle would misdescribe it, but a stated preference costs nothing to honour. The privacy page says so. (D-432 question) | Adopted (owner-delegated, Fable) |
| FU-7 | **Voice messages accept `audio/webm` (one Opus track, sniffed Matroska magic) beside `audio/ogg`; the recorder uses WebM where Ogg is unsupported.** Chromium's `MediaRecorder` cannot write Ogg, so without it no Chromium member could record one. (wave 32 question 3) | Adopted (owner-delegated, Fable) |
| FU-8 | **Invite-card claimant lists are purged `invites.card_claimants_retention_days` (default 30, 0 = never) after the card finishes or is cancelled; the card and its counts stay.** Who said they would come is personal data with no use after the event. (wave 32 question 11) | Adopted (owner-delegated, Fable) |
| FU-9 | **The `house-notes` entrance copy (D-239) is substituted by `sr-server` into the rendered legal page, escaped as text; the key is not dropped.** The 30b build-time templates had orphaned a decided key. (wave 30b question 3) | Adopted (owner-delegated, Fable) |
| FU-10 | **`/app` reopens the member's last section (`lastSection` is now written on in-app navigation, per account, never for settings, auth or error pages).** Discord parity; signed-in only. (wave 30b question 4) | Adopted (owner-delegated, Fable) |
| FU-11 | **Caddy proxies `/~*` to `sr-server`, which serves the public page without a session and the signed-in profile with one.** A signed-in member seeing the public view of their own instance was a parity bug Caddy could not fix alone. (wave 34 question 2) | Adopted (owner-delegated, Fable) |
| FU-12 | **Every other default of waves 41 to 46 and sections 4b/4c stands as recorded:** D-401 to D-404, D-405 to D-411, D-412 to D-419, the wave-44 rows (CLI branch action without TOTP, the Operator as D-422 defines it, D-420, D-421, D-425, D-426, D-428, D-429), D-433 (list reads not audited; `#crash-triage` for roles with a platform permission), crash limits as constants, D-435, D-436, D-440 to D-446, the "Lumen bot" name rule, P30-MUS-1, P31-JB-2 to JB-5, P32-UTL-1 to UTL-5 and the remaining drawer questions, the entrance `429` bucket and the bearer rule for `openapi.json`, P34-PRF-2, P34-RNG-1, P34-SCR-1, P34-UNF-1/2. Reasons per row in `fu-fable-decisions.md`. | Adopted (owner-delegated, Fable) |
