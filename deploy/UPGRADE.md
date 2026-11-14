# Upgrading

Releases are semantic-versioned. Migrations are forward-only and each release must work with the previous release's binary for one version (expand/contract, doc 12 section 6), so a rollback of one version is safe.

`sr-cli upgrade` does the database half in one command: it compares the database with this release's migrations, refuses a database that is newer than the program or has a failed migration, takes a pre-upgrade backup (`sr-cli backup`, needs `pg_dump`; `--no-backup` skips it) and applies what is pending. `sr-cli upgrade --dry-run` only lists it. Steps 2 and 6 below are that command; the rest stay manual.

This page is rehearsed on every change to the install surface: the `upgrade` job of `.github/workflows/prod-smoke.yml` (`deploy/scripts/prod-smoke.sh upgrade-*`) installs the previous version on a Compose stack, seeds it, runs the commands below against the pull request's image, then rolls back from the backup and checks the data after each move (RDY-OPS-007). The commands in the blocks below are the ones it runs. The shorthand used throughout:

```
cd /opt/sideroom/deploy/compose
dc="docker compose -f prod.yml --env-file .env.prod"
```

## Routine upgrade

1. Read the release notes for migrations, settings and new `.env.prod` keys that need action.
2. **Back up first.** On a Compose install, run `backup.sh` inside the postgres container (the host has neither `pg_dump` nor a published database port; BACKUP.md "On the Compose stack" explains the variables). Note the dump it prints, you need it if you roll back, then confirm that `restore-drill.sh` still passes:

   ```
   $dc exec -T -u postgres postgres sh -c 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh'
   ```

   On a plain-binary install run `deploy/scripts/backup.sh` with `SR_DATABASE_URL` set instead.
3. Update the deploy files to the release: the new `deploy/` folder (at least `compose/prod.yml`, `compose/caddy/`, `compose/garage/` and `scripts/`, which the postgres container mounts) replaces the old one, and your `compose/.env.prod` and `compose/garage/garage.prod.toml` stay as they are. Compare `compose/.env.prod.example` with your `.env.prod` and add any key the release introduced (`docker compose config` fails with "set X in .env.prod" for a required one). Nothing is regenerated: `install.sh` is for a first install and refuses to touch an existing one.
4. Pull the new image (Compose: change `SR_IMAGE` in `.env.prod`, `$dc pull`) or install the new binary.
5. Unpack the new web build into `SR_WEB_DIR`. The server reads the same directory (read-only, `SR_WEB__ROOT`; see "the web app is served by the server" below), so the swap applies to both containers: recreate Caddy and the server (`$dc up -d --force-recreate caddy sr-server`) after swapping the directory itself. Replace it atomically (unpack beside it, then swap the directory) so no visitor sees a half-written build. On Compose the directory is a bind mount: after swapping the directory itself, recreate Caddy (`$dc up -d --force-recreate caddy`) so it sees the new one; unpacking over the existing files needs nothing. Open tabs keep working: the service worker keeps serving the old build until the visitor accepts the "new version" toast.
6. Apply migrations **before** starting the new server: `sr-cli upgrade` (with Compose, from the new image, since step 4 changed `SR_IMAGE`; step 2 already backed up and the image has no `pg_dump`, hence `--no-backup`; the image carries `sr-cli`):

   ```
   $dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server upgrade --no-backup
   ```

   The old server keeps running during this; the previous binary tolerates the new schema by the one-version rule.
7. Restart: `$dc up -d sr-server` or `systemctl restart sr-server`. The server drains for up to 25 s, then jobs for another 25 s, so allow a minute. With the `monitoring` profile on, also run `$dc up -d prometheus`: Prometheus lives in the server container's network namespace and must follow a recreated server.
8. Check readiness, the Grafana overview, and the error ratio for ten minutes. Caddy answers `/readyz` with 404 (it lists dependency errors; the shipped `Caddyfile.prod` has a `handle /readyz` block for this), so ask the server itself: `$dc exec sr-server /usr/local/bin/sr-server healthcheck` (exit 0 = ready; `$dc ps` shows the same probe as the container health), or `curl -fsS http://127.0.0.1:8080/readyz` on a plain-binary install. `https://<domain>/healthz` through Caddy must answer too.

## Rolling back

Two cases, in this order of preference.

**The migration ran and the new release is bad.** Roll back the binary and web build to the previous release, and leave the database alone: the previous binary tolerates the new schema by the one-version rule. With Compose, put the old `SR_IMAGE` back in `.env.prod` (and the old deploy files and web build if the release changed them) and run `$dc up -d sr-server`.

**The migration itself failed, or the previous binary cannot read the new schema** (a release note will say so; migration 0803 below is one). Go back to the pre-upgrade dump from step 2. Restore it into an *empty* database, not over the upgraded one: `SR_RESTORE_FORCE=1` only drops the objects the dump contains, so tables a newer migration added stay behind and the next upgrade then stops on "already exists". With Compose, using the dump path step 2 printed:

```
$dc stop sr-server
$dc exec -T postgres psql -X -U sideroom -d postgres -c 'drop database sideroom with (force)' -c 'create database sideroom'
$dc exec -T -u postgres postgres sh -c "sh /opt/sideroom/scripts/restore.sh /var/backups/sideroom/db/sideroom-<stamp>.dump postgres://sideroom@/sideroom"
# set the previous SR_IMAGE in .env.prod (and the previous deploy files), then
$dc up -d sr-server
```

Everything written after the backup is lost; the restored database is exactly the one the previous release had. Report the failing migration.

## When the version skips

Upgrading across several releases: step through each release in order, running `sr-cli migrate` at each, unless the release notes say a jump is safe.

## Release notes: release builds refuse test-only settings; metrics stay on loopback (wave 54, SEC-AUD-028, SEC-AUD-031)

- A release build of `sr-server` (the image, the release binaries) now refuses to start, naming the key, when it finds: an empty `server.secret_key`; `unfurl.allow_private_for_tests`; `storage.endpoint = "memory:"`; `activities.origin_mode = "same_host_sandboxed"`; `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS` while `SR_PAYMENTS_ESPLORA_URL` is not a private service (a one-label name such as `http://electrs:3000`, or a private IP address); or a `server.metrics_bind` that is not loopback while `server.metrics_token` is empty (the check is `crates/sr-server/src/release_guard.rs`). A debug build (`cargo build`, the e2e stack and the tunnel test of `docs/ops/test-deploy-quick-tunnel.md`) still only warns. The private-host opt-in now covers the Esplora endpoint only; the price sources always use the strict policy.
- `gateway.max_unidentified_per_ip` (`SR_GATEWAY__MAX_UNIDENTIFIED_PER_IP`; `SR_GATEWAY_MAX_UNIDENTIFIED_PER_IP` in `.env.prod`) is the number of gateway sockets one client address may hold before they sign in (D-556, default 32). Raise it when many members share one address (a NAT); `0` is refused at start. The key is in `.env.prod.example`: add it to an older `.env.prod` if you want to change it (the default applies without it).
- `/internal/metrics` takes an optional bearer token, `server.metrics_token` (`SR_SERVER__METRICS_TOKEN`, or `SR_SERVER__METRICS_TOKEN_FILE`): set it when Prometheus scrapes the server from another host, and send `Authorization: Bearer <token>`.
- Compose: `prod.yml` no longer sets `SR_SERVER__METRICS_BIND=0.0.0.0:9100`. The `monitoring` profile's Prometheus runs in the server container's network namespace (`network_mode: service:sr-server`) and scrapes `127.0.0.1:9100`; Grafana reaches it as `sr-server:9090`. Take the release's `compose/prod.yml`, `monitoring/prometheus.yml` and `monitoring/grafana/provisioning/datasources/prometheus.yml`, then `$dc up -d sr-server prometheus grafana`.
- Caddy now answers `/readyz` with 404 (before, the catch-all proxied it to anyone). Take the release's `compose/caddy/Caddyfile.prod` and recreate Caddy: `$dc up -d --force-recreate caddy`.
- `sr-cli restore` reads the dump's table of contents (`pg_restore --list`) and checks it is a SIDE ROOM dump before `--force` drops anything; a damaged or foreign dump stops with "nothing was changed".

## Release notes: a signed-in member sees their full profile at `/~handle` (FU-BETA-1)

Caddy now sends `/~*` to `sr-server` when the request carries a session cookie. The server answers a member with the app (the full profile page); a visitor without a cookie, or one whose cookie the server refuses (it answers `404`), still gets the static public profile shell from Caddy. To pick this up, re-render the Caddyfile (`node deploy/scripts/render-caddyfile.mjs --public-url https://<domain>`, or take the release's `compose/caddy/Caddyfile.prod`) and recreate Caddy: `$dc up -d --force-recreate caddy`.

## Release notes: public profiles need a second switch (phase 39, OPS-FLG-001, P39-FLG-1)

A public profile page now needs **both** the feature `platform.public_profiles` (Admin > Features) **and** the new setting `profiles.access = opt_in_public` (Admin > Limits & settings, group privacy). The setting defaults to `walled`. **If your instance had `platform.public_profiles` switched on, set `profiles.access` to `opt_in_public` after upgrading, or every public profile page goes dark** (members' choices are kept; the pages come back as soon as the setting is set). Instances that never switched the feature on need do nothing.

## Release notes: invite controls (phase 39, OPS-INV-001)

Migration 1890 renames the setting `invites.initial_credits` to `invites.default_credits` and carries a saved value over; registration now reads it (before, it read only the configured seed). If you scripted `PATCH /admin/settings` with the old key, use the new one. New settings `invites.locked` (off), `invites.max_outstanding` (20) and `invites.unused_code_days` (30), and the job `invites.prune` (every six hours), are described in [OPERATOR.md](OPERATOR.md) section 3. Nothing to do unless you want to change them.

## Release notes: the plaintext signing seed is gone (migration 0803, G-85)

Migration `0803_drop_plaintext_signing_seed.sql` drops `applications.interactions_signing_seed`. Seeds have lived encrypted in `interactions_signing_seed_sealed` since `0771`, and the old column was emptied by the startup job `apps.seal_seeds` of the release that shipped `0771`. The upgrade order is therefore: first run a release that contains `0771` (any release after it) once, with `server.secret_key` set, then upgrade. If a plaintext seed is left, the migration stops with "run the previous release once to seal seeds" and changes nothing; start the previous release, then retry. The one-version rule does not cover this migration: the release before it still reads the dropped column at startup, so go back to the pre-upgrade dump (the second case of "Rolling back") if you must roll back past it.

## Release notes: the instance bucket is storage mount 1 (migration 1420, BYOS-INS-001)

No operator action. On first start the server records your `[storage]` bucket as mount 1 in `storage_mounts` (the access key's secret is sealed with `server.secret_key`, like every mount secret) and the media tables gain a `mount_id` column that defaults to 1, so existing rows keep pointing at the same bucket and the same keys. The `[storage]` config stays the source of truth for the instance bucket and nothing is copied or moved.

One new refusal: if mount 1 already exists and `storage.bucket` names a different bucket, the server will not start (every existing media row names mount 1, so a silent switch would break them). Restore the old bucket name, or move the objects and then run `sr-cli storage set-instance-mount`. With `storage.endpoint = "memory:"` (test use) no mount is seeded.

## Release notes: the web app is served by the server, behind the session (D-385, SEC-ENT-003)

A visitor without a login now receives only the public files (the landing, sign-in and invite pages, the fonts, the icons, the service worker). The app itself (`/app`, `/app/*` and every in-app path) is served by `sr-server` from the same web build, and only to a request with a valid session; the Compose Caddy no longer serves it. The OpenAPI document (`/api/v1/openapi.json`) and the API reference (`/developers/docs/api`) need a session or a bot token. This is a **one-time hosting change** at the first upgrade that carries it:

1. **Give the server the web build.** Compose: take the new `compose/prod.yml` (step 3); it mounts `SR_WEB_DIR` read-only into `sr-server` as `/srv/web` and sets `SR_WEB__ROOT=/srv/web`. Nothing else to edit: `SR_WEB_DIR` is the variable you already have. Plain-binary install: set `SR_WEB__ROOT=/opt/sideroom/web` (or `[web] root` in `config.toml`) in `/etc/sideroom/server.env`; the unit in `deploy/systemd/sr-server.service` already names that path.
2. **Re-render the Caddyfile.** Replace `compose/caddy/Caddyfile.prod` with the release's file, or, if you have edited it, run `node deploy/scripts/render-caddyfile.mjs --public-url https://<domain>` against your copy (add `--activities` and `--voice-url` as before). The file now has no `try_files` fallback: Caddy answers the listed public paths and proxies everything else to the server. Recreate Caddy: `$dc up -d --force-recreate caddy`.
3. **Order.** Unpack the web build first (step 5), then restart the server (step 7): with `[web] root` set the server **refuses to start** when `app/csp.txt` is missing from the build, so a half-unpacked or old build stops the start instead of serving the app without its policy. `sr-cli preflight` has a row for it ("web root set and app.html present"; a warning when `SR_WEB__ROOT` is unset).
4. **Check.** Signed out, `https://<domain>/channels` answers `302` to `/login?next=/channels` and `https://<domain>/app/anything.js` answers an empty `401`, whether or not the file exists. Signed in, the app opens. `/healthz` is unchanged.

If you serve the web app from some other proxy, reproduce the rule in the Caddyfile: serve only the public paths of `dist/surfaces.json` yourself, and proxy every other path to `sr-server`. Do not add a `try_files` fallback to `index.html`.

## Release notes: guest links and Sketch & Slander (waves 43 and 47, ARC-GST-001, ARC-GST-002, ACT-SKS-001)

Guest links let friends without an account play a voice-channel game from a phone at `https://<domain>/play/<token>`. The release's `Caddyfile.prod` has a `/play/*` block that serves the guest page with its own policy (`connect-src` for `wss://<host>`, `frame-src` for `https://*.activities.<host>`), so **re-render or replace the Caddyfile and recreate Caddy** as in "the web app is served by the server" step 2 above (`$dc up -d --force-recreate caddy`); without it a guest link opens no guest page. Guests need `activities.enabled` (the guest socket `/gateway/guest` exists only then) and a correct `server.trust_forwarded_for` (joins are limited per address). Sketch & Slander is a new house game: build it with `node scripts/build-activities.mjs sketch`, upload and approve it like the others; its prompt packs are read from a shelf through the member's page, so a bucket on another origin needs `SR_CSP_CONNECT` in the web build ([FAQ.md](FAQ.md)). The Arcade page, the House notes formatting and the `platform.showroom` switch (off) need nothing from you. Details: [doc 12](../docs/12-deployment-and-operations.md) section 2.

## Release notes: a mistyped embed host is named; the process reports its CPU time (wave 68, SEC-AUD-223, D-696..D-698, D-706)

- **Migration 2930** adds one partial index on `transcode_jobs`; nothing to do. A transcoding worker built before this release keeps working and shows no GPU name until it is updated; the GPU name needs `nvidia-smi` on the worker's `PATH` (the NVIDIA container runtime provides it).
- **No migration, nothing to configure.** Workers older than this release keep working: they register as before and Admin > Transcoding shows no GPU name for them (it shows the hardware encoders they found). Nothing to do for the phone glyph: the web app tells the gateway its platform by itself.
- **A new warning at start.** An entry of `SR_PEERTUBE_HOSTS` or `SR_MEDIA_HOSTS` that is not a plain host name (`http://tube.example.org`, `tube.example.org/videos`, `*.example.org`, `localhost`, a name with a space) was always ignored; the server now says so once at start, in a `warn` line that names the variable and each entry, and `sr-cli preflight` shows the same in its `web` row (a warning, never a failure). The start is not stopped. Write the host alone: `tube.example.org`.
- **`SR_CSP_*` is unchanged and stays strict:** `http` and `ws` entries are for a loopback host only (a LAN address over `http` is refused, and the message now says loopback). The voice origin of the app policy comes from `voice.endpoints`; there is no separate voice URL setting.
- **`process_cpu_seconds_total`** (user plus system CPU time of the process, whole seconds, refreshed every 15 seconds) is on `/internal/metrics` of `sr-server` and on the standalone SFU's `/metrics`, on Linux only. With the SFU inside `sr-server` it is the whole server's. No alert rule ships with it.

## Release notes: the release web build fits every configuration; the `SR_CSP_*` variables are read at start (wave 67, SEC-AUD-210, D-682)

- **The release web build needs no rebuild for Activities or a separate voice host.** At start `sr-server` adds to the app policy it reads from `app/csp.txt` the origins its own configuration implies: the Activities frame origin when `activities.enabled` is on and the site is https, the host of `voice.endpoints` when it is another host, the hosts of `SR_PEERTUBE_HOSTS` and `SR_MEDIA_HOSTS`, and your `SR_CSP_CONNECT`, `SR_CSP_FRAME` and `SR_CSP_MEDIA` origins. It only adds; a build from source that already holds the origins gets nothing twice. The start-up log has one `app policy (...)` line saying what was added. The `VITE_*` values stay for the dev server and builds from source.
- **Check your `SR_CSP_*` values before upgrading.** They used to be read by the web build only; `sr-server` now reads them and a bad entry stops the server with a message that names the variable and the entry. Each entry is `scheme://host[:port]`, scheme `https` or `wss` (`http` or `ws` for `localhost` only), an optional leading `*.`, entries separated by commas. A space, a semicolon, a quote, a path or a keyword such as `'unsafe-inline'` is refused. The compose file and `.env.prod.example` pass the three variables through; with the systemd unit put them in `server.env`.
- **Migration 2890** adds columns to `video_subtitle_tracks` (forward-only and fast) and fills `source_ordinal` for existing tracks from the newest finished transcode, so staff edits to embedded subtitles survive the next re-encode. Uploaded subtitles live under `subtitles/{asset}/` in storage.
- **`tv.footer_note`** is a new setting (Admin > Settings, group `tv`): a plain-text note of at most 200 characters for the foot of the Live TV guide; empty keeps the built-in text; a change applies without a restart.
- **The beta bug reporter rule (D-684).** `platform.beta_reports` may be on for the closed beta (under 100 accounts) before the lawyer answers Q27; from 100 accounts on leave it off until the answer is in and the privacy notice says what it requires. It is still your switch.
- **Two things the docs now say plainly.** `uploads.standard_limit_bytes` is the free upload size: lowering it lowers what free members may upload, and a member with a perk gets the larger of the perk's size and this one (D-678). A Staff Server feed channel you delete returns at the next start; mute it or hide it from a role to silence it (D-683).

## Release notes: the two server limits are enforced; two settings do something, two are gone (wave 66, D-659..D-662)

- **No migration, nothing to configure.** Check the two new settings in Admin > Limits & settings, group `limits`: `guilds.max_per_user` (default 200: the servers one account may be in; the Staff Server does not count) and `guilds.max_members` (default 25,000: the members one server may have; the Staff Server has no limit). They were documented figures before, but nothing checked them. They are checked now: creating a server and joining one (invite, directory, approved application, adding a bot) are refused at the limit with the codes 30001 and 30019. **A limit removes nobody:** an account or a server already over a limit keeps everything and admits no new membership until it is under. If you need a different number, set it before members notice.
- **`uploads.standard_limit_bytes` and `retention.large_file_days` now work.** The standard attachment size (default 30 MiB) and the lifetime of a large attachment (default 7 days) follow the settings from the next upload, without a restart; attachments already stored keep the expiry they were given. The standard size's range is now 1 MiB to 500 MiB (it was 1 KiB to 100 GiB, which could not work): a stored value above 500 MiB reads as the default 30 MiB.
- **Two settings are removed from the list:** `messages.max_length` (message length stays 2,000) and `invites.credit_price_usd` (the price of an invite credit is `[payments] invite_credit_price_cents`; nothing read the setting). A value already stored for either stays in the table and is ignored.
- **The API gained** error code 30019 and `max_members` on `GET /guilds/{id}`; 30001 now carries the number in force. Regenerate any client you built from `openapi.json`.
- **The web build is in the release.** From the release that carries `sideroom-<tag>-web.tar.gz` (D-668), replace the web build with that file after checking it against `SHA256SUMS`: `tar -xzf sideroom-<tag>-web.tar.gz -C <the parent of SR_WEB_DIR>` (a build from source with your own `VITE_*` settings is still how you keep those settings). The server refuses to start when `app/csp.txt` is missing from the web directory.
- **Staff Server channels.** On the first start after this release `sr-server` creates `#new-members` and `#donations` in the Staff Server if they are missing; there is nothing to run.

## Release notes: Activity package limits are settings; staff code is served only to staff (waves 64 and 65, D-651..D-657)

- **Migrations.** `2790_bug_reports.sql` (the beta bug reporter, below) and `2830_activity_package_uploads.sql` (the sessions of an Activity package uploaded in parts). They only add tables; the previous binary ignores them.
- **No fixed 32 MiB package limit any more.** Three operator settings (Admin > Limits & settings, group `activities`, in MiB, each 32 to 1 048 576) set it: `activities.package.max_mb` (every developer, default 512), `activities.package.reviewed_max_mb` (an application with a staff-approved bundle, default 4096) and `activities.package.platform_max_mb` (an application owned by the Owner or by staff who hold `REVIEW_APPS`, default 32 768); an application gets the largest tier it qualifies for. `activities.package.max_files` (default 20 000) caps the file count. The limit applies to the zip and to the unpacked total. Nothing to do on upgrade; lower the settings if the defaults are more disk than you want to give developers. Packages are never held in memory: the browser sends the zip to your bucket in parts (the same resumable path as the Library, so the bucket's CORS rule from `sr-cli storage ensure-cors` matters), and a job checks it from the bucket. A single request upload remains for a zip of at most 32 MiB. Activity files are served with range support (`Accept-Ranges: bytes`).
- **Guild cartridges have no size limit** (D-656): only the zip's index and `sideroom.json` are read, from the shelf's own storage.
- **The Admin app's code is served only to staff** (D-651, D-652). The web build puts it under `app/staff/`, and `sr-server` sends `/app/staff/*` only to a session whose account has an active staff appointment (answer `404` otherwise, `no-store`). Nothing to configure, and no Caddy change: Caddy already proxies every non-public path to the server. It does mean the web build you unpack (step 5) must be the release's complete `apps/web/dist`, including `app/staff/`. The Admin section labels are in a staff catalog under `app/staff/` and are sent only to staff (D-663).
- **The beta bug reporter** is the platform switch `platform.beta_reports` (Admin > Features), off by default (D-647..D-650). Turn it on for a small beta only (under 100 members, as the switch's description says). Staff read reports in Admin; a report's picture is deleted when it is closed or after 30 days and the report after 90.
- **When the live connection falls behind** the gateway resends state to the affected sessions on the same socket (D-646). No setting.

## Release notes: storage links carry a download type; a bucket that ignores it stops handing out links (waves 61 to 63, D-629, D-637, D-641..D-645)

A link SIDE ROOM signs to a file in a member's or a server's bucket tells the bucket what to answer: a file on the inline media list is served with that media type, everything else as `application/octet-stream` with `Content-Disposition: attachment`. Browser-direct uploads never keep a content type a browser would run (HTML, SVG, XML, scripts and most `text/*` become `application/octet-stream`). The storage connection test (the instance bucket, a server's mount and a member's mount) now has a row `response_type` that checks the provider honours those response overrides. A mount whose stored test has a failed `response_type` row hands out no download links (uploads and the server's own reads are unaffected), and the daily mount recheck runs the check on every live mount that has no such row or a failed one. After this upgrade, a provider that ignores the overrides therefore shows as `failing` within a day. Nothing to do for a provider that honours them (run the connection test again to see the row); for one that does not, move to one that does.

## Release notes: sign-in, seeding and logging changes for operators (waves 60 and 61)

- `sr-cli seed` on a non-local instance (`server.public_url` not a localhost address) needs `--force` and also `--password`, at least 12 characters and not the built-in development password.
- With no mail server configured (the log-only mail transport), the server never writes a message body to its log (CHANGELOG, Phase 60).
- An Activity's old files are kept for a day after an update so people mid-session are not cut off (migration `2670_activity_bundle_superseded_at.sql`).
