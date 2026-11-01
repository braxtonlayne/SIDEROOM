# The owner's action checklist

Everything that only the owner (an account, the host, legal, a yes to a download) or the integrator (a push, a dispatch) can do, in one ordered list. Tick a box when it is done. The decisions behind the rows stay in [the decisions log](../01-decisions-log.md); [open-questions.md](../open-questions.md) points here instead of repeating the list. Source of the plan: [roadmap-p20.md](roadmap-p20.md) section 8 and the owner round (D-308 orders the human actions).

Nothing in this list is done by an agent: agents never push, merge, download or spend money without a clear yes.

How to read a row: **What** (the action), **Unblocks** (what becomes possible or provable when it is done).

## 1. Accounts

- [ ] **Register the second Discord redirect URI** on the existing Discord application: `<public_url>/api/v1/connections/discord/callback` (D-292). Admin > System shows the exact URI once the instance has its public URL. **Unblocks:** the "Connect Discord" button on Settings > Connections, which stays hidden until the URI is registered.

## 2. Host

Do these in the order of [deploy/GO-LIVE.md](../../deploy/GO-LIVE.md); this list only says what each one unblocks.

- [ ] **Provision and harden the host** (doc 12 section 8: firewall with only 22, 80, 443, 3478, 5349 and the voice ports open, SSH keys only, unattended upgrades, fail2ban, disk encryption where the provider supports it). **Unblocks:** everything below.
- [ ] **Point DNS** at the host (the public hostname, and the TURN hostname if it differs). **Unblocks:** TLS issuance, the installer, `sr-cli preflight`'s public URL check.
- [ ] **Run the first real install** from the shipped files (`deploy/install.sh`, then `sr-cli preflight`). **Unblocks:** the exit gates that have never run on a real host (doc 13), every operations proof, and the first invites.
- [ ] **Create the ingest folder** `/srv/sideroom/ingest`, or mount the Storage Box there and set `SR_INGEST_DIR` (D-296). **Unblocks:** local-folder import in the Library and Cinema ingest.
- [ ] **Order and connect the Storage Box** (SFTP target for backups, doc 12 section 5; the repository goes in `SR_BACKUP_REPOSITORY`). **Unblocks:** off-host copies of the dumps, the WAL archive and the object snapshots; without it a dead disk takes the backups with it.
- [ ] **Web Push keys: nothing to generate.** The VAPID key pair is created by `sr-server` on first start and stored sealed in the database under `server.secret_key` (`crates/sr-server/src/app.rs`, `sr_push::load_or_create_vapid`). The action is only to keep `server.secret_key` safe and in the backup (`deploy/BACKUP.md`). **Unblocks:** Web Push on the installed PWA.
- [ ] **Decide the Bitcoin path** for payments (D-288, D-312): either run a non-pruned `bitcoind` on the host and the `electrs` Compose profile (replace the image placeholder in `deploy/compose/prod.yml` with the pinned digest you trust, point `SR_ELECTRS_DAEMON_RPC_ADDR` at the node), or keep the documented interim `SR_PAYMENTS_ESPLORA_URL=https://mempool.space/api`. **Unblocks:** live invoice addresses without third-party address leakage (the interim works but sends derived addresses to a third party).
- [ ] **Set the lawyer settings** after the brief is answered (see section 4). **Unblocks:** the entrance pages and the privacy page showing the operator's country, contact and minimum age.
- [ ] **A real device pass:** Safari voice E2EE and the installed iOS PWA on a Mac or iPhone; the manual screen-reader pass (NVDA on Windows is free). **Unblocks:** removing "untested on Safari" and the accessibility gate.
- [ ] **Fund a signet wallet and run RDY-PAY-001 once before the first paid invite** (D-368): a Patron invoice paid on signet through the real watcher, recorded in `docs/dev/payments-live-test.md` "Recorded runs". **Unblocks:** the phase 3 exit gate in doc 13.
- [ ] **Run the two real-use gates with friends:** two weeks of daily chat (phase 1) and a two-hour 10-person voice session (phase 2). **Unblocks:** those exit gates in doc 13.

## 3. GitHub

Pushes and dispatches are the owner's call; the integrator performs them once the owner says yes. Several of these workflows have never run on GitHub.

- [ ] **Push the integration branch** so the workflows below are on `main`. **Unblocks:** every row of this section.
- [ ] **Branch protection:** turn on "Require review from Code Owners" and make `Repo checks`, `Web` and `Release dry run` required checks (and `visual` when it exists). **Unblocks:** `.github/CODEOWNERS` taking effect; until then it is advisory.
- [ ] **First green run of `release.yml`** (static binaries and images, with `THIRD_PARTY_NOTICES.md` copied in). **Unblocks:** a release the installer and `UPGRADE.md` can point at.
- [ ] **First green run of `openapi.yml`.** **Unblocks:** the spec drift guard.
- [ ] **First green run of `storage.yml`** (real S3 multipart against Garage). **Unblocks:** the chat multipart upload and composer reattach proof.
- [ ] **First green run of `e2e.yml`.** **Unblocks:** FND-CI-004 and the three-browser proof.
- [x] **First scheduled or dispatched run of `sdk-compat.yml`.** **Unblocks:** ticking DEV-SDK-003 (D-306). (done 2026-10-26, run 40318281178; DEV-SDK-003 stays partly done until discord.py and twilight run, P26-NEXT-1)
- [ ] **Dispatch `load-test.yml`** (release build). **Unblocks:** the load target figure (2,000 sessions, 200 msg/s, REST p95 under 100 ms), which a debug build on a laptop missed.
- [ ] **First run of `backup-drill.yml`.** **Unblocks:** proof that the backup, restore and WAL scripts do not rot.
- [ ] **Dispatch `prod-smoke.yml`** once wave 20 is merged (it also runs on pull requests that touch `deploy/**`, the workflow, `crates/sr-cli/**` or `crates/sr-preflight/**`, D-354, so the phase-20 PR runs it first). **Unblocks:** the proof that the production stack boots from the shipped files. Make it a required check in branch protection only after it has been green on the default branch at least twice (D-354), and the `upgrade` job only after two green default-branch runs each under 25 minutes (P25-OPS-1): then add the three job names `stack`, `systemd` and `upgrade` as required checks for `main` (never `upgrade-base`). The companion workflow `.github/workflows/prod-smoke-skip.yml` (same job names, `paths-ignore` on the same four paths, jobs that only echo) reports them on pull requests outside the install surface, so those do not wait forever on an unreported check; it must be on `main` before the checks are required.
- [ ] **Dispatch `visual-baselines`** (the manual run of `e2e.yml`, D-307), then commit the downloaded `*-chromium-linux.png` files in a PR. **Unblocks:** the visual regression job (L-21, L-22) and its required check.
- [ ] **Triage the first `pnpm audit` and `shellcheck` output** in Actions (XC 4): fix or waive each finding. **Unblocks:** a clean `Repo checks`.
- [x] **First green run of `media.yml`** (real ffmpeg: probe, review proxy, HLS ladder, slate, resilience cases; weekly and on pull requests that touch the media crates). (done 2026-10-24, run 40124328097; the latest main run, 40668134948 on 2026-10-28, took 16.7 minutes of its 60) **Unblocks:** the Cinema and Live TV media proof (RDY-MED-003 to 005).
- [x] **First green run of `voice-soak.yml`** (dispatch only; ten members for ten minutes by default, `minutes=120` for the phase 2 gate). (done 2026-10-26, run 40258927429) **Unblocks:** RDY-VOX-002. The two-hour run is still the owner's to dispatch.
- [x] **First scheduled run of `advisory-watch.yml`** (weekly reminder for the temporary cargo-deny exceptions). (done 2026-10-26, run 40328348953) **Unblocks:** removing the `crypto-bigint` ignore when it fails.
- [x] **First green run of `flaky-tests.yml`** (nightly vitest repetitions). (done 2026-10-17, run 39012601715) **Unblocks:** the flake report. Its new job `hunt-e2e` (OPS-CI-001, the Playwright specs of waves 27 to 37, three repetitions) has not run yet: [ ] first green run of `hunt-e2e`.
- [x] **First green run of `prod-smoke-skip.yml`** (the companion that reports `stack`, `systemd` and `upgrade` on pull requests outside the install surface). (done 2026-10-26, run 40304494473) **Unblocks:** making the three `prod-smoke.yml` jobs required checks.
- [x] **First green run of `security-nightly.yml`** (OPS-IMM-003, wave 39: the whole authorization matrix in three flag states, compared; weekly on Monday 03:23 UTC and on demand). (done 2026-10-31, run 41224789059, dispatched on main after PR #70) **Unblocks:** the immutable-rules proof for the three rules that no switch may weaken (end-to-end encryption of calls, per-route authorization, the audit and access logs).
- [x] **First green run of the `Discord bridge against the fake Discord (Chromium)` and `Converter with a real ffmpeg for the fixture (Chromium)` steps of `e2e.yml`'s `live` job** (OPS-CI-001). (done 2026-10-31, run 41226230526: `bridge.spec.ts`, `bridge-whole-guild.spec.ts` and `convert.spec.ts` passed on GitHub after the CI fixes: `SR_BRIDGE_SUPERVISE_SECS=2` for the bridge step, the convert fixture without `-shortest`; both steps are required since) **Unblocks:** the CI proof of BRG-COR-001, BRG-UI-001, BRG-SYNC-001 and BYOS-CNV-001.

## 4. Legal and decisions

Each row says what you decide, **the default that stands until you do** (so nothing waits), and where it is recorded. Tick a decision when you have answered it; the answer goes into [the decisions log](../01-decisions-log.md).

### 4a. Legal

- [ ] **Hand the brief to a lawyer:** [docs/legal/lawyer-brief.md](../legal/lawyer-brief.md). It is questions only, not legal advice. Parts A to D are the launch questions; **Part E is the feature phase**: Q20 (brokering access to members' buckets, delisting), Q21 and its addendum (the Discord bridge notice, whole-server mode), Q22 (TMDB and other metadata terms), Q23 (emulator cores, BIOS), Q25 (music played to a private voice channel). Q24 and Q26 (game servers) are not asked until wave 38 resumes. **Unblocks:** every row below.
- [ ] **Set `accounts.minimum_age`** to what the lawyer says for the operator's country (default 13). Admin > Features > Compliance.
- [ ] **Set `compliance.operator_country`** (ISO two-letter code, for example `DE`). **Unblocks:** the privacy page naming the jurisdiction.
- [ ] **Set `compliance.operator_contact`** (the address copyright notices go to) and check that `/help` publishes it. **Unblocks:** the notice-and-action path being real: a notice address that nobody reads is worse than none.
- [ ] **Decide the two open build questions:** mailing the templated takedown reply (N-35) and the mandatory external reporting step (D-336 (h)). **Unblocks:** whether those two items are built.
- [ ] **Approve the repeat-infringer wording** for the house notes and `/help` (the brief names it). **Unblocks:** publishing the strike rule.
- [ ] **TMDB attribution and use** (Q22): confirm the attribution text and logo are shown wherever fetched metadata appears, or switch TMDB off with `review.lookup.enabled`. **Default:** attribution shown, covers kept as thumbnails only.
- [ ] **Decide the corresponding-source offer for the GPL pieces** (ffmpeg.wasm and the emulator cores; Q3 and Q23): a link on `/help` to each project's source and the exact build set in `deploy/emu-cores.lock`. **Default:** `THIRD_PARTY_NOTICES.md` (linked from `/help`) carries the licence texts and names each project; the pinned build set is in `deploy/emu-cores.lock` and `deploy/EMULATOR.md`. A dedicated source-offer page is not written; the lawyer says whether the link is enough.

### 4b. Decisions of the feature phase (waves 27 to 45)

The Study (wave 41, [handoff](handoff/integration-phase41.md)):

- [ ] **Write-capable mount keys** (D-401). The roadmap asked the mount check to refuse a key that can `PutObject`. **Default:** write keys stay allowed, because uploads, conversions and emulator saves use the mount key; grantees never hold a key either way. **Option:** split read and write keys per mount (a later wave).
- [ ] **Who edits titles, covers and notes on a server's shelf** (D-402). **Default:** the bucket owner and the server's managers. **Option:** the bucket owner only.
- [ ] **Continue Watching and Continue Reading across devices** (D-403). **Default:** per browser, never sent to the server. **Option:** a server table that follows the account.
- [ ] **The member submission API with the ingest switch** (D-404). **Default:** `/submissions*` and `/library/submissions/*` answer 404 while `platform.ingest_queues` is off, like the staff queue. **Option:** keep member submissions open with the queue off.
- [ ] **The PlayStation HLE BIOS: D-390 and P37-EMU-2 disagree.** Roadmap-p28 (D-390) allows the built-in open-source HLE BIOS when no user BIOS is mounted; wave 37's default (P37-EMU-2) does not offer it. Wave 41 did not touch the emulator. **Default until you say:** the shipped behaviour (P37-EMU-2, not offered); the wave that next touches the emulator should follow D-390 unless you object.
- [ ] **Shelf strikes and the takedown counter.** A delist strike (`shelf_delist_strikes`) records the notice id but does not count toward the repeat-infringer counter. **Default:** separate. **Option:** feed the counter.

The Arcade and the sandbox (wave 42, [handoff](handoff/integration-phase42.md)):

- [ ] **Vouch "good standing"** (D-405). **Default:** an active, non-bot account at least 3 days old, at most 30 vouches a day. **Option:** other numbers.
- [ ] **OPERATOR-FEATURED only while the current code is signed** (D-406). **Default:** an unsigned update keeps its Front Page place but shows UNVERIFIED UPDATE. **Option:** featured shows regardless.
- [ ] **UNVERIFIED UPDATE names the previous build by an 8-character code-hash prefix** (D-407). **Default:** stands (no version number is stored). **Option:** store and show a developer version.
- [ ] **URL-mapping writes from the sandbox** (D-408). **Default:** `Origin: null` plus the instance's per-participant proxy token; reviewed URL mappings admit `'self'`. **Option:** keep mapped writes off for games.
- [ ] **Trust keyed on the code hash** (D-409), **cartridges on shelf objects** (D-410), **the Arcade link always shown** (D-411). **Default:** all stand.
- [ ] **"Notify me" delivery is not built.** Opt-ins are stored (`arcade_coming_soon_notify`); nothing tells members when a game comes out yet. **Default:** leave it for a later wave. **Option:** an inbox notice when the Operator releases it.
- [ ] **Profile tip links: no Bitcoin/Lightning QR code.** They show as a payment link and the text. **Default:** stands (no QR library is a dependency). **Option:** approve a QR encoder or write one.
- [ ] **Bitcoin addresses are not checksum-verified** on the server (only shape and prefix). **Default:** stands. **Option:** verify base58check/bech32 checksums.
- [ ] **Drop the scratch database `sr_p42_arcade_room`** on the development machine: the integrator's session was not allowed to drop it.

Guest links (wave 43, [handoff](handoff/integration-phase43.md)):

- [ ] **Guest names** (D-412): random callsigns only. **Default:** stands. **Option:** a nickname the guest types and the host approves (needs an approval flow on both sides).
- [ ] **Who bridges the guests** (D-413): only the member who made the link; closing that tab cuts guests off from the room. **Default:** stands. **Option:** any member of the instance can hold the bridge (token and key passed over the room's E2EE channel).
- [ ] **Guest links from text-channel games** (D-414): not offered (no encrypted room data lane there). **Default:** voice channels only. **Option:** a lane for text-channel instances.
- [ ] **A members-only lobby** (D-419): a `lobby_channel` game without a guest link has no lobby. **Default:** none. **Option:** a lobby per instance.
- [ ] **Rekey on revoke** (D-416): revoking closes guests and announces an epoch but keeps the key. **Default:** stands. **Option:** the host rotates `#key=` and the lobby rekeys.
- [ ] **Limits and the single-node relay** (D-415, D-417, D-418): 6 h links, 2 h visits, 5 links, 16 guests, 32 seats; one process; the game served from the existing activity origin. **Default:** all stand.

The emulator (wave 37, [handoff](handoff/integration-phase37.md)):

- [ ] **SNES and Genesis cores** (P37-EMU-1). The pinned EmulatorJS 4.2.3 set has no build of Beetle Supafaust or BlastEm, and Snes9x, Genesis Plus GX and PicoDrive are non-commercial. **Default:** both platforms say "core not installed". **Options:** accept the gap, name another source of a GPL core, or schedule an Emscripten build wave.
- [ ] **The homebrew ROM picks** (P37-EMU-5). Candidates with a checked licence: Nova the Squirrel (NES, GPL-3.0) and Solar Guard (GBA, GPL-3.0); Super Tilt Bro (NES, WTFPL repository, assets unchecked, built from source). **Nothing was downloaded.** **Default:** no ROM ships; the Games page tells a member to bring their own. Pick none, one or two, and confirm each licence including art and music; details in [feat-p37-emu-ops.md](handoff/feat-p37-emu-ops.md) section 2.
- [ ] **The PlayStation HLE BIOS** (P37-EMU-2). **Default:** a real BIOS from the member's own bucket only, digest-checked; PCSX-ReARMed's built-in substitute is not offered. Say if you want it offered (games then run worse but need no BIOS). Also supply SHA-1 values from a BIOS dump you own for the slots that have none.
- [ ] **The `extract7z.js` licence.** EmulatorJS's repository is GPL-3.0, the 7-Zip port inside states no licence. **Default:** it stays, flagged "upstream licence unconfirmed" in `deploy/emu-cores.lock` and `THIRD_PARTY_NOTICES.md`. **Option:** drop it and unpack the core archives at install time.
- [ ] **The `/games` page policy** (P37-EMU-6). `blob:` is allowed in `script-src` only on documents loaded at `/games`. **Default:** a member who opens Games from inside the app keeps the policy of the page they first loaded, so the page reloads (inside a call it warns that a reload leaves the call). **Option:** widen `script-src` for the whole signed-in app with `blob:` so nobody reloads (a weaker policy everywhere).
- [ ] **Co-op seats and `view_games`** (P37-EMU-4). **Default:** a seat goes only to a member of the call's server who holds `view_games` there (the host's machine checks it, failing closed); everyone in the call may still watch the screen share. **Option:** any member of the call may sit.
- [ ] **Friend shelves and games** (P37-EMU-3). **Default:** a Friend Shelf never carries games; only the owner and server roles with `view_games` or `manage` reach `emu/`. **Option:** add a games column to the share.

Activities (wave 36, [handoff](handoff/integration-phase36.md)):

- [ ] **Mount images in activities.** The Activity CSP allows images only from `self`, `data:` and `blob:`. **Default:** a whiteboard picture or tabletop map cannot come from a mount. **Options:** an SDK command that hands the Activity a file the member may read, or an `img-src` allowance for the instance's media host.
- [ ] **An activity download right.** The sandbox has no `allow-downloads`. **Default:** the whiteboard's PNG export opens in a new tab. **Options:** `allow-downloads`, or a host `saveFile` command that asks the member.
- [ ] **Host handover** (P36-ACT-1). **Default:** when the host leaves the instance or the call, the instance ends for everybody. **Option:** a successor protocol (a new `started_by` named by the gateway, each activity handing over its state).
- [ ] **Card table, built-in Activities, data channel** (P36-ACT-2 to ACT-5): the 60-second turn timer and the host's Remove button; the data-channel capability needing no approval beyond bundle review; built-in Activities installed through the developer portal (no seed step yet); English only. **Default:** all four stand.

The Discord bridge (waves 35 and 35b, [35](handoff/integration-phase35.md), [35b](handoff/integration-phase35b.md)):

- [ ] **Held-channel hiding.** A channel deleted on Discord is held for `bridge.delete_hold_hours`, but the channel list, the gateway and the read routes still show it during the hold. **Default:** shown during the hold (the Restore and Delete now actions work). It touches every channel read path; schedule it if you want it hidden at once.
- [ ] **`Origin::Bridge`** (P35B-INT-2). **Default:** changes applied here from Discord run as the server owner, so their ordinary audit rows name the owner, not the bridge user; loop prevention rests on the `synced_state` comparison. **Option:** a shared parameter on the role, channel and thread services.
- [ ] **The first real whole-server bridge run.** Everything so far ran against a fake Discord. To try it: a Discord application with a bot, the privileged intents MESSAGE CONTENT, SERVER MEMBERS and PRESENCE, the bot invited with the link the bridge screen shows (it needs Administrator for whole-server mode), then Server Settings > Modules > Discord bridge as the server owner. Run it first on a throwaway Discord server and record the result in [bridge docs](../../deploy/OPERATOR.md#the-discord-bridge-d-381-d-386). **Unblocks:** trusting whole-server mode (design risk R3).
- [ ] **The whole-server defaults** (D-386, P35B-Q1 to Q11): SIDE ROOM owns role order and permissions, a role deleted on Discord is deleted here only if it denies nothing, Discord-only roles import only when the owner confirms the plan, no exclusions, no history backfill, voice structure only, a teardown that leaves Discord as it is (with two optional extras), and the red-flagged Administrator warning. **Default:** all stand. D-386 is still marked proposed in the decisions log.
- [ ] **The bridge defaults of wave 35** (P35-BRG-4 to BRG-8): the MFA step only when the server's level asks, 24-hour signed links, presence used when Discord grants it, no mention collisions. **Default:** all stand.

Profiles, invites and operator switches (waves 34 and 39):

- [ ] **The `profiles.access` upgrade change** (P39-FLG-1). Public profile pages now need `platform.public_profiles` **and** `profiles.access = opt_in_public`. **Default:** `walled`, so an instance that had public profiles on must set the second one after upgrading or its public pages go dark ([deploy/UPGRADE.md](../../deploy/UPGRADE.md)). Confirm O-6.
- [ ] **The invite "never activated" definition** (P39-INV-1). **Default:** the email was never verified and the gateway never saw the account online (`last_seen_at` is null); it only marks display data, nothing is deleted.
- [ ] **Lock the Doors also blocks emergency invites** (P39-INV-2) and `invites.locked` defaults to off. **Default:** both stand.
- [ ] **Existing profiles become private** (wave 34, important). No backfill was run. **Default:** leave it. **Options:** a notice in the profile editor (shipped as a notice), or a one-time switch to Members only for customised profiles.
- [ ] **O-1** (storage grants as a table or permission bits 61 and 62): the table stands. **O-4** (the emulator core set): the recommended set stands. **O-6** (`profiles.access`, `invites.locked`): the defaults stand.

Graduated moderation and profile vouches (wave 44, [handoff](handoff/integration-phase44.md)):

- [ ] **The CLI branch action asks for no TOTP code** (MOD-GRD-001, T43). `sr-cli moderation branch-action` runs as the Operator on the host (shell access) after a second staff member approved the request on the web with a 2FA step-up; the web route `POST /admin/moderation/branch-action` does demand the Operator's step-up. **Default:** the CLI stays as it is. **Option:** wire the AuthService into the CLI so it asks for the Operator's code too.
- [ ] **Who is "the Operator"** (D-395, D-422). **Default (integrator):** the Owner, or a staff member the Owner granted the Owner-only `OPERATOR` bit (the same Operator as the infrastructure routes); a branch ban also needs `PLATFORM_BAN`. 44-A had built it as the Owner alone. **Option:** go back to the Owner alone.
- [ ] **Young accounts are never auto-frozen** (D-421). An account younger than 7 days cannot be judged "unactivated" under D-392, so a case lists it for review and never freezes it automatically. **Default:** stands.
- [ ] **The "serious" set** (D-420). The member ban route treats spam, fraud, ban evasion, child safety, illegal content and platform abuse as serious (a subtree case opens and vouchers are told). **Default:** that list. Confirm or change it.
- [ ] **A member cannot remove a vouch given to them** (D-426). Only the voucher can withdraw it, so many aged accounts could pile vouches on someone to raise their automated penalties up to 3x (T44 residual, capped by the limits below). **Default:** no "decline vouch". **Option:** let the recipient remove a vouch.
- [ ] **Vouch limits** (D-425): at most 20 new vouches a day, 500 outstanding, and the voucher's account at least 3 days old. **Default:** those numbers.
- [ ] **The voucher notice names the category, never the banned member** (D-428). **Default:** stands. **Option:** name the member (the voucher already knew whom they vouched for).
- [ ] **Vouching needs a visible profile** (D-429 with the wave-34 privacy rule). A page is private until its owner opens it, so nobody can vouch for (or see the vouch panel of) a member whose page is private. **Default:** stands; it follows the profile privacy the member chose.

Crash telemetry and the Showroom (wave 45, [handoff](handoff/integration-phase45.md)):

- [ ] **Crash reports: opt-out and Global Privacy Control** (D-432). **Default:** every signed-in browser reports, with no opt-out, because a report holds no personal data (no account id, address, user agent or message text; only the allow-listed card) and nothing is sent before sign-in. **Option:** a member setting to switch reports off, and/or honour the browser's GPC signal.
- [ ] **Audit reading the crash list** (D-433). **Default:** opening one replay (`GET /admin/crash-reports/{id}`) writes the `crash.report.read` audit entry; reading the list does not. **Option:** audit list reads too.
- [ ] **Crash limits as platform settings** (D-432). **Default:** constants in code: 6 reports per account per 10 minutes, 600 per hour for the platform, 60 new crash groups per hour, 32 KiB per report, 5 per page load. **Option:** make them editable under Admin > Limits.
- [ ] **Who sees `#crash-triage`** (D-433). **Default:** staff roles that carry a platform permission; a staff role without one (a cosmetic role) does not. Any appointed staff member can read the two JSON routes. **Option:** every staff role sees the channel.
- [ ] **The Showroom switch in production builds** (D-435). **Default:** the preview's code ships in every build (lazy chunk, loaded only when a signed-in member turns it on) and the operator's switch `platform.showroom`, OFF by default, decides. **Option:** a build flag so production images do not carry the toggle at all.
- [ ] **The Showroom over the Lounge** (D-436). **Default:** the preview replaces the servers rail (ten sample servers), The Study and The Arcade; the Lounge channel list, messages, the Cinema and the Library stay real. **Option:** a store-level overlay with a bigger fixture set so the Lounge itself looks populated.

Dependencies:

- [ ] **tantivy 0.27 on or after 2026-11-17** (OPS-DEP-001, DEP-FU-001). Already approved; the item is not started and waits for its date (or an advisory). It is the wave 40 agent `dep-tantivy`'s job once the date comes: `cargo update -p tantivy`, the three `deny.toml` ignores removed, `cargo deny check`, the format-7 index opening read-write. **Unblocks:** closing DEP-FU-001.

### 4c. Earlier questions, defaults standing

These were asked in the integration handoffs of waves 30 to 34 and have not been answered; each default stands. They are one line each here; the full wording is in the handoff named.

- Music ([30](handoff/integration-phase30.md)): favorites with their own cap; a server playlist hides tracks the viewer cannot reach; hand edits win over "Rescan tags"; the `deny.toml` ignore of RUSTSEC-2024-0436 (paste, through lofty); where the music library opens from.
- Jukebox ([31](handoff/integration-phase31.md)): `jukebox.vote_skip_percent` (50) or a decimal ratio; a track a listener cannot read shows its length; listening-room titles visible to everyone who can view the channel; rooms from a member's bucket.
- Drawer ([32](handoff/integration-phase32.md)): a second drawer key; Go to outside the Lounge; WebM voice messages; a server-side recovery copy of the notes passphrase; secret dice rolls; server loot tables readable by every member; shared time zones; the ECB allow-list line; English-only calculator phrases; card reminders versus DM settings; purging old invite cards; "Open in studio" in the message menu.
- Entrance ([30b](handoff/integration-phase30b.md)): an OAuth2 bearer token reading `openapi.json`; the gate's `429` bucket as a setting; the `house-notes` copy; `/app` remembering the last section.
- Beta checkpoint ([34](handoff/integration-phase34.md)): Caddy proxying `/~*`; Now playing only for signed-in viewers; `platform.public_profiles` for the beta; rings on by default and the rolling allowance of 20; scrapbook grants, reactions in private channels and a per-server cap; `media-src https:` versus listed hosts, PeerTube parties and non-Safari HLS; which player-skin extras come next.
- Not decided and not asked: game servers (D-383), the node agent's address notice, CurseForge and the EULA click (Q24 and Q26).

## 5. Downloads waiting for a yes

An agent downloads nothing without your word. Each row says what the download is and what it unblocks. Nothing in the feature phase was downloaded outside what you approved.

- [ ] **ffmpeg on the GitHub runner** (`apt-get install ffmpeg` in a workflow step; nothing on your machine). **Unblocks:** the media proofs of wave 22 (the transcode, playback and Live TV gate). This is the one download the plan depends on.
- [ ] **The `electrs` container image** for the `payments` profile (pinned by digest; see the Bitcoin row above). **Unblocks:** self-hosted invoice addresses.
- [ ] **Upstream licence texts** for the placeholders in `THIRD_PARTY_NOTICES.md` (the FreeType year, the NTP and Zlib holders; the note in that file names the upstream file each text comes from). **Unblocks:** a notices file with no placeholder. The alternative is to keep the "To complete" note.
- [ ] **tantivy 0.27 and its companions** (`cargo update -p tantivy`, approved as DEP-FU-001), **on or after 2026-11-17**. **Unblocks:** removing three `deny.toml` ignores.
- [ ] **SNES and Genesis cores.** There is no allowed prebuilt source: either name one (a GPL core's WebAssembly build with a licence you accept) or approve an Emscripten build wave. **Unblocks:** SNES and Genesis in the emulator.
- [ ] **A homebrew ROM or two** (candidates: Nova the Squirrel, 262 KB, from `github.com/NovaSquirrel/NovaTheSquirrel/releases`; Solar Guard, 4.2 MB, from `github.com/Deft-Spade/Solar-Guard/releases`; Super Tilt Bro is built from source). None is downloaded; each needs your yes and a licence check, then it is committed under `apps/web/public/emu/roms`. **Unblocks:** a game to play on a fresh instance.
- [ ] **An HTTP client for `sr-cli emu fetch-cores` inside the image** (`reqwest`, a new dependency; it also needs a `deny.toml` wrapper entry). **Default:** no; the command uses `curl` on the host or a dev machine and the distroless image has none, so cores are fetched by `deploy/scripts/fetch-emu-cores.sh` after the build. **Unblocks:** fetching cores from inside the container.
- [ ] **The emulator core files themselves** (7 files, 7.6 MB, pinned by SHA-256 in `deploy/emu-cores.lock`): fetched by the operator at install by `deploy/scripts/fetch-emu-cores.sh` or in CI by pinned digest; never committed. Already approved in the plan; listed so the operator knows it is a step. **Unblocks:** the Games page working.
- [ ] **Docker images for the game servers** (deferred, D-383): the signed presets' images by digest and `alpine` for `node-agent.yml`. Not asked until wave 38 is resumed.
- [ ] **Optional: ffmpeg on a development machine** (human action 5, D-269). **Unblocks:** running the pipeline and the TV slate locally. Not required.
- [ ] **Optional: gitleaks in CI.** **Unblocks:** a second secret scanner; wave 21 ships a no-download scanner first.
- [ ] **Optional: discord.py and twilight** for the compatibility suite (each with a licence check). **Unblocks:** wider SDK coverage.
- [x] ~~**Optional: a Node WebRTC library for TypeScript SDK voice.**~~ Done: `werift` 0.25.0 has been in `packages/bot` since wave 22 (struck in the phase 25 integration).
- [ ] **Optional: one GPU rental** to run the NVENC path of `sr-transcoder` once (`deploy/TRANSCODER.md`). **Unblocks:** the proof of the hardware path; the CPU path in CI is enough for the plan.

## 6. Development machines

- [ ] **Install pnpm:** `corepack enable && corepack prepare pnpm@10.12.1 --activate` on every machine (D-308). **Unblocks:** dropping the `npx pnpm@10.12.1` workaround.
- [ ] **Optional: review a sample of the Markdown corpus against Discord** (100 cases, OQ-15). **Unblocks:** replacing the remaining guessed expectations. Not launch-blocking (D-308).
- [ ] **Check CSP report delivery from an ordinary browser once** (D-365, RDY-SEC-009). With `sr-server` and Vite running, open the dev site in your everyday browser (not Playwright: the Reporting API sends nothing under automation), sign in as an operator, run `document.body.insertAdjacentHTML('beforeend','<img src="https://blocked.example.test/x.png">')` in the console, wait up to a minute, then open Admin > System "Blocked content reports" (or `select * from csp_report_counters`) and look for the `img-src` row with origin `https://blocked.example.test`. Ignore the `style-src` rows: the dev page's Report-Only policy produces those by itself. **Unblocks:** the one unproven link of RDY-SEC-009 (a browser reaching `/internal/csp-report` through the proxy); the server side is already proven by tests. One minute, no download.
