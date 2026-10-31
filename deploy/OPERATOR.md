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
| `compliance.operator_contact` | unset | The address the copyright and takedown notices go to; published on `/help`. |
| `compliance.data_request_deadline_days` | 30 (7 to 90) | Due date of a data-subject request. |
| `compliance.invoice_retention_years` | 10 | Years an invoice stays linked to the member after the year of payment. |
| `moderation.strike_review_threshold` | 3 | Upheld takedowns in the window that open a review case (never an automatic suspension). |
| `moderation.strike_window_months` | 12 | The window for the line above. |

The values for the lawyer-dependent ones are open questions: [lawyer brief](../docs/legal/lawyer-brief.md). The defaults stand until the lawyer answers.

**Limits and abuse protection (Admin > Limits & settings)**

| Key | Meaning |
|---|---|
| `accounts.cap` | The most active accounts the instance accepts. |
| `limits.rate.ip`, `limits.rate.session`, `limits.rate.bot`, `limits.rate.messages`, `limits.rate.uploads` | Live HTTP rate limits per minute; a change applies on the next request. Sudden 429s are the limiter, not an outage. |
| `abuse.*` (for example `abuse.friend_requests_per_hour`, `abuse.dm_opens_per_hour`, `abuse.data_export_cooldown_hours`) | Limits on member-writable surfaces, group `abuse`, listed in doc 05 "Abuse limits". |
| `retention.deleted_message_days`, `retention.large_file_days` | How long deleted messages and large files stay before the purge. Lower them only on purpose. |
| `invites.suspicion_freeze_threshold`, `invites.suspicion_escalate_threshold` | When a banned invitee freezes the inviter's invites and when a case opens. |
| `moderation.hold_review_days` | A hold older than this is flagged on the holds list (default 90). |

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

Every module of the feature phase has a platform switch (and, where it makes sense, a per-server one under Server Settings > Modules): `lounge.jukebox`, `lounge.listening_rooms`, `platform.tools` (and one `platform.tools.<tool>` per drawer tool), `platform.activities_datachannel`, `platform.emulator`, `platform.watch_together_embeds`, `platform.byos`, `platform.bridge`, `lounge.scrapbook`, `lounge.rings`, `platform.public_profiles` and `platform.game_servers` (which only shows or hides the "Coming soon" page). A switched-off module's routes answer 403 `SR_FEATURE_DISABLED`. No switch can turn off call encryption, authorization or the audit and access logs.

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
| `SR_PEERTUBE_HOSTS` | empty | Comma-separated PeerTube hosts that get a "Play here" button; set the same value for `sr-server` and for the web build (it widens the page policy's `frame-src`). YouTube and Vimeo (privacy-enhanced) are always the two embed hosts. |

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

The instance ships no game and no BIOS. The core files are fetched at install by `deploy/scripts/fetch-emu-cores.sh` (or `sr-cli emu fetch-cores` on the host, which needs `curl`) from a list pinned by SHA-256 in `deploy/emu-cores.lock`, into `<web root>/emu/cores`, after the web build; they are served to a signed-in session only. Without them the Games page says the core is not installed. `/games` documents alone get a page policy with `blob:` in `script-src`; no setting changes that. Members bring their own ROMs (and a PlayStation BIOS) in `emu/` of their own bucket or of a server shelf with the `view_games` grant.

**Search engines and the front door**

Only the landing page is indexable (D-384); there is no setting that changes it, and `apps/web/public/robots.txt` and the `X-Robots-Tag` headers come with the build and the Caddyfile. The signed-out pages are a separate build and the server serves the web app itself (`SR_WEB__ROOT`; D-385, [UPGRADE.md](UPGRADE.md)). `sr-cli preflight` shows the `web` row.

**Jobs added by the feature phase** (Admin > System > Jobs lists them; a singleton that did not run shows red)

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
