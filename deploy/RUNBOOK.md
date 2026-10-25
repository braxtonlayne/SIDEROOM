# Runbook

Short, ordered checks for the alerts in `deploy/monitoring/alerts.yml`, the red tiles of Admin > System and the incidents an operator meets. Start from [OPERATOR.md](OPERATOR.md) when you do not know where to look. Logs are JSON lines on stdout (`docker compose logs sr-server` or `journalctl -u sr-server`); every request line carries `request_id`, `method` and `path`, and the same id comes back to clients in `X-Request-Id`, so a user report with that header leads straight to the line.

Useful everywhere: `GET /healthz` (process is up), `GET /readyz` (Postgres and object storage reachable; failing checks are named in the body), `GET :9100/internal/metrics` (loopback or compose network only).

## Server down

1. `/healthz` from the host. Down: `systemctl status sr-server` or `docker compose ps`; read the last 100 log lines.
2. Crash loop with `secret_key` or config errors: the message names the key. Fix the environment, restart.
3. Starts but `/readyz` is 503: the body says which check failed. Postgres: is it up and accepting connections, is `database.url` right, does `sslmode` match. Object storage: is Garage up, are the key and bucket right.
4. Out of disk is the usual cause of both Postgres and Garage failing together: `df -h`, then see Disk pressure.

## High error rate

1. Grafana "Requests per second" split by status: which status, and the "Slowest routes" panel for which route.
2. Search the logs for the failing route and read the lines around the `request_id` of a 5xx response.
3. A deploy in the last hour? Roll back (UPGRADE.md).
4. Sudden 429s are the rate limiter, not an outage: see the "Rate limited" panel and `scope`.

## Slow requests

1. "REST latency" p95 and "Database pool": a pool at zero idle means queries are waiting for connections.
2. In Postgres: `select pid, now()-query_start as age, state, left(query,120) from pg_stat_activity where state <> 'idle' order by age desc;`
3. A single slow route points at a missing index: run `EXPLAIN (ANALYZE, BUFFERS)` on its main query (XC-PERF-004 tracks the review).

## Database pool exhausted

Long transactions or a leak. Same `pg_stat_activity` query; terminate the oldest idle-in-transaction session (`select pg_terminate_backend(pid)`), then raise `database.max_connections` only if Postgres has headroom (`show max_connections`).

## Jobs failing

The `jobs_processed_total{outcome="failed|dead"}` series names the `kind`. Failed attempts retry with backoff until `max_attempts`; then the job is `dead`. Read the rows (`select id, kind, attempts, last_error from jobs where state = 'dead' order by finished_at desc limit 20;`), fix the cause (SMTP down, object storage full), and set the row back to `queued` with `attempts = 0` once it is fixed.

## Disk pressure

1. `du -xh /var/lib | sort -h | tail`. Postgres WAL, Garage data, search index (`search.dir`) and the backup directory are the usual four.
2. Backups: lower `SR_BACKUP_KEEP` or move them to the Storage Box.
3. Never delete files inside the Postgres or Garage data directories by hand.

## Lost or suspect secret key

`server.secret_key` seals every TOTP secret and every application's interactions signing seed. If it leaked, or you only suspect it, rotate it; ask members to re-enroll TOTP only if a database dump leaked as well (the key alone opens nothing without the database). See BACKUP.md "Secrets rotation" for the backups.

This drill is run end to end by `drill_lost_or_suspect_secret_key` in `crates/sr-cli/tests/drills.rs` (Postgres); the commands below are the ones it ran.

1. Make a new key (64 hex characters, mode 0600 file or `SR_SERVER__SECRET_KEY_FILE`). Put the **old** value in `server.secret_key_previous` (`SR_SERVER__SECRET_KEY_PREVIOUS_FILE`, one key per line) and the new one in `server.secret_key`. Do not restart the server yet.
2. Re-encrypt everything under the new key:

   ```
   sr-cli rotate-secret-key
   ```

   Expected: `re-encrypted N TOTP secret(s) under the current key` and `re-encrypted M interactions signing seed(s) under the current key`. If you forgot step 1 the command stops with `re-encrypting TOTP secrets (is the old key still listed in server.secret_key_previous?)` and exit code 1, and changes nothing (one transaction).
3. Restart the server with the same configuration. Every TOTP secret and signing seed now opens under the new key alone.
4. Remove the old key from `server.secret_key_previous` and restart again. From then on the old key opens nothing in the database. Running `sr-cli rotate-secret-key` again is harmless.
5. Keep both keys with the backups made before the rotation: an older dump needs the key that was current when it was made.

## Compromised staff account

Use this when a staff member's password, session or authenticator may be in someone else's hands. It is run end to end by `drill_compromised_staff_account` and `drill_verify_access_log_detects_tampering` in `crates/sr-cli/tests/drills.rs` (Postgres); the commands are the ones they ran.

1. **End every session.** Admin > Accounts > the account > Revoke sessions (`POST /admin/accounts/{id}/force-logout`). The sessions stop working at once.
2. **Remove the second factors**, so that an authenticator the intruder enrolled cannot be used to come back. On the host:

   ```
   sr-cli user reset-2fa <user>
   ```

   Expected: `reset 2FA for account <id>: N TOTP, N recovery codes, N passkeys removed; N trusted devices and N sessions revoked`, a platform audit row `user.reset_2fa`, and a notice mail to the account's address.
3. **Take the staff roles away** until the person is back in control of the account:

   ```
   sr-cli staff list
   sr-cli staff revoke <user> --reason "compromised account"
   ```

   The account no longer appears in `staff list`.
4. **Check what the account did.** Admin > Access log filtered by the account, from the earliest time the credentials may have leaked. Then prove the log itself is intact:

   ```
   sr-cli verify-access-log
   ```

   Expected: `access log OK: N rows, head <hash>`. A broken chain prints `ACCESS LOG BROKEN at row N: ...` on stderr and exits 1 (an edited row reads `row contents do not match its hash`, a removed row `sequence gap`). Treat that as a database compromise, not an account compromise: stop, keep a copy of the database, and follow "Lost or suspect secret key" as well.
5. **Rotate what the account could read.** The account holder signs in again (password reset first, then a fresh authenticator) and replaces every bot token and client secret of their applications in the developer portal (`POST /applications/{id}/bot/reset-token` and `/reset-secret`, both ask for the password). The old token stops authenticating the moment the new one is issued. If the account was the Owner, also run the "Lost or suspect secret key" drill.
6. **Give the roles back** only after the person confirms the new authenticator: `sr-cli staff appoint <user> --role <Role> --reason "<why>"` (or the offer flow in Admin > Staff).

## Locked-out owner

The full playbook is [Lost owner second factor](#lost-owner-second-factor). In short: `sr-cli user reset-2fa <user>` removes TOTP, recovery codes and passkeys and revokes every session. `sr-cli invite` creates a platform invite without credits (it writes a platform audit row), and `sr-cli staff` manages appointments.

## Staff alerts and the second-factor window

The Staff Server's `#alerts`, `#new-members` and `#donations` channels are fed by the system user. A 15-minute health job raises an alert for a disk that is too full, a missing or stale backup, and each failed transcode job; the same problem is not raised again within a day. Tune it in Admin, Limits (group `alerts`), or `PATCH /admin/settings`:

| Setting | Default | Meaning |
|---|---|---|
| `alerts.disk_percent` | 80 | Alert when the watched disk is this percent full. |
| `ops.disk_path` | empty | Directory whose disk is watched; empty watches the server's working directory. Point it at the Postgres and Garage volume. |
| `ops.backup_dir` | empty | `SR_BACKUP_DIR` of `deploy/scripts/backup.sh`. Empty switches the backup alert off. |
| `alerts.backup_max_age_hours` | 36 | A backup (`*.dump` or manifest `*.json`) older than this raises an alert. |
| `alerts.email_to_owner` | off | Also email each alert to the Staff Server owner, at most one mail per kind every 30 minutes. Needs `[smtp]`. |

Sensitive admin actions (reading private content, changing an account's email, forcing a password reset, resetting 2FA, appointing or revoking staff, announcements) need a second factor checked on the session in the last 15 minutes. Staff without TOTP or a passkey cannot use them until they enroll. The full list of gated actions is in doc 07 section 6 (`SENSITIVE_ACTIONS` in `crates/sr-api/src/sensitive.rs`). For local development and end-to-end tests set `SR_ADMIN_REQUIRE_REAUTH=0` to switch the check off; the server refuses to start with it unless `server.public_url` is a localhost address (RDY-SEC-010).

## Upgrade note: legacy download fallback is off

`SR_DOWNLOADS_LEGACY_FALLBACK` is gone and migration 0871 drops `cinema_download_grants`: only entitlements grant the Patron download perk. **Upgrade order:** run a Phase 12 release first (its migration 0542 copied unexpired legacy grants into entitlements), check Admin > Entitlements that everyone who should keep downloads has a grant, then upgrade to this release. Migration 0871 refuses to run ("run a Phase 12 release first so entitlements are backfilled") when a legacy grant row is newer than the newest entitlement row, so a skipped release stops at startup instead of silently dropping the perk; the fix is to deploy the Phase 12 release, then retry. The guard cannot see a grant created after the last entitlement; if you added legacy rows by hand, add the matching entitlement in Admin > Entitlements first.

## Preflight and the alert-path test

`sr-cli preflight` answers "is this box ready for the first invite?" (RDY-OPS-004). It checks the database and migrations, an object storage round trip (put, get, presign, multipart create and abort), the mail relay (connect only), the public URL and its headers (HSTS, CSP, `frame-ancestors`), TURN (credentials derive and an allocation succeeds over UDP), the Web Push key, free disk space, clock skew against the database, secret file modes (0600 or 0400) and, when on, Esplora and the voice node. It prints a table of PASS, WARN and FAIL rows with a sentence each and exits 1 when any row fails; warnings do not fail it. `--json` prints the report as JSON, `--insecure-tls` accepts a private CA when fetching the public URL (the smoke stack), `--send-to you@example.org` also sends a test mail through the relay.

The same checks run from Admin > System ("Preflight" card, `POST /admin/system/preflight`, `OPERATOR`) and as the last step of the Setup checklist on the Admin dashboard; each run is written to the access log (`system.preflight`). Set `SR_PREFLIGHT_INSECURE_TLS=1` on the server to let that card accept a private CA; it is honoured only when `server.public_url` is a localhost address (`localhost`, `127.*`, `[::1]` or a `*.localhost` name), and on any other host `sr-server` refuses to start until it is unset (D-357). A run with `--insecure-tls` or that variable reports the public-URL row as WARN ("certificate not verified"), never PASS, because it did not check the certificate. The Esplora row fails on a private, loopback or link-local address (the self-hosted `http://electrs:3000`) until `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1` is set, the same rule `sr-payments` applies; with the opt-in the row passes and says "(private host, opted in)" (D-358).

`sr-cli preflight --alert-test` (RDY-OPS-006) posts a test alert through the real staff feed path: the alert is stored, a card that names the host, the instance and the UTC time is posted to `#alerts` of the Staff Server, and, when Admin > Settings has the Owner email for alerts on, the Owner is mailed. The table gets an "Alert card" and an "Alert mail" row. The command is a separate process, so connected clients see the card the next time they open the channel, not instantly. Without a Staff Server (no `bootstrap` yet) the card row fails.

## How to read the playbooks below

Every playbook from here to the end of the file has the same shape: what raised it, what to look at, what to do, how to know it is over. They are linked from the red tiles of Admin > System (the "Playbook" link under a failing probe, dead jobs, disk or backups), from the alert rules in `deploy/monitoring/alerts.yml` and from [OPERATOR.md](OPERATOR.md). `node scripts/check-ops-files.mjs` fails when an alert rule has no playbook here, when a tile points at an anchor that does not exist, or when a playbook names a metric the server does not export.

> **Status of the walk-through.** Each playbook below is _written against prod-smoke's documented stack; walk pending_. The commands were checked against the code (CLI definitions, setting keys, metric names, screens) but nobody has yet followed them end to end on a running stack. The first operator to meet one of these situations, or the owner during the go-live rehearsal ([GO-LIVE.md](GO-LIVE.md)), should walk it and replace this line with the date and the stack.

`dc` stands for `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod`, and `sr-cli` for the binary on the host (or `dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server`).

## Job backlog

_Raised by:_ Admin > System or Dashboard "Jobs queued" staying high, members reporting slow thumbnails, mails or transcodes. The Prometheus rule `JobBacklog` fires when `sr_jobs_oldest_queued_age_seconds` stays above 600 for 10 minutes (`sr_jobs_queued{kind}` says which kind); the health job sets both every 15 minutes. The queue is also on the Admin > System tile.

1. Admin > Jobs, state "Queued": which `kind` fills the queue, and whether "Next run" is in the past (a worker is not picking them up) or in the future (they are waiting out a retry backoff).
2. Rate: `sum(rate(jobs_processed_total[5m])) by (kind, outcome)` in Grafana (Prometheus). A queue that grows while this rate is zero means the job runner is stuck or the server is down: see Server down. A backlog of one kind only usually has one cause (mail relay down for `outcome="failed"`, storage full, transcoder worker gone: [TRANSCODER.md](TRANSCODER.md)).
3. Transcode backlog: Admin > Cinema transcoding shows the workers; start another worker or wait. Nothing else needs doing.
4. Dead rows are Jobs failing, not a backlog: follow that playbook.
5. Over when "Jobs queued" falls back to the usual few and "Dead jobs" is 0.

## Voice egress

_Raised by:_ the `VoiceEgress` rule (`sr_voice_egress_bits_per_second` above the budget for 10 minutes; edit the number in `alerts.yml` to match `voice.egress_budget_mbps`), the "Voice egress" figure on the Admin Dashboard (bytes the embedded SFU has sent since it started; it only appears when the SFU runs in the server process) growing faster than the uplink pays for, or members reporting video that drops to low quality.

1. Dashboard: "Voice rooms" and "Voice egress". Sample the egress twice a minute apart: the difference over 60 s, times 8, over one million, is the current Mbit/s.
2. Compare it with the governor budget `voice.egress_budget_mbps` (environment `SR_VOICE__EGRESS_BUDGET_MBPS`). Above the budget the SFU caps simulcast layers on its own, Level 0 first and Patrons last (doc 08 section 3), so a full uplink degrades quality instead of failing. `0` turns the governor off; a good value is about 70 percent of the measured uplink.
3. If members are being capped too early, raise the budget only up to what the uplink really carries. The node-wide limits are `voice.max_participants` and `voice.max_room_size`.
4. Check the host's network graph or the provider's traffic counter before deciding it is a SIDE ROOM problem: a metered plan runs out before the budget matters.
5. A restart of the server resets the counter. Over when the egress rate stays under the budget for an evening.

## Invoice stuck

_Raised by:_ the `InvoiceStuck` rule (`sr_invoice_oldest_pending_age_seconds` above a day; `sr_invoices_pending` is the count), a card in the Staff Server `#alerts` channel from the payment watcher ("payment attention": underpaid, overpaid or paid late), or a member writing that they paid and nothing happened.

1. Admin > Invoices: filter by state. `open`, `seen` and `confirming` are normal for a short time; `underpaid`, `overpaid` and `late_paid` need a person. Open the invoice: transactions seen, confirmations, the audit trail.
2. Nothing seen at all although the member paid: Admin > System > Preflight, the Esplora row. The watcher job `payments.watch` polls every 15 s and talks to the chain source (the electrs of the `payments` profile, or an external Esplora); if the source is down, every invoice looks unpaid. Fix the source first, the watcher catches up by itself (polling is idempotent: a second poll never credits twice).
3. The member can press "I've paid" on the invoice dialog (Settings > Support): it polls that invoice at once, at most every 10 s.
4. Underpaid: the member can top up the same invoice. Overpaid and late-paid are credited already; the excess is a note for you.
5. Resolve by hand only when the money is verifiably in your own wallet: invoice > "Resolve by hand" > "Mark paid" credits once, "Refund noted" records a refund you made yourself. No funds move from SIDE ROOM (watch-only wallet). A reason is required and the action is logged.
6. Over when the invoice is in a closed state and no card is waiting in `#alerts`.

## Storage usage

_Raised by:_ the "Storage used" figure on the Admin Dashboard, the Disk space card of Admin > System, a `disk_low` card in `#alerts`, or uploads failing with 503.

1. Two different things fill up: the disk of the host (Disk space card, `df -h`) and the bucket (Dashboard "Storage used"; Garage: `garage stats`). For the disk, follow Disk pressure.
2. Where the bytes are: Admin > Cinema transcoding (HLS renditions are rebuilt from originals and are the cheapest to give up) and Admin > Ingest sources (a source that keeps re-importing).
3. Retention that frees space by itself: `retention.deleted_message_days` and `retention.large_file_days` (Admin > Limits & settings). Lowering them deletes real data after the next purge job; read the setting's description first and do not lower them on an incident whim.
4. More room: add capacity to Garage ([GARAGE.md](GARAGE.md), "Several nodes") or move the bucket; do not delete files inside Garage's data directory by hand.
5. The alert threshold is `alerts.disk_percent` (default 80) on the path in `ops.disk_path`. Over when usage is back below it and uploads succeed.

## Backup missing or stale

_Raised by:_ the `BackupMissingOrStale` rule (`sr_backup_last_success_timestamp_seconds` older than 26 hours, or 0 when there is no backup), a `backup_failed` card in `#alerts`, the Backups card of Admin > System showing "none" or an old time, or `restore-drill.sh` refusing an old dump.

1. Is the setting pointing at the right folder? `ops.backup_dir` must be the `SR_BACKUP_DIR` of the backup job ([BACKUP.md](BACKUP.md), "On the Compose stack"). Empty switches the alert off.
2. Did the timer run? `systemctl list-timers 'sideroom-*'` and `journalctl -u sideroom-backup.service -n 100`. Run it by hand: `deploy/scripts/backup.sh`.
3. `pg_dump` errors (permission, version mismatch, disk full) are printed there. Disk full: see Disk pressure.
4. The age limit is `alerts.backup_max_age_hours` (default 36). Over when the newest `*.dump` is younger than that, its manifest exists, and the tile shows the new time.

## WAL archive lag

_Raised by:_ WAL files piling up in the `wal-archive` volume, a Postgres log full of `archive command failed`, or a recovery point older than the five minutes `archive_timeout=300` promises ([BACKUP.md](BACKUP.md), "Point-in-time recovery"). The `WalArchiveLag` rule watches `sr_wal_archive_last_success_timestamp_seconds` and `sr_wal_archive_failed_total` (both come from `pg_stat_archiver` and are absent while Postgres `archive_mode` is off, so the rule stays quiet on a server that does not archive); for the cause look in Postgres.

1. In the postgres container: `select last_archived_wal, last_archived_time, last_failed_wal, last_failed_time, failed_count from pg_stat_archiver;`. A `last_failed_time` newer than `last_archived_time` means the archive command is failing right now.
2. Why: the archive volume is full or not writable (`df -h` inside the container, owner `postgres`, mode 700), or `wal-archive.sh` refuses a file that differs from the one already archived (it never overwrites). The script's message is in the postgres log (`dc logs postgres`).
3. A full archive is fixed by a base backup, which trims it: `deploy/scripts/basebackup.sh` (it keeps the newest `SR_BASEBACKUP_KEEP`, default 7, and runs `pg_archivecleanup` when `SR_WAL_ARCHIVE_DIR` is set). Do not delete segments by hand: a gap makes every later recovery stop at the gap.
4. Postgres keeps unarchived segments in `pg_wal` and fills its own disk while archiving fails: this is how a lag becomes Disk pressure. Fix the archive first.
5. Over when `failed_count` stops growing, `last_archived_time` is recent after `select pg_switch_wal();`, and a fresh base backup exists.

## Restore drill failed

_Raised by:_ the `RestoreDrillFailed` rule (`sr_restore_check_last_success_timestamp_seconds` older than 8 days), the `sideroom-restore-drill.service` unit failing (wire it to `OnFailure=`, see BACKUP.md), a failed `drill` or `wal-drill` job in the weekly `backup-drill.yml` workflow, or the restore-check line of the Admin > System Backups card going stale.

1. `journalctl -u sideroom-restore-drill.service -n 100`. The script says which check failed: no dump, dump older than `SR_DRILL_MAX_AGE_HOURS` (default 36), checksum mismatch, a migration without success, a missing table.
2. Old dump: this is Backup missing or stale; fix that first. Checksum mismatch: the file or its manifest is damaged; the next backup replaces it, and copies off the host should be compared.
3. Tables or migrations missing from a good dump: do not trust the backups until it is explained. Take a fresh `deploy/scripts/backup.sh`, run `deploy/scripts/restore-drill.sh` by hand, and keep the failing dump for inspection.
4. Failures of `backup-drill.yml` in CI mean a script changed, not your data: read the job log.
5. Over when `restore-drill.sh` exits 0 and `restore-drill.ok` has a new time in the backup folder.

## CSP reports spike

_Raised by:_ the `CspReportsSpike` rule (more than 50 reports in 5 minutes, from `sr_csp_reports_total{directive}`), the "Blocked content reports" card of Admin > System showing counts that jump, or members reporting a broken embed, player or image after a deploy.

1. Read the card: the violated directive, the blocked origin and the page path (paths only, no members, no queries). A new origin on one directive after a deploy is a policy that is too strict for a feature (a separate voice host, activities, an image host).
2. Compare with `deploy/compose/caddy/Caddyfile.prod` and `apps/web/src/security/csp.ts`; `apps/web/src/security/csp.test.ts` keeps the two equal. The [FAQ](FAQ.md) explains the origin variables.
3. Many keywords such as `inline` or `eval` from one path: a page started injecting script. Treat it as a possible XSS: find the page, check the last deploy, and look at what content is rendered there before loosening anything. Never add `unsafe-inline` or `unsafe-eval` to silence the card.
4. Browser extensions produce scattered, one-off counts with odd origins; ignore those.
5. The endpoint is `POST /internal/csp-report` (rate limited and capped); it is the only `/internal` path the proxy forwards. Over when the counts stop growing after the fix.

## Suspicion case opened

_Raised by:_ an `inviter_suspicion` or `inviter_escalated` card in `#alerts`: a serious ban of an invitee raised the inviter's suspicion; at the freeze threshold invites are frozen, at the escalation threshold a case opens for senior staff (thresholds `invites.suspicion_freeze_threshold` and `invites.suspicion_escalate_threshold`).

1. Admin > Moderation > Cases: the case linked to the banning case. Admin > Accounts > the inviter > suspicion panel: events, banned descendants with their reason, the invite path to the root.
2. Admin > Moderation > Signals shows evasion hints (shared sign-in addresses inside the retention window, a matching Discord identity, shared lineage, a new account shortly after a ban). They are hints for a person to judge, never a ban by themselves.
3. Decide: clear with a reason (the events stay), keep the freeze, or act on the accounts through the case. The inviter is told only that invite creation is paused for review, never which invitee caused it; do not tell them more.
4. Over when the case is closed with an outcome and the invite freeze is either lifted or deliberately kept.

## Takedown notice received

For a copyright or illegal-content notice that reached the address on `/help` (`compliance.operator_contact`). Spec: `docs/specs/moderation-compliance.md` section 5. Who: `MANAGE_TAKEDOWNS` for the register, `MANAGE_CASES` for holds. Target: triage within one business day.

1. Admin > Moderation > Takedowns > enter the notice: notifier name and contact, claimed work or legal basis, exact URLs or ids, date received, the original text. A case opens.
2. Identify the content in SIDE ROOM. An incomplete notice gets the templated reply by hand (there is no mail path yet: mailing the reply waits on the lawyer, D-336 and D-335 item 4; staff copy the text out and mail it themselves).
3. Hold, then disable: place a hold on the affected rows so the evidence and the stored object survive, then make the content unavailable (unpublish a library or cinema item, remove a message; an upheld notice also blocks the stored object so already signed links stop working).
4. The uploader or author gets a system DM with the notice summary and the counter-notice path; it never names the notifier.
5. A counter-notice is recorded by staff; the restore date is 10 business days (UTC, the operator country's weekend). Restore is a logged action. _Lawyer-dependent:_ whether the ten days, the forwarding of the counter-notice to the notifier and the DSA statement of reasons are right for the operator's country (lawyer brief, D-336 (a) to (g)). Until answered the defaults stand.
6. Strikes: `moderation.strike_review_threshold` (3) upheld takedowns within `moderation.strike_window_months` (12) open a review case, never an automatic suspension.
7. Nothing in the register is deleted; it is exportable (CSV or JSON) from the Takedowns tab. Over when the case is closed and the register row shows the outcome.

## Data-subject request received

For an access, erasure, restriction, rectification or objection request, from a member or by mail. Who: `MANAGE_TAKEDOWNS` (rectification also `MANAGE_ACCOUNTS`). Spec: `docs/specs/moderation-compliance.md` sections 6 and 17.3.

1. Admin > Data requests: create the request if it came by mail, or open the one the member filed. The due date is `compliance.data_request_deadline_days` (default 30, range 7 to 90) after intake.
2. Verify who is asking before anything else (a reply from the account's mail address, or a signed-in confirmation). Mark the request verified; the verifier is recorded.
3. Access or portability: "run" queues the member's export job without its cooldown (`abuse.data_export_cooldown_hours` applies to self-service only). The link expires after seven days.
4. Erasure: before running, read the retained list the member will be told (Person record, invite graph, invoices, moderation history, held rows, messages and uploads they did not delete). Held rows are kept and flagged: decide each hold on Admin > Moderation > Holds (release, or retain with a stated legal basis). Run it from the request; it is never automatic.
5. Restriction: the "hide activity" action hides cinema and library sharing, presence, aggregates and recommendations while the request is open; the member gets a system DM when it starts and ends. Rectification: link or split the Person from the request, with a reason.
6. Cannot meet the deadline: record an extension with a reason; the member is told by system DM when they can be reached, otherwise answer on the channel they used. _Lawyer-dependent:_ the lawful basis for the retained records and whether a records-of-processing document is needed (lawyer brief, Q17).
7. Over when the request is completed or declined (with an outcome text); rows are never deleted.

## Suspected compromised staff account

This is [Compromised staff account](#compromised-staff-account) above (sessions, `sr-cli user reset-2fa`, `sr-cli staff revoke`, the access log, `sr-cli verify-access-log`, rotating bot tokens, giving roles back). Start there. For the Owner account also read [Lost owner second factor](#lost-owner-second-factor) and [Lost or suspect secret key](#lost-or-suspect-secret-key).

## Lost owner second factor

The Owner cannot reach Admin because the authenticator and the recovery codes are gone, or the Owner's password and device are in doubt. Needs shell access to the host.

1. The host login is the proof that it is the Owner. Run `sr-cli user reset-2fa <owner username or email>`: it removes TOTP, recovery codes and passkeys, revokes every session and trusted device, writes the platform audit row `user.reset_2fa` and mails a notice to the address.
2. Sign in with the password (reset it from the sign-in page if needed), enroll a new authenticator first thing and store the new recovery codes off the machine. Sensitive admin actions need a second factor checked in the last 15 minutes, so nothing sensitive works until this is done.
3. If the account itself may be in someone else's hands, do not stop at step 1: follow Compromised staff account (access log check and `sr-cli verify-access-log`).
4. If the Owner account is lost for good, hand the Staff Server to another account: `sr-cli staff transfer-owner <user> --reason "<why>"`, then `sr-cli staff ensure-roles`.
5. Over when the Owner signs in with the new factor and `sr-cli staff list` shows the Owner.

## Disk full

The disk is at 100 percent: Postgres refuses writes, Garage fails, `/readyz` returns 503. An escalation of Disk pressure; do that playbook first if the disk is only filling.

1. Stop the growth: `systemctl stop sr-server` (or `dc stop sr-server`) so nothing writes while you work. Do not stop Postgres abruptly.
2. `du -xh /var/lib | sort -h | tail`; the usual four are Postgres WAL (a failing archive, see WAL archive lag), Garage data, the search index (`search.dir`) and the backup directory.
3. Free the safe things first: old backups (lower `SR_BACKUP_KEEP` and run `deploy/scripts/backup.sh`, or move them to the Storage Box), old container logs (`docker system df`), the transcoder's scratch space. Never delete files inside the Postgres or Garage data directories by hand.
4. Start Postgres, then the server, and check `/readyz` and Admin > System. Take a fresh backup (a restore point from before the incident is worth more than the space it uses).
5. Prevent the repeat: set `ops.disk_path` to the data volume and `alerts.disk_percent` low enough to leave a day of headroom.

## Restore from backup

For a dead database, a bad migration or data loss. The details are in [BACKUP.md](BACKUP.md).

1. Stop `sr-server`, so nothing writes during the restore. Keep the broken database as it is until the restored one is verified.
2. Choose the source. A dump (up to 24 hours of recovery point): `ls /var/backups/sideroom/db/`, then `deploy/scripts/restore.sh <dump> <url-of-an-empty-database>` (or `sr-cli restore <dump>` against the configured database, `--force` to replace one that has data). To land on a moment (about five minutes of recovery point): a base backup plus the WAL archive with `deploy/scripts/pitr-restore.sh <base backup> <data dir> <wal archive> "<timestamp>"`.
3. Apply the migrations newer than the dump: `sr-cli migrate` (or `sr-cli upgrade`).
4. Object storage: restore from the same night as the dump (BACKUP.md, "Restoring"); orphans are harmless, missing objects show as broken attachments. `server.secret_key` must be the key that was current when the dump was made, or every TOTP secret in it is unreadable (`server.secret_key_previous` holds earlier keys).
5. Start the server, run `sr-cli preflight` and `sr-cli verify-access-log`. Sign in as the Owner and check the newest message and the newest invoice.
6. Tell the members what window of changes was lost. Over when preflight is green and the next backup and drill run clean.

## A member reports CSAM

> **PLACEHOLDER, lawyer-dependent steps below.** The preservation period, the duty to report and who must be told depend on the operator's country and are an open question for the owner's lawyer (decision D-336 item (h), lawyer brief Q16). The register has no reporting step today. **Until the lawyer answers, the steps marked LAWYER are not defined and must not be improvised from this page.** The owner should have the answer before the first invite goes out.

What is certain and safe regardless of the answer:

1. **Do not open, download, forward, copy or screenshot the material** on a personal device, and do not ask the reporter or anyone else to send it. Work from the report and the case only. A member who has already received it is told to delete it and not to share it.
2. The report arrives in Admin > Moderation > Cases with category `child_safety`; the Dashboard tab counts open child-safety cases. Claim the case (`MANAGE_CASES`).
3. **Hold the evidence in place:** place a hold from the case on the message, attachment or submission so the stored object is kept and not purged (a hold needs a reason and is logged). Holds never expire by themselves. A held object stays readable only by staff with `READ_PRIVATE_CONTENT`, and every read is access-logged with a reason; do not read it unless the lawyer's steps need it.
4. **Make it unavailable to members** (remove the message or unpublish the item; that is not the same as deleting the evidence).
5. **Act on the account:** platform-ban with reason category `child_safety` through the case (serious by default, so the banned person's inviter is reviewed through the suspicion tooling). A system DM may be skipped when staff judge it unsafe.
6. **LAWYER (not defined):** whether and where to report (for a US operator the question names 18 U.S.C. 2258A and NCMEC; the equivalent for the operator's country is the lawyer's to name), the contents and deadline of the report, how long the preserved material and the account data must be kept, and who at the operator may handle the material.
7. **LAWYER (not defined):** whether the hold's retention conflicts with a data-subject request or an erasure by the uploader (the hold flags the request; staff decide, and the lawyer's answer decides the basis).
8. Record in the case notes what you did and when, never the material itself. The case stays open until the lawyer's reporting step has been done.
