# Operator handbook

The one page to start from. It says which document answers which question, what to check every day, week and month, which settings an operator must know, and what each tile on Admin > System means. It repeats no procedure: the procedures live in the pages it links, and each of those is reachable from here.

Status: the shipped files were rehearsed in CI (`.github/workflows/prod-smoke.yml`), not yet on a real host. The playbooks in the runbook are marked "walk pending" until someone follows them on a running stack. A mistake you find belongs in the page that has it.

## 1. Which page for which question

| You want to | Read | Notes |
|---|---|---|
| Install from nothing | [INSTALL.md](INSTALL.md) | Both routes: Docker Compose and the static binary with systemd. |
| Go from an installed box to the first invites | [GO-LIVE.md](GO-LIVE.md) | Nine steps, each with a command and a "done when". |
| Upgrade, or roll back | [UPGRADE.md](UPGRADE.md) | Back up first; migrations run before the new server starts. |
| Back up, restore, recover to a moment, rotate the secret key | [BACKUP.md](BACKUP.md) | Dump, base backup plus WAL archive, restore drill. |
| Something is on fire, or an alert fired | [RUNBOOK.md](RUNBOOK.md) | One playbook per alert and per incident, see section 5. |
| Set up or grow object storage | [GARAGE.md](GARAGE.md) | Single node, three nodes, CORS rule. |
| Run video transcoding | [TRANSCODER.md](TRANSCODER.md) | The worker, its token, GPU notes. |
| Turn on the emulator | [EMULATOR.md](EMULATOR.md) | Fetch the pinned cores, verify their SHA-256, licences, no SNES or Genesis yet. |
| A quick answer (ports, metrics, 2FA lost, telemetry) | [FAQ.md](FAQ.md) | Short, with pointers. |
| What the owner and lawyer still have to do | [owner checklist](../docs/dev/owner-checklist.md), [lawyer brief](../docs/legal/lawyer-brief.md) | Not operator duties, but they gate go-live. |
| Why the system works the way it does | [doc 12](../docs/12-deployment-and-operations.md), [doc 09](../docs/09-retention-and-privacy.md) | Policy, retention, hardening checklist. |

Commands you will use everywhere: `sr-cli preflight` (is this box ready), `sr-cli upgrade --dry-run` (what a release would change), `sr-cli verify-access-log` (is the staff log intact), `sr-cli staff list`. `sr-cli --help` lists the rest, and Admin > Command line shows the emergency commands with what each does.

## 2. Daily, weekly, monthly

Five minutes a day, once the box is past its first fortnight.

**Daily**

- Admin > System: no red badge under Readiness probes, "Dead jobs" is 0, the Backups card shows a "Last backup" from the last 36 hours, the Disk space card has room. Each red tile links to its playbook (section 4).
- The Staff Server `#alerts` channel: read and acknowledge what is there. The health job raises disk, backup and transcode alerts there, and the payment watcher and the invite-suspicion rules add theirs.
- Admin > Moderation > Cases: anything open and unclaimed. A takedown notice has a one-business-day triage target.

**Weekly**

- Admin > System > Backups: "Last restore check" is from this week (the Sunday timer runs `restore-drill.sh`). A stale time is the playbook "Restore drill failed".
- Admin > Data requests: nothing close to its due date.
- Admin > System > "Blocked content reports": counts steady. A jump after a deploy is a policy that blocks a feature.
- A glance at Grafana ("SIDE ROOM overview"): error ratio, REST p95, database pool, job outcomes.
- A copy of the dumps and the WAL archive exists off the host (Storage Box), and you know where `server.secret_key` is stored.

**Monthly**

- Read the release notes and upgrade ([UPGRADE.md](UPGRADE.md)). Run `sr-cli upgrade --dry-run` first.
- Do a real restore into a scratch database, not only the drill: `createdb`, `deploy/scripts/restore.sh <dump> <url>`, look at the newest message, drop it. A backup nobody has restored is a hope.
- `sr-cli verify-access-log` and `sr-cli preflight`; read every warning.
- Admin > Staff & roles: everyone with a role still needs it. Admin > Invites and the account cap (`accounts.cap`) against the gauge on the Dashboard.
- Disk growth: compare the Disk space card with last month; extend before `alerts.disk_percent` fires, not after.
- Credentials you control: the mail relay password and the object storage key still work (`sr-cli preflight` tries both). The TLS certificate renews itself (Caddy).

## 3. The settings an operator must know

Settings live in Admin > Limits & settings (or `PATCH /admin/settings`) and Admin > Features (feature settings need a reason and are logged). The keys below were checked against the code. Anything not named here keeps its default.

**Who may join and who is responsible (Admin > Features > Compliance, a reason is required)**

| Key | Default | Meaning |
|---|---|---|
| `accounts.minimum_age` | 13 | Minimum age asked at registration and shown on the invite landing. |
| `compliance.operator_country` | unset | Two-letter country code; the privacy page names it. Set it before the first invite. |
| `compliance.operator_contact` | unset | The address the copyright and takedown notices go to; published signed out on the house notes (`/house-notes`) and the sign-in page, and in the handbook (`/help`, signed-in members only, D-385). |
| `compliance.data_request_deadline_days` | 30 (7 to 90) | Due date of a data-subject request. |
| `compliance.invoice_retention_years` | 10 | Years an invoice stays linked to the member after the year of payment. |
| `moderation.strike_review_threshold` | 3 | Upheld takedowns in the window that open a review case (never an automatic suspension). |
| `moderation.strike_window_months` | 12 | The window for the line above. |

The values for the lawyer-dependent ones are open questions: [lawyer brief](../docs/legal/lawyer-brief.md). The defaults stand until the lawyer answers.

**Limits and abuse protection (Admin > Limits & settings)**

| Key | Meaning |
|---|---|
| `accounts.cap` | The most active accounts the instance accepts. |
| `guilds.max_per_user`, `guilds.max_members` | The most servers one account may be in, created or joined (default 200, 1 to 1,000; the Staff Server does not count, bots are not limited) and the most members one server may have, bots and pending members included (default 25,000, 2 to 1,000,000; the Staff Server has no limit). At a limit a create or a join is refused with Discord's codes 30001 and 30019; a change applies to the next request. **Lowering a limit removes nobody:** an account over it keeps its servers and cannot create or join another; a server over it keeps its members and admits nobody until it is under. A one-community instance may set the first to 1. |
| `uploads.standard_limit_bytes` | The standard attachment size in bytes (default 30 MiB; 1 MiB to 500 MiB, the system maximum). It is the free upload size, and the size above which an attachment is "large": lowering it lowers what free members may upload, and a member with a perk (Patron, server-level) gets the larger of the perk's size and this one, never less than this one (D-678). A change applies to the next upload. A value stored by an older version above 500 MiB reads as the default. |
| `limits.rate.ip`, `limits.rate.session`, `limits.rate.bot`, `limits.rate.messages`, `limits.rate.uploads` | Live HTTP rate limits per minute; a change applies on the next request. Sudden 429s are the limiter, not an outage. |
| `abuse.*` (for example `abuse.friend_requests_per_hour`, `abuse.dm_opens_per_hour`, `abuse.data_export_cooldown_hours`) | Limits on member-writable surfaces, group `abuse`, listed in doc 05 "Abuse limits". |
| `retention.deleted_message_days`, `retention.large_file_days` | `retention.deleted_message_days` (default 30) is how long staff can read a deleted private message and when a released hold is purged. `retention.large_file_days` (default 7, 1 to 365) is the lifetime of a large attachment (above `uploads.standard_limit_bytes`) from its upload; a change applies to uploads from then on and leaves attachments already stored alone (docs/09). Lower the first only on purpose. |
| `tv.footer_note` | A line at the foot of the Live TV guide in place of the built-in text (empty by default, at most 200 characters). Plain text, no links. Applies within seconds. |
| `invites.suspicion_freeze_threshold`, `invites.suspicion_escalate_threshold` | When a banned invitee freezes the inviter's invites and when a case opens. |
| `moderation.hold_review_days` | A hold older than this is flagged on the holds list (default 90). |

**Activity packages (Admin > Limits & settings, group activities; D-654 to D-657)**

The zip of an Activity no longer has a fixed 32 MiB limit. The limit is the largest of the tiers an application qualifies for, and applies to the zip and to the unpacked total.

| Key | Default | Meaning |
|---|---|---|
| `activities.package.max_mb` | 512 (32 to 1 048 576) | Largest package any developer may upload, in MiB. Raising it to the top gives everyone the big limit. |
| `activities.package.reviewed_max_mb` | 4096 (same range) | Largest package for an application that has a staff-approved bundle. |
| `activities.package.platform_max_mb` | 32768 (same range) | Largest package for an application owned by the Owner or by staff who hold `REVIEW_APPS`. |
| `activities.package.max_files` | 20000 (2000 to 200000) | Most files and folders in one package. |

The zip goes from the browser to the platform's bucket in parts (a resumable upload), and the job `activities.package_ingest` checks it from the bucket, reading the zip index by range before any file is inflated; nothing is held in memory. A zip of at most 32 MiB may still be sent in one request. The unpacked total may be at most 100 times the zip (a code constant). A guild cartridge has no size limit; only its index and `sideroom.json` are read. Activity files are served with range support (`Accept-Ranges`, `206`, `416`).

**Invites (Admin > Invites; the quotas are edited in Admin > Limits & settings, group invites)**

| Key | Default | Meaning |
|---|---|---|
| `invites.locked` | off | Lock the Doors: nobody registers, whatever the invite (emergency invites included). Change it on Admin > Invites (needs Manage features and a reason; each change is logged as `invites.lock` / `invites.unlock`). Existing members are unaffected; the landing page says the doors are closed. |
| `invites.default_credits` | 5 (0 to 1000) | Invite credits a new account starts with (was `invites.initial_credits` before phase 39; migration 1890 moved a saved value). |
| `invites.max_outstanding` | 20 (1 to 1000) | The most unused codes one account may hold. Lowering it keeps existing codes. |
| `invites.unused_code_days` | 30 (1 to 3650) | An unused, non-emergency code older than this is revoked by the `invites.prune` job. |

The job `invites.prune` runs every six hours: it revokes the old unused codes above and marks, for Admin > Invites, the subtrees whose root never activated (email never verified and never seen online). It never deletes or changes the invite graph. Its last pass is shown on Admin > Invites.

**Public profiles (Admin > Limits & settings, group privacy, and Admin > Features)**

| Key | Default | Meaning |
|---|---|---|
| `profiles.access` | `walled` | `walled` keeps every profile inside the instance. `opt_in_public` lets members who choose it have a public profile page, but only while the feature `platform.public_profiles` is also on. Both are needed. |

**Module switches (Admin > Features)**

Every module of the feature phase has a platform switch (and, where it makes sense, a per-server one under Server Settings > Modules): `lounge.jukebox`, `lounge.listening_rooms`, `platform.tools` (and one `platform.tools.<tool>` per drawer tool), `platform.activities_datachannel`, `platform.emulator`, `platform.watch_together_embeds`, `platform.byos`, `platform.bridge`, `lounge.scrapbook`, `lounge.rings`, `platform.public_profiles` and `platform.game_servers` (which only shows or hides the "Coming soon" page). Wave 41 (the Study, D-400, D-404) added six platform switches, all **off by default**: `platform.external_metadata` (nothing on the server calls TMDB, AniList, MangaDex, MangaUpdates or ComicVine until it is on), `platform.storefront_categories` (the Cinema's Movies, Series and Anime browse pages), `platform.global_trending` (trending rails and rankings), `platform.requests_board` (the global requests board), `platform.ingest_queues` (the staff review queues and the member upload and submission pages that feed them; their routes answer 404 while it is off) and `platform.showroom` (wave 45, D-435: a "Preview populated state" button that fills the client with invented sample servers, shelves and games, for beta and test deployments; nothing in it is saved or sent). Wave 64 added `platform.beta_reports` (D-647), **off by default**: the beta bug reporter, see "The beta bug reporter" below. A switched-off module's routes answer 403 `SR_FEATURE_DISABLED` (the bug reporter and a few other modules answer 404 as a missing page, as `platform.requests_board` does). No switch can turn off call encryption, authorization or the audit and access logs.

**Alerts and what the System page watches**

| Key | Default | Meaning |
|---|---|---|
| `alerts.disk_percent` | 80 | Alert when the watched disk is this full. |
| `ops.disk_path` | empty | The directory whose disk is watched; point it at the Postgres and Garage volume. |
| `ops.backup_dir` | empty | The backup folder. Empty switches the backup alert and the Backups card off. |
| `alerts.backup_max_age_hours` | 36 | A backup older than this raises an alert. |
| `alerts.email_to_owner` | off | Also mail each alert to the Owner (needs SMTP). |

**Voice (configuration file or environment, not Admin)**

`voice.egress_budget_mbps` (`SR_VOICE__EGRESS_BUDGET_MBPS`), `voice.max_participants` and `voice.max_room_size`; see the playbook "Voice egress".

**Gateway (configuration file or environment, not Admin)**

| Key | Default | Meaning |
|---|---|---|
| `gateway.max_connections` (`SR_GATEWAY__MAX_CONNECTIONS`) | 10000 | Gateway sockets at once, identified or not; further upgrades get 503. |
| `gateway.max_unidentified_per_ip` (`SR_GATEWAY__MAX_UNIDENTIFIED_PER_IP`) | 32 | Sockets one client address may hold before they sign in (IDENTIFY, within 30 s); further upgrades get 429 (D-556). Raise it when many members reach the server from one address: a shared NAT, or a proxy whose forwarded address the server does not trust. |

When a gateway pump misses events (the event bus lagged) it rebuilds its state and then re-sends state to the affected sessions on the same socket (a fresh READY and GUILD_CREATEs, or an `SR_RESYNC` marker); sessions keep their id, so voice and calls continue (D-646). There is no setting.

**Environment you will meet:** `SR_BACKUP_DIR`, `SR_BACKUP_KEEP` (default 14,8,12), `SR_DRILL_MAX_AGE_HOURS`, `SR_BASEBACKUP_KEEP`; secrets are `server.secret_key` (rotation: [BACKUP.md](BACKUP.md)), database and storage credentials. Secrets go in the environment or a mode-0600 file, never on a command line.

### The feature phase: settings, switches and jobs (waves 27 to 40)

Everything below is also a switch in Admin > Features (see "Module switches" above), so an operator who does not want a module can leave it off or kill it; a killed module's routes answer 403 `SR_FEATURE_DISABLED`. The member-facing explanation of each is in Help > Operators (`/help/operators`).

**Bring your own storage (Admin > Limits & settings, group storage; `platform.byos`)**

| Key | Default | Meaning |
|---|---|---|
| `byos.who_may_mount` | empty (all) | Who may connect a bucket of their own: `all`, `patrons` or `staff`. A server's own buckets follow its Manage Storage grant instead. |
| `byos.endpoint_allow` | empty | Hosts a bucket endpoint may use (`s3.example.com`, `*.example.com`). Empty: any https endpoint reachable from the public internet; private addresses are always refused. |
| `byos.max_mounts_per_user`, `byos.max_mounts_per_guild` | 3 and 5 | Buckets one member or one server may connect. Lowering keeps the ones already connected. |
| `byos.presign_ttl_secs`, `byos.presign_ttl_cap_secs` | 900 and 3600 | Lifetime of a presigned read link and its ceiling (never above 86400, never longer than the caller's session). |
| `byos.upload_parts_per_hour` | 2000 | Part URLs one member may request per hour across their buckets. |
| `abuse.storage_tests_per_hour` | 20 | Connection tests of a mount per member per hour. |
| `byos.require_client_remux`, `byos.client_convert_max_mib`, `byos.worker_for_members` | off, 2048, off | The browser converter: ask for a remux before upload, the largest file it accepts, and whether the operator's transcoder workers convert members' videos inside their own buckets. |

The operator's own bucket is mount 1 (seeded from `storage.*` at first start); attach it to a server or detach it with `POST /admin/storage/mounts/1/attach` and `/detach` (the OPERATOR permission, audited). The daily job `storage.cors_recheck` (id 134) re-checks every mount's CORS rule; `storage.index` (id 1450) refreshes the per-mount indexes. A mount whose CORS check has not passed serves nothing. The runbook for a member whose bucket fails is the connect screen's test table; the operator never holds a member's bucket keys in the clear (they are sealed under `server.secret_key`, and `sr-cli rotate-secret-key` re-seals them).

**Response types of bucket links (D-629, D-637, D-641 to D-645; no setting)**

A link SIDE ROOM signs to an object in a member's or a server's bucket never opens as a page. A file whose extension is on the app's inline media list is answered with that media type; every other file is answered as `application/octet-stream` with `Content-Disposition: attachment`. This holds for `/cdn/mount`, the shelf object URL, the profile showcase, the storage presign route and the routes that use it (music tracks, jukebox, scrapbook) and shelf artwork. A browser-direct upload to a mount stores HTML, XHTML, SVG, XML, script types and every `text/*` type other than plain text, WebVTT and CSV as `application/octet-stream`. A mount's connection test checks that the provider honours the response overrides (row `response_type`); a provider that ignores them, or a check that cannot be completed, fails the test and the mount is `failing`, and a mount whose stored test has a failed `response_type` row hands out no presigned GET. The daily `storage.cors_recheck` also runs this check on every live mount that has no such row or a failed one. Objects put into a bucket outside the app stay as their owner stored them.

**The beta bug reporter (`platform.beta_reports`; D-647 to D-650)**

Off by default. While it is on, a signed-in member can open "Report a bug" (default key Alt+Shift+B, in the "?" help menu and on the "You" tab of a phone); the report goes to the platform staff. The reporter may be on for the closed beta (under 100 accounts) before the lawyer has answered Q27; from 100 accounts on it stays off until the answer is in and the privacy notice says what the answer requires (D-684). It stays your switch: nothing turns it off by itself; from 100 accounts on, the notice on Admin > Bug reports (D-665) is the reminder to switch it off. Turn it off when the beta ends. While it is off, `POST /bug-reports` answers 404 and the Admin page "Bug reports" (group Overview) is hidden; the staff routes `/admin/bug-reports*` stay open, so staff can read and close reports after the beta. The member sees the exact picture and debug information before sending. At most 5 reports per account per hour and 100 per day for the platform, 3 MiB per request, 2 MiB per picture. Any appointed staff member can list, open and close reports; opening a report, opening its picture and closing it each write a platform audit entry. The picture is deleted when the report is closed or after 30 days, the report after 90 days (the daily job `bugs.retention`, id 2550). Erasing an account deletes its reports, and the data export includes them.

**Music, the jukebox and the drawer (`platform.byos` for the shelves; `lounge.jukebox`, `lounge.listening_rooms`, `platform.tools`)**

| Key | Default | Meaning |
|---|---|---|
| `music.max_playlists`, `music.max_items` | 100 and 5000 | Playlists one member or server may keep; tracks per playlist or favorites. |
| `jukebox.vote_skip_percent` | 50 | Percent of the listeners whose skip votes skip a track. |
| `tools.currency_rates` | on | The daily fetch of the European Central Bank reference rates (the only outbound request of the drawer). Off: no request, the converter is hidden. The server needs to reach `www.ecb.europa.eu`; an instance with an outbound allow list adds that host. |
| `tools.currency_stale_hours` | 96 | When the converter says its rates are out of date. |
| `voice.datachannel_kbps` (configuration file or `SR_VOICE__DATACHANNEL_KBPS`) | 256 | Per-member budget of the activity data channel in kbit/s; `0` turns the channel off. |

The voice jukebox adds no media storage: listeners play files from the mount they may read, and a DJ's stereo track rides the encrypted call (a second audio track, see the playbook "Voice egress" for the gauge `sr_voice_music_egress_bits_per_second`).

**Status rings, the scrapbook, profiles and embeds**

| Key or switch | Default | Meaning |
|---|---|---|
| `lounge.rings` (platform switch) | off | Status rings. A ring is a 24-hour photo, clip, line or track on the avatar; files go to the author's own bucket or, without one, to the instance bucket under `rings/`. |
| `lounge.scrapbook` (platform switch, then per server) | off | The scrapbook vault. Offer it at platform scope and each server owner chooses a shelf for it. |
| `platform.public_profiles` and `profiles.access` | off and `walled` | Both are needed for any public profile page (P39-FLG-1). Pages are never indexed. |
| `SR_PEERTUBE_HOSTS` | empty | Comma-separated PeerTube hosts that get a "Play here" button; at start the server also adds them to the app policy's `frame-src` and `connect-src` (no web rebuild). YouTube and Vimeo (privacy-enhanced) are always the two embed hosts. Watch Together resolves a PeerTube video through the instance's API (D-515). Each entry is a plain host name such as `tube.example.org` (no scheme other than an optional `https://`, no path, no wildcard; a port is ignored). An entry that is not one is ignored, and the server says so once at start in a `warn` line that names the variable and the entry (`sr-cli preflight` shows the same in its `web` row); it never stops the start (D-697). |
| `SR_MEDIA_HOSTS` | empty | Comma-separated hosts a listed PeerTube instance serves files and HLS segments from (object storage, a CDN). A party plays a resolved address only on the instance or one of these, and at start the server adds them to the app policy's `connect-src` (for hls.js). Same rules as `SR_PEERTUBE_HOSTS`: an entry that is not a plain host name is ignored with a warning at start (D-697). |
| `SR_CSP_CONNECT`, `SR_CSP_FRAME`, `SR_CSP_MEDIA` | empty | Extra origins for the app's Content-Security-Policy: a storage bucket or another host the app page itself fetches from (`SR_CSP_CONNECT`), another frame host (`SR_CSP_FRAME`), an image or media CDN (`SR_CSP_MEDIA`, which goes to `img-src` and `media-src`). Comma-separated, each `scheme://host[:port]` with scheme `https` or `wss` (`http` or `ws` for a loopback host only: `localhost`, `127.0.0.1`, `[::1]`; a LAN address over `http` is refused, because the app needs a secure context for its service worker and end-to-end encryption, which such an address cannot give, D-698) and an optional leading `*.`. Read by the server at start; see "The app's Content-Security-Policy origins" below. |

#### The Discord bridge (D-381, D-386)

The bridge is off on every server. Only a server owner switches it on, under Server Settings > Modules > Discord bridge, pastes the bot token and chooses a mode (bridge named channels, or the whole server). As operator you can kill it for the whole instance with `platform.bridge`: every server's bridge is then torn down (webhooks deleted on Discord, maps and token wiped) and stays off.

| Key | Default | Meaning |
|---|---|---|
| `bridge.max_bridges` | 10 | Gateway connections (one per bridged server) this instance runs at once; a server that switches one on beyond it is refused. |
| `abuse.bridge_messages_per_minute` | 60 | Messages one bridge may bring in from Discord per minute; the rest are dropped and counted. |
| `bridge.delete_hold_hours` | 24 | Whole-server mode: how long a channel deleted on Discord is held before it is deleted here (0 deletes at once). |
| `bridge.reconcile_minutes` | 15 | Whole-server mode: minutes between full comparisons with Discord. |
| `bridge.struct_writes_per_sec` | 2 | Whole-server mode: role, channel and thread writes per second the bridge may make to Discord. |
| `SR_BRIDGE_SUPERVISE_SECS` (environment) | 30 | How often the shards are matched to the database. `SR_BRIDGE_DISCORD_API` and `SR_BRIDGE_DISCORD_GATEWAY` point the bridge at a stand-in Discord for tests; leave them unset in production. |

What to tell a server owner who wants the bridge: create a Discord application with a bot, enable the privileged intents MESSAGE CONTENT, SERVER MEMBERS and PRESENCE, invite the bot with the link the bridge screen shows (whole-server mode needs Administrator, because it creates roles and channels), then paste the token. A webhook that Discord would not delete on teardown is remembered by id on the screen so the owner can remove it by hand. The privacy notice for members is built in; the retention of the maps is in [doc 09](../docs/09-retention-and-privacy.md). The first run against a real Discord server is still on the owner's list ([owner checklist](../docs/dev/owner-checklist.md) section 4).

**The emulator (`platform.emulator`; [EMULATOR.md](EMULATOR.md))**

The instance ships no game and no BIOS. The core files are fetched at install by `deploy/scripts/fetch-emu-cores.sh` (or `sr-cli emu fetch-cores` on the host, which needs `curl`) from a list pinned by SHA-256 in `deploy/emu-cores.lock`, into `<web root>/emu/cores`, after the web build; they are served to a signed-in session only. Without them the Emulator (a card in the Arcade, opening at `/games/emulator`) says the core is not installed. `/games` documents alone get a page policy with `blob:` in `script-src`; no setting changes that. Members bring their own ROMs (and a PlayStation BIOS) in `emu/` of their own bucket or of a server shelf with the `view_games` grant.

**The Arcade, guest links and Sketch & Slander (waves 41 to 47; [doc 12](../docs/12-deployment-and-operations.md) section 2)**

The Arcade (`/games`) is the games front page; it has no setting of its own and needs `activities.enabled` for the house games to run. The house games are built and uploaded by the operator (`node scripts/build-activities.mjs <whiteboard|holdem|tabletop|landio|arena3d|sketch>`, then upload each zip as an application's Activity bundle and approve it). Admin > Arcade (the Operator only) features games and keeps the Coming soon list; the Featured section stays hidden until you pick something (D-474). A **Share link** on a running voice-channel game lets friends without an account play from a phone at `/play/<token>`: it needs Activities on, the release's Caddyfile (re-render it after upgrading) and a correct `server.trust_forwarded_for` (20 guest joins a minute per address, D-415). Sketch & Slander's **Guild Prompt Packs** come from a shelf the host may read (`platform.byos`); the member's page fetches the pack, so a bucket on another origin must be named in the web build's `SR_CSP_CONNECT`, as for emulator ROMs ([FAQ.md](FAQ.md)). Guests never receive a pack (D-397).

**Crash notes (wave 45, D-430 to D-433)**

Nothing to configure. A crash in a signed-in browser becomes one card in the staff-only `#crash-triage` channel of the Staff Server (one card per distinct crash, with a count) and a row under Admin > Overview > Crash reports. The note holds no messages, links, keys or typed text. Read it as a bug report: nothing is ever deployed from a card.

**Beta bug reports and the Staff Server feeds (wave 66)**

Admin > Bug reports warns while the beta bug reporter is on and the platform has 100 or more accounts; the notice links to Admin > Features, where the switch is `platform.beta_reports`. The rule behind the notice (D-684): the reporter may be on for the closed beta, under 100 accounts, before the lawyer answers Q27; from 100 accounts on it stays off until the answer is in and the privacy notice says what it requires. The Staff Server's `#new-members` and `#donations` channels are created at server start if missing; deleting one on purpose does not last past the next restart (D-683), so to silence a feed mute the channel or hide it from a role: that survives restarts. There is no setting that stops a feed.

**The app's Content-Security-Policy origins (SEC-AUD-210, D-682)**

The release web build carries the plain policy of the signed-in app (`app/csp.txt`). When `sr-server` starts it adds to it the origins its own configuration implies, by the rule the web build uses: `https://*.activities.<host>` in `frame-src` when `activities.enabled` is on and `server.public_url` is https (not in the test-only `same_host_sandboxed` mode); the host of each `voice.endpoints` entry when it differs from the site's (the voice origin comes from `voice.endpoints`, the list of addresses clients are handed; there is no separate voice URL setting, D-696), in `connect-src` as `https` and `wss` (or `http` and `ws`); the hosts of `SR_PEERTUBE_HOSTS` in `frame-src` and `connect-src`; the hosts of `SR_MEDIA_HOSTS` in `connect-src`; and your `SR_CSP_CONNECT`, `SR_CSP_FRAME` and `SR_CSP_MEDIA` origins. It only adds: nothing in the file is removed, an origin already there is not added twice (a build from source with the `VITE_*` values already holds them), and the public pages' policy, the guest-play page's and the legal page's are never touched. The origins come from your configuration only, never from a request, and the header is built once at start: change a value and restart. The start-up log has one `app policy (...)` line listing what was added per directive, or saying no origins were added. A bad `SR_CSP_*` entry (a space, a semicolon, a quote, a path, a keyword such as `'unsafe-inline'`, a bare `*`) stops the server with a message naming the variable and the entry; fix it in `.env.prod` or `server.env`. A bad entry in `SR_PEERTUBE_HOSTS` or `SR_MEDIA_HOSTS` is different: it is dropped and the start goes on, with one `warn` line per variable naming the entries (D-697). The values are your own: an origin you list is allowed, so list only what you run.

**Search engines and the front door**

Only the landing page is indexable (D-384); there is no setting that changes it, and `apps/web/public/robots.txt` and the `X-Robots-Tag` headers come with the build and the Caddyfile. The signed-out pages are a separate build and the server serves the web app itself (`SR_WEB__ROOT`; D-385, [UPGRADE.md](UPGRADE.md)). `sr-cli preflight` shows the `web` row. App code for platform staff (the Admin app and everything only it loads) is built into `/app/staff/` and `sr-server` serves it only to a session with an active staff appointment, read from the database on every request: any other session gets 404, a request without a session the 401 of any app file, and the files are `Cache-Control: private, no-store` (D-651, D-652). There is no setting. The first-paint limit (250 KB gzip, with at least 8 KB of room) measures only what `app.html` loads for a member and is not raised (D-636, D-653). The wording of the Admin sections is in a staff catalog under `app/staff/`, served only to staff (D-663).

**Jobs added by the feature phase** (Admin > Jobs lists them; a singleton that did not run shows red)

| Job (id) | Runs | Does |
|---|---|---|
| `storage.cors_recheck` (134) | daily | Re-checks each mount's CORS rule. |
| `storage.index` (1450) | daily | Refreshes the mount indexes. |
| `jukebox.rooms` (1580) | every 2 s | Advances the cursor of each 24/7 listening room. |
| `tools.fx_refresh` (1620) | hourly check, one fetch per 20 h | Fetches the ECB rates when `tools.currency_rates` is on. |
| `tool_rooms.sweep` (1660) | hourly | Deletes tool rooms older than 7 days. |
| `cards.remind` (1670) | every 30 s | Sends invite-card reminders. |
| `scrapbook.requests` (1690), `scrapbook.removals` (1691) | 30 s and 5 min | Archives requested messages; retries bucket removals. |
| `rings.expire` (1710) | every 10 min | Deletes expired rings and their files. |
| `invites.prune` (1890) | every 6 h | Revokes old unused codes and marks inactive branches (never touches the invite graph). |
| `activities.package_sweep` (966), `activities.package_ingest` | daily; on demand | Expires Activity package uploads nobody finished (24 hours) and deletes session rows after seven days; checks one uploaded zip (D-654). |
| `bugs.retention` (2550) | daily, 03:40 UTC | Deletes bug report pictures after 30 days or on close, and reports after 90 days (D-650). |

The security gate of the phase is `security-nightly.yml` (the whole authorization matrix with modules killed); an operator running a fork reads its result in Actions. The wave-by-wave record is [docs/13](../docs/13-roadmap-and-phases.md).

## 4. What the tiles on Admin > System mean

Open Admin > System. Anything red or empty where it should not be carries a "Playbook" link to the section of [RUNBOOK.md](RUNBOOK.md) named below. Playbook opens the runbook inside Admin (Admin > Runbook, `/admin/runbook`: the copy shipped with this build); the file is also `deploy/RUNBOOK.md` in the install checkout.

| Tile | Healthy | When it is red or wrong | Playbook |
|---|---|---|---|
| Version | The release you installed. | A different version after an upgrade means the old process still runs. | [Upgrading](UPGRADE.md) |
| Gateway connections | A number close to the people online. | Zero while people are online: clients cannot hold a connection (check the proxy forwards `/gateway`, INSTALL.md step 6). | [Server down](RUNBOOK.md#server-down) |
| Jobs queued (and running) | A few, falling back to zero. | High and rising: workers are not keeping up. | [Job backlog](RUNBOOK.md#job-backlog) |
| Dead jobs | 0. | Jobs that ran out of attempts; Admin > Jobs lists them with the last error. | [Jobs failing](RUNBOOK.md#jobs-failing) |
| Readiness probes: database, storage | Both "OK". | A failing probe names the error: Postgres or Garage is down or misconfigured. | [Server down](RUNBOOK.md#server-down) |
| Preflight | Last run has no failed row. | A failed row says what to fix in a sentence; the same table is `sr-cli preflight`. | [Preflight](RUNBOOK.md#preflight-and-the-alert-path-test) |
| Blocked content reports | "None", or small steady counts. | A jump in one directive or origin after a deploy. | [CSP reports spike](RUNBOOK.md#csp-reports-spike) |
| Backups: last backup, last restore check, last object snapshot | Times within 36 hours (backup), a week (restore check). | "None" means the folder is not set (`ops.backup_dir`) or nothing ran. | [Backup missing or stale](RUNBOOK.md#backup-missing-or-stale), [Restore drill failed](RUNBOOK.md#restore-drill-failed) |
| Disk space | Free space beyond `alerts.disk_percent`. | Low or unreadable (the path does not exist for the server). | [Disk pressure](RUNBOOK.md#disk-pressure), [Disk full](RUNBOOK.md#disk-full) |
| Integrations: Email, sign-in with Discord, Voice, TURN, GIF search, Web Push, Linked accounts | "Configured" for what you turned on. | "Not configured" for a feature you rely on. The Email row has a test-mail button. | [GO-LIVE.md](GO-LIVE.md) step 7, [INSTALL.md](INSTALL.md) |

The Dashboard (Admin > Dashboard) adds "Storage used" ([Storage usage](RUNBOOK.md#storage-usage)), "Voice rooms" and "Voice egress" ([Voice egress](RUNBOOK.md#voice-egress)), the review-queue tiles and the account gauge against `accounts.cap`.

## 5. The playbooks

Every alert rule in `deploy/monitoring/alerts.yml` names its playbook in the `runbook` annotation, and `node scripts/check-ops-files.mjs` fails when one does not resolve. All of them are in [RUNBOOK.md](RUNBOOK.md); this is the index.

**Alerts and tiles**

[Server down](RUNBOOK.md#server-down), [High error rate](RUNBOOK.md#high-error-rate), [Slow requests](RUNBOOK.md#slow-requests), [Database pool exhausted](RUNBOOK.md#database-pool-exhausted), [Jobs failing](RUNBOOK.md#jobs-failing), [Job backlog](RUNBOOK.md#job-backlog), [Voice egress](RUNBOOK.md#voice-egress), [Invoice stuck](RUNBOOK.md#invoice-stuck), [Storage usage](RUNBOOK.md#storage-usage), [Disk pressure](RUNBOOK.md#disk-pressure), [Backup missing or stale](RUNBOOK.md#backup-missing-or-stale), [WAL archive lag](RUNBOOK.md#wal-archive-lag), [Restore drill failed](RUNBOOK.md#restore-drill-failed), [CSP reports spike](RUNBOOK.md#csp-reports-spike), [Suspicion case opened](RUNBOOK.md#suspicion-case-opened).

**Incidents**

[Takedown notice received](RUNBOOK.md#takedown-notice-received), [Data-subject request received](RUNBOOK.md#data-subject-request-received), [Suspected compromised staff account](RUNBOOK.md#suspected-compromised-staff-account), [Lost owner second factor](RUNBOOK.md#lost-owner-second-factor), [Disk full](RUNBOOK.md#disk-full), [Restore from backup](RUNBOOK.md#restore-from-backup), [A member reports CSAM](RUNBOOK.md#a-member-reports-csam) (steps that need the lawyer are marked and undefined until the owner's lawyer answers, D-336 (h)).

**Other**

[Lost or suspect secret key](RUNBOOK.md#lost-or-suspect-secret-key), [Locked-out owner](RUNBOOK.md#locked-out-owner), [Staff alerts and the second-factor window](RUNBOOK.md#staff-alerts-and-the-second-factor-window), [Preflight and the alert-path test](RUNBOOK.md#preflight-and-the-alert-path-test).

The Prometheus rules cover what the server exports as metrics: requests, latency, database pool, job outcomes, gateway sessions, and (RDY-OBS-001) the job backlog, backup, restore-drill and WAL-archive times, disk free ratio, stuck invoices, CSP report rate and voice egress. The health job (every 15 minutes), the invoice watcher and the CSP report handler set those gauges; each rule names its playbook. Suspicion cases have no gauge: the `#alerts` card when a case opens is the alert. A standalone SFU node is scraped by the `sr-voice` job in `prometheus.yml`. The Grafana overview has a panel per gauge.

## 6. When you are stuck

1. Admin > System, then the Staff Server `#alerts` channel, then the playbook.
2. The log line for a failing request: the browser or the member gives you `X-Request-Id`; `docker compose logs sr-server` (or `journalctl -u sr-server`) has it ([RUNBOOK.md](RUNBOOK.md) intro).
3. `sr-cli preflight` and `GET /readyz` (through the server, not the proxy).
4. If a document here is wrong, fix the document in the same change that fixes the problem.
