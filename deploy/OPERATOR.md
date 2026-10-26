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
