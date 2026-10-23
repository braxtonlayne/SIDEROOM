# Go-live checklist

The steps from an empty server to the first invites, in order. Each step names the command and what "done" looks like. Do not skip ahead: every step assumes the ones before it are done. The owner-side actions (accounts, GitHub, downloads) are in [docs/dev/owner-checklist.md](../docs/dev/owner-checklist.md); the questions for a lawyer are in [docs/legal/lawyer-brief.md](../docs/legal/lawyer-brief.md). Reference pages: [INSTALL.md](INSTALL.md), [BACKUP.md](BACKUP.md), [RUNBOOK.md](RUNBOOK.md), [UPGRADE.md](UPGRADE.md).

Status: the shipped files have not yet booted on a real host (see the README "Status"). The first run of this list is the proof; write down what breaks.

`dc` below stands for `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod`, and `sr-cli` for `dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server` (the server image carries `sr-cli`; this is how `install.sh` calls it). For the plain-binary route, `sr-cli` is the binary itself.

## 1. Provision and harden the host

Doc 12 section 8 is the list: UFW with only 22, 80, 443, 3478, 5349 and the voice ports open, SSH keys only, unattended upgrades, fail2ban, disk encryption where the provider supports it, a separate Postgres role per service, secret files at mode 0600. Install Docker with the compose plugin.

- **Command.** `sudo ufw status verbose`; `docker compose version`.
- **Done when.** The firewall shows only the listed ports, password SSH login is refused, and `docker compose version` prints a version.

## 2. DNS

Point the public hostname (and the TURN hostname, if separate) at the host.

- **Command.** `dig +short chat.example.org` (use your hostname).
- **Done when.** It prints the host's public address, from outside the host as well. Caddy cannot get a certificate until this is true.

## 3. Install

Unpack the web build into `SR_WEB_DIR` (default `/opt/sideroom/web`), then run the installer. It asks for the domain, the mail server, the ingest folder and the Owner account, and generates every secret.

- **Command.** `deploy/install.sh` (or `deploy/install.sh --yes` with the answers in the environment; see the script header).
- **Done when.** The script ends without an error, `dc ps` shows every service running, and `curl -fsS https://chat.example.org/healthz` answers. Back up `deploy/compose/.env.prod` now: `SR_SECRET_KEY` decrypts every stored TOTP secret and seals the Web Push key.

## 4. Preflight is green

`sr-cli preflight` (item RDY-OPS-004) checks the database and migrations, storage, SMTP, the public URL and its headers, TURN, disk space, clock skew, secret-file modes and, when they are on, payments and the voice node. `install.sh` runs it as its last step once it lands in the integration branch.

- **Command.** `sr-cli preflight`
- **Done when.** The table has no row marked fail and the exit code is 0. Read every warn: each one says in a sentence what to do. The same table is on Admin > System > Preflight once RDY-OPS-005 lands. Until the command exists, the manual equivalent is `curl -fsS https://<host>/readyz` plus the "Integrations" tiles on Admin > System.

## 5. Backups verified by a restore

A backup nobody has restored is a hope ([BACKUP.md](BACKUP.md)).

- **Commands.**
  1. Take a dump: `deploy/scripts/backup.sh` (or `sr-cli backup`).
  2. Restore it into a scratch database: `createdb sideroom_restored`, then `deploy/scripts/restore.sh <dump> <url-of-that-database>`.
  3. Run the drill: `deploy/scripts/restore-drill.sh`.
  4. Enable the schedule: copy `deploy/systemd/sideroom-backup.*`, `sideroom-basebackup.*` and `sideroom-restore-drill.*` to `/etc/systemd/system/` and `systemctl enable --now` the three timers.
  5. Set the Storage Box repository in `/etc/sideroom/backup.env` (`SR_BACKUP_REPOSITORY`) so the copies leave the host.
- **Done when.** The restore into the scratch database succeeds and `restore-drill.sh` exits 0 and touches `restore-drill.ok`; Admin > System shows the last backup and its verification; a copy of the dump exists off the host. Drop the scratch database afterwards.

## 6. Monitoring and the alert test

- **Commands.** `dc --profile monitoring up -d`; then `sr-cli preflight --alert-test` (item RDY-OPS-006).
- **Done when.** Prometheus and Grafana are up (the dashboards in `deploy/monitoring/grafana` load, no panel says "no data" for a metric the server exports), and a test card naming the host and the time appears in the Staff Server `#alerts` channel (and the mail arrives, if `alerts.email_to_owner` is on). If the command does not exist yet, use the Admin > System "Send a test email" button and confirm that the health job's alert settings (`alerts.*`, [RUNBOOK.md](RUNBOOK.md)) are set, then repeat this step with the command when it lands.

## 7. The Setup Wizard

Sign in as the Owner (the one-time link printed by `bootstrap` is single use and valid for one hour, so open it right away) and open the setup checklist on the Admin dashboard.

- **Done when.** Every row of the checklist is green or consciously waived: storage test, email test, Discord sign-in (needs the redirect URI of the owner checklist, section 1), voice and TURN, wallet or the donations-off choice, default limits, staff. The Owner has turned on a second factor.

## 8. Lawyer settings

After the brief has been answered ([lawyer-brief.md](../docs/legal/lawyer-brief.md)); if it has not, leave the defaults and write down that you did.

- **Where.** Admin > Features > Compliance (a reason is required): `accounts.minimum_age`, `compliance.operator_country`, `compliance.operator_contact`; the other compliance values keep their defaults unless the lawyer says otherwise.
- **Done when.** `/help` publishes the copyright notice address, the privacy page names the operator's country and contact, and the invite landing shows the minimum age. Check all three signed out.

## 9. The first invites

- **Command.** Admin > Invites, or from the host `sr-cli invite create --note "first friend"`.
- **Done when.** One friend has opened the link in a clean browser, registered (both boxes ticked), signed in with a second factor and seen the Lounge; a message sent between you arrived. Send the rest of the invites only after that.

If anything in steps 1 to 9 needed a workaround, add it to [FAQ.md](FAQ.md) or fix the file that forced it, and tell the next operator in [RUNBOOK.md](RUNBOOK.md).
