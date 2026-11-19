# Go-live checklist

The steps from an empty server to the first invites, in order. Each step names the command and what "done" looks like. Do not skip ahead: every step assumes the ones before it are done. The owner-side actions (accounts, GitHub, downloads) are in [docs/dev/owner-checklist.md](../docs/dev/owner-checklist.md); the questions for a lawyer are in [docs/legal/lawyer-brief.md](../docs/legal/lawyer-brief.md). Reference pages: [INSTALL.md](INSTALL.md), [BACKUP.md](BACKUP.md), [RUNBOOK.md](RUNBOOK.md), [UPGRADE.md](UPGRADE.md). The tests that wait for a real server (R2, two-network voice, Safari, the payment walk, the playbooks) are listed in order in `docs/dev/deployment-day-checks.md` (link: ../docs/dev/deployment-day-checks.md from deploy/).

After go-live, the operator handbook is [OPERATOR.md](OPERATOR.md): the map of every deploy page and the daily, weekly and monthly checks.

Status: the shipped files boot on a CI runner (`prod-smoke.yml`, RDY-OPS-001), but have not yet booted on a real host (see the README "Status"). The first run of this list on a real host is the proof; write down what breaks.

`dc` below stands for `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod`, and `sr-cli` for `dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server` (the server image carries `sr-cli`; this is how `install.sh` calls it). For the plain-binary route, `sr-cli` is the binary itself.

## 1. Provision and harden the host

Doc 12 section 8 is the list: UFW with only 22/tcp, 80/tcp, 443/tcp and 443/udp open, plus, when voice is on, 50000-50099/udp, and, when the `turn` profile is on, 3478 udp+tcp, 5349/tcp and 49152-49999/udp, SSH keys only, unattended upgrades, fail2ban, disk encryption where the provider supports it, a separate Postgres role per service (optional hardening, not set up by the shipped files), secret files at mode 0600. Install Docker with the compose plugin. Docker publishes the ports of `prod.yml` around UFW's rules, so `ufw status` is not proof of what is reachable; check from outside with a port scan.

- **Command.** `sudo ufw status verbose`; `docker compose version`.
- **Done when.** The firewall shows only the listed ports, password SSH login is refused, and `docker compose version` prints 2.24 or newer.

## 2. DNS

Point the public hostname (and the TURN hostname, if separate) at the host.

- **Command.** `dig +short chat.example.org` (use your hostname).
- **Done when.** It prints the host's public address, from outside the host as well. Caddy cannot get a certificate until this is true.

## 3. Install

**What must exist first.** The release: push the version tag and wait for the Release workflow to finish green, then check that the release lists `sideroom-<tag>-web.tar.gz` in `SHA256SUMS` and that the image `ghcr.io/<owner>/sideroom-server:<tag>` (the tag includes its leading `v`, the owner is lower case) can be pulled. If the package is private, the owner runs `docker login ghcr.io` and types the token.

**Get the files onto the server.** The release has no `deploy/` bundle, and the web tarball holds only the web build. Take `deploy/` and `packages/` from the tag with git, and put them beside the web folder, because the backup units expect `/opt/sideroom/deploy` and the installer looks for the starter packs at `deploy/../packages`:

```
git clone --branch <tag> --depth 1 <repo-url> /opt/sideroom-src
mkdir -p /opt/sideroom
cp -a /opt/sideroom-src/deploy /opt/sideroom/deploy
cp -a /opt/sideroom-src/packages /opt/sideroom/packages
```

Copy them; do not link `/opt/sideroom/deploy` to the clone. `install.sh` finds its folder with `cd` and `pwd`, so through a link `deploy/../packages` is read as `/opt/sideroom/packages` (POSIX `cd` resolves `..` by the path as typed), not as the clone's `packages`, and the starter-pack step would stop the installer. Then unpack the web build into `SR_WEB_DIR` (default `/opt/sideroom/web`): `tar -xzf sideroom-<tag>-web.tar.gz -C /opt/sideroom` after `sha256sum -c`, and `test -f /opt/sideroom/web/app/csp.txt`.

**The installer.** It asks for the domain, the image, the mail server, the ingest folder and the Owner account, and generates every secret. The image question is the full reference `ghcr.io/<owner>/sideroom-server:<tag>`: its default is the example's placeholder, and the installer refuses the placeholder before it writes or starts anything, saying what to type. If a run stops at the pull, edit `SR_IMAGE` in `deploy/compose/.env.prod` and run the installer again: an `SR_IMAGE` given to the rerun is not applied, the installer says so and names both values. The generated Owner password (when you did not type one) and the one-time sign-in link are printed as soon as the Owner exists, and the password again at the end; the link is good for one hour and is not shown again, so open it at once. If the script stops after that point, the rerun cannot show either again: use "Forgot password" on the sign-in page with the Owner's email. It also takes `SR_INSTALL_EMU_CORES=yes` (fetch the emulator cores by pinned URL and SHA-256, [EMULATOR.md](EMULATOR.md); the default is no, and the Games page needs them) and `SR_PAYMENTS_ELECTRS=yes` (the self-hosted `electrs` profile; owner checklist section 2).

- **Command.** `deploy/install.sh` (or `deploy/install.sh --yes` with the answers in the environment; see the script header). If it stops part way, run it again: it resumes with the files it already wrote (INSTALL.md, "Running it again").
- **Done when.** The script ends without an error, `dc ps` shows every service running, and `curl -fsS https://chat.example.org/healthz` answers. Back up `deploy/compose/.env.prod` now: `SR_SECRET_KEY` decrypts every stored TOTP secret and seals the Web Push key.

## 4. Preflight is green

`sr-cli preflight` (item RDY-OPS-004) checks the database and migrations, storage, SMTP, the public URL and its headers, TURN, disk space, clock skew, secret-file modes and, when they are on, payments and the voice node. `install.sh` runs it as its last step (`SR_INSTALL_PREFLIGHT=warn` prints the table and reports a FAIL row without failing the install; `strict` exits 3 on one; `skip` leaves it out).

- **Command.** `sr-cli preflight`
- **Done when.** The table has no row marked fail and the exit code is 0. Read every warn: each one says in a sentence what to do. The same table is on Admin > System (the Preflight card, `POST /admin/system/preflight`, RDY-OPS-005). A quick manual check is `curl -fsS https://<host>/healthz` (liveness) and, from the host, `dc exec sr-server /usr/local/bin/sr-server healthcheck` (readiness: exit 0) plus the Integrations tiles on Admin > System.

## 5. Backups verified by a restore

A backup nobody has restored is a hope ([BACKUP.md](BACKUP.md)). On the Compose route (what `install.sh` builds) the client tools, the database and the `pg-backups` volume are all inside the Postgres container, so every step below runs there with `dc` (the host needs nothing but Docker). The plain-binary route is the last bullet.

- **Commands (Compose route, run from `/opt/sideroom`).**
  1. Take a dump inside the Postgres container:
     `dc exec -T -u postgres postgres sh -c 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh'`
     It prints `wrote sideroom-<stamp>.dump (... bytes, ... migrations)` and `backup complete`. The file is `/var/backups/sideroom/db/sideroom-<stamp>.dump` with a `.dump.json` manifest beside it; `dc exec -T postgres ls -1 /var/backups/sideroom/db` lists it.
  2. Restore it into a scratch database (use the file name from step 1):
     `dc exec -T postgres psql -X -U sideroom -d postgres -c 'create database sideroom_restored'`
     `dc exec -T -u postgres postgres sh -c "sh /opt/sideroom/scripts/restore.sh /var/backups/sideroom/db/sideroom-<stamp>.dump postgres://sideroom@/sideroom_restored"`
  3. Run the drill:
     `dc exec -T -u postgres postgres sh -c 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom sh /opt/sideroom/scripts/restore-drill.sh'`
  4. Tell the server where the backups are: Admin > Limits & settings, group `alerts`, set `ops.backup_dir` to `/var/backups/sideroom` (the folder as sr-server sees the `pg-backups` volume, read-only). With it empty the Admin backup alert is off.
  5. Enable the schedule. Copy `deploy/systemd/sideroom-backup-compose.*`, `sideroom-restore-drill-compose.*` and `sideroom-basebackup.*` to `/etc/systemd/system/`, run `systemctl daemon-reload`, then `systemctl enable --now sideroom-backup-compose.timer sideroom-restore-drill-compose.timer sideroom-basebackup.timer`. Each unit runs the command of step 1, step 3 or the base backup with `docker compose ... exec` and needs no password. Do **not** enable `sideroom-backup.timer` or `sideroom-restore-drill.timer` here: they are the plain-binary route's and fail every night on Compose.
  6. Get the copies off the host. The dumps live in the `pg-backups` volume (`/var/lib/docker/volumes/<project>_pg-backups/_data` on the host). Install rustic or restic on the host yourself and copy that folder to the Storage Box on a schedule of your own; the owner types the Storage Box address, the SFTP key and the repository password, nobody else. The Compose units do not read `/etc/sideroom/backup.env` and the container has neither tool, so `SR_BACKUP_REPOSITORY` is not used on this route. Garage's metadata folder is not in an object snapshot: copy `garage-meta` as well.
- **Done when.** Step 2's restore succeeds, `restore-drill.sh` exits 0 and touches `restore-drill.ok`, Admin > System shows the last backup and its verification (after step 4), and a copy of the dump exists off the host. Drop the scratch database afterwards: `dc exec -T postgres psql -X -U sideroom -d postgres -c 'drop database sideroom_restored'`.
- **Plain-binary route** (the systemd layout of [INSTALL.md](INSTALL.md), with `pg_dump` on the host). Dump with `deploy/scripts/backup.sh` (or `sr-cli backup`), restore with `createdb` and `deploy/scripts/restore.sh <dump> <url>`, drill with `deploy/scripts/restore-drill.sh`, and enable `sideroom-backup.timer` and `sideroom-restore-drill.timer` as in BACKUP.md "One-off setup" (the settings, including `SR_BACKUP_REPOSITORY` for the Storage Box, go in `/etc/sideroom/backup.env`). `sideroom-basebackup.timer` is Compose-only: it execs into the Postgres container and no host form ships, so leave it off on this route.

## 6. Monitoring and the alert test

- **Commands.** `dc --profile monitoring up -d`; then `sr-cli preflight --alert-test` (item RDY-OPS-006).
- **Done when.** Prometheus and Grafana are up (the dashboards in `deploy/monitoring/grafana` load, no panel says "no data" for a metric the server exports), and a test card naming the host and the time appears in the Staff Server `#alerts` channel (and the mail arrives, if `alerts.email_to_owner` is on). The Admin > System "Send a test email to me" button checks mail alone; the health job's alert settings are `alerts.*` ([RUNBOOK.md](RUNBOOK.md)). The Prometheus rules in `deploy/monitoring/alerts.yml` are shown in Grafana only; no Alertmanager is installed. The alerts that reach you are the cards in `#alerts` and the optional mail to the Owner; the test above proves exactly that.

## 7. The Setup checklist

Sign in as the Owner (the one-time link printed by `bootstrap` is single use and valid for one hour, so open it right away) and open the setup checklist on the Admin dashboard.

- **Discord row (Compose).** It goes green after the Discord client id is in the server's env file (`/etc/sideroom/server.env`) as `SR_DISCORD__CLIENT_ID`, `SR_DISCORD__CLIENT_SECRET_FILE` is set there to `/run/secrets/sideroom/discord_client_secret`, and the owner has typed the secret into `/etc/sideroom/secrets/discord_client_secret` (owner 65532, mode 0400; OPERATOR.md "Voice (configuration file or environment, not Admin)"), and the server is recreated with `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod up -d sr-server`; otherwise mark the row waived.
- **Done when.** Every row of the checklist (object storage, wallet, limits, second staff member, SMTP, Discord, voice and TURN) is green or consciously waived: limits goes green only after you change a value, and the staff row needs a second appointment. Separately: the storage and email test buttons on Admin > System have been pressed, and the Owner has turned on a second factor.

## 8. Lawyer settings

After the brief has been answered ([lawyer-brief.md](../docs/legal/lawyer-brief.md)); if it has not, leave the defaults and write down that you did.

- **Where.** Admin > Features > Compliance (the reason field is optional, D-632): `accounts.minimum_age`, `compliance.operator_country`, `compliance.operator_contact`; the other compliance values keep their defaults unless the lawyer says otherwise.
- **The beta bug reporter (`platform.beta_reports`, D-684).** Before you invite anyone: the beta bug reporter is off, or the platform has fewer than 100 accounts, or the lawyer has answered Q27 and the privacy notice follows the answer.
- **Done when.** Signed out, the house notes (`/house-notes`) and the help section of the sign-in page (`/login`) show the copyright notice address, the privacy page names the operator's country and contact, and the invite landing shows the minimum age. (`/help`, the handbook, is for signed-in members only and sends a visitor to the sign-in page, D-385.)

## 9. The first invites

- **Command.** Admin > Invites, or from the host `sr-cli invite create --note "first friend"`.
- **Done when.** One friend has opened the link in a clean browser, registered (both boxes ticked), signed in with a second factor and seen the Lounge; a message sent between you arrived. Send the rest of the invites only after that.

If anything in steps 1 to 9 needed a workaround, add it to [FAQ.md](FAQ.md) or fix the file that forced it, and tell the next operator in [RUNBOOK.md](RUNBOOK.md).
