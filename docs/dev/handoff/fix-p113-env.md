# fix/p113-env: SEC-AUD-569 / D-1042, an operator-owned server env file on the Compose route

## Clock times
Started 02:23, committed about 02:30 (machine clock; T = 02:25).

## Blocks a first deployment
Nothing new. Two things would mislead an operator or fail a Discord set-up: the `_FILE` secret form has no mount on Compose, and `required: false` needs Compose 2.24 (see "What I could not prove").

## Done (commits)
One commit, subject `SEC-AUD-569 D-1042 optional operator-owned server env file on the Compose route (ALL READY) (UNRUN: proven by deploy_files.rs and CI)`.

- `deploy/compose/prod.yml`, `sr-server` only: new key `env_file:` (between `depends_on` and `environment:`), one long-form entry, `path: ${SR_SERVER_ENV_FILE:-/etc/sideroom/server.env}`, `required: false`, plus a short comment. The `environment:` list is untouched (same lines, same order).
- `deploy/compose/.env.prod.example`: 3 commented lines at the very end (`# SR_SERVER_ENV_FILE=/etc/sideroom/server.env`). No value, no secret.
- `deploy/OPERATOR.md`: the sentence under "Voice" (about :130) now carries the whole how-to (path, mode, create command, three-line example, recreate command, upgrade keeps it, `prod.yml` wins, `_FILE` caveat); the "Gateway" heading (about :132) points at it.
- `deploy/GO-LIVE.md` section 7: one bullet on the Discord row.
- `docs/12-deployment-and-operations.md` section 3: the "stay file-based operator config" sentence now says where the setting goes on each route.

js-yaml parse of `prod.yml`, `services["sr-server"].env_file` prints `[{"path":"${SR_SERVER_ENV_FILE:-/etc/sideroom/server.env}","required":false}]` (and the `environment` map still has 32 keys). `npx prettier --check` on the five changed files: pass (02:25, exit 0). `node scripts/check-doc-links.mjs`: see the reply (run before the commit).

## Not done (why)
Step 2 (a setting the loader cannot take from the environment): none found, so no `SR_CONFIG` mount. All eight named settings are reachable: figment `Env::prefixed("SR_").split("__")` (config.rs:715) covers `discord.client_id` / `client_secret` (config.rs:552-558), `connections.<provider>.*`, `gif.api_key`, `lookup.tmdb_api_key`, `gateway.max_connections` (:164), `voice.egress_budget_mbps` (:201), `server.metrics_token` (:61), `log.filter` (:445). Every secret among them is in `SECRET_FILE_KEYS` (config.rs:627-646).

## FINDERS
`git grep` for `.env.prod.example`, `OPERATOR.md`, `server.env`, `env_file` across `.github/ scripts/ e2e/ docs/ deploy/`:
- `.github/workflows/release-dry-run.yml:97` copies `.env.prod.example` to `.env.prod`: comment-only addition, harmless.
- `deploy/scripts/prod-smoke.sh:433-439` (`stage_upgrade_apply`) loops over `^[A-Z][A-Z0-9_]*=` keys of the example: my lines are comments, so no key is added to `.env.prod`.
- `scripts/install.test.mjs:127,318,439` read the example as key=value: comments ignored (installer's file; not run by me).
- `scripts/check-ops-files.mjs:137` reads `OPERATOR.md` for links and metric names: I added neither.
- `deploy/INSTALL.md:143,145,162`, `deploy/FAQ.md:35`, `deploy/UPGRADE.md:94,123`: `/etc/sideroom/server.env` ALREADY names the systemd route's `EnvironmentFile`. Same path, other route; see "Proposed rows" 3.
- `crates/sr-core/tests/deploy_files.rs`: `Service::section("environment")` reads the lines indented under `    environment:`; my `env_file:` is a sibling key at 4 spaces, so that parse is unchanged; `every_required_variable_is_in_the_env_example` only counts substitutions without a default (mine has `:-`); `the_server_environment_loads_through_the_real_config_loader` reads only the `environment` section, so it stays passing unchanged.

## CASE FOR VERIFY
(a) The `sr-server` service has an `env_file` key whose entry has a path starting `${SR_SERVER_ENV_FILE:-` that defaults to a path not under `deploy/` (`/etc/sideroom/server.env`), and `required: false`. Reads `deploy/compose/prod.yml`, `sr-server`, key `env_file` (two lines: `      - path: ...` and `        required: false`). Would have caught: a default inside `deploy/` (lost at an upgrade, the original fault), or a required file that stops the stack when absent.
(b) Extend `the_server_environment_loads_through_the_real_config_loader`: after the `environment:` pairs, add pairs from an example server env file (`SR_DISCORD__CLIENT_ID=abc`, `SR_DISCORD__CLIENT_SECRET=s3cret`, `SR_LOG__FILTER=info,sr_server=debug`), build the same toml, load through `Config::from_figment`, and assert `cfg.discord.client_id`, `cfg.discord.client_secret.expose()` and `cfg.log.filter`. Best: take the three example lines from the OPERATOR.md paragraph by their `SR_` prefix, so the page is the test input. Would have caught: a key in the docs the loader does not know (a typo such as `SR_DISCORD__CLIENTID`), or a setting reachable only from `config.toml`.
(c) A key in both: put `SR_VOICE__ENABLED=true` (a listed key) in the example env file; assert the merged map takes the `environment:` value (build it as env_file pairs first, then `environment:` pairs over them). Would have caught: the test's own merge order only; compose's rule itself cannot run here. It keeps the documented claim "prod.yml wins" under a test.

## The written walk
Turning on Discord sign-in on a Compose install:
1. The owner makes the application at Discord and copies the client id and secret (only the owner sees the secret).
2. `sudo install -d -m 0755 /etc/sideroom && sudo install -m 0600 /dev/null /etc/sideroom/server.env` leaves an empty, root-only file.
3. The owner types into it `SR_DISCORD__CLIENT_ID=...`, `SR_DISCORD__CLIENT_SECRET=...` (single quotes if the value holds `$`) and optionally `SR_LOG__FILTER=...`.
4. `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod up -d sr-server` (as root, since the file is root-only: the CLI reads it): Compose sees the changed config, recreates `sr-server` (prints `Container sideroom-sr-server-1  Recreated`, then `Started`); the container gets the env_file keys, then the fixed list over them.
5. Admin > Setup checklist: the Discord row goes green (non-empty `discord.client_id`); the redirect URI to register at Discord is `<public_url>/api/v1/sr/auth/discord/callback` (config.rs:549-550).

Upgrade: UPGRADE.md step 3 replaces `deploy/`; `/etc/sideroom/server.env` is outside it and survives. If `SR_SERVER_ENV_FILE` was set, it is in `.env.prod`, which the existing steps keep. Step 5's `up -d` re-reads the file.

prod-smoke with the file absent: `required: false` makes Compose skip the entry silently (2.24 and newer); `install`, `up`, `run --rm ... sr-cli` all load `prod.yml`, so all go through it.

## FOR THE COMMANDER: reading CI
`prod-smoke.yml` step "Install (deploy/install.sh --yes against prod.yml, https://smoke.localhost)" (`prod-smoke.sh install`) is the first to load `prod.yml` with the file absent. Failure looks like `docker compose` printing `env file /etc/sideroom/server.env not found` (a missing `required: false` effect) or, on Compose older than 2.24, a schema error `additional properties 'required' not allowed`. "Healthz through Caddy" then proves the server started with it. The upgrade stage (`stage_upgrade_apply`) reloads the file and adds no key from my commented lines. `release-dry-run.yml` copies the example only. I touched no script.

## What I could not prove
- Compose version: the tree pins none. `install.sh:75` only checks that `docker compose version` succeeds. `required: false` needs 2.24 (Jan 2024). The CI runner's version is the unknown; an old distro plugin would fail. I built no fallback (an empty file made by the installer) to stay in my row; see proposed row 1.
- Merge order of `env_file` and `environment` on a real engine (documented Compose rule: `environment` wins).
- That the invoking user's `docker compose` can read a 0600 root file (it is the CLI that reads it): walk assumption.
- The `_FILE` form: `SR_X_FILE` names a path inside the container and `prod.yml` mounts no secrets folder, so the task's example `SR_DISCORD__CLIENT_SECRET_FILE` would stop the server at start (missing file is a startup error, doc 12 3.1). The pages therefore say: on this route the secret goes in the 0600 file as the value itself. Also setting both `SR_X` and `SR_X_FILE` is a startup error, and the fixed list sets `SR_SMTP__PASSWORD`, `SR_STORAGE__*_KEY`, `SR_VOICE__TURN__SECRET` and `SR_SERVER__SECRET_KEY`, so those must not be given as `_FILE` in the env file.

## Proposed rows
1. (installer, `install.sh:75`) Check for Compose 2.24 or newer, or create an empty mode-0600 `/etc/sideroom/server.env` when missing.
2. (searchdir, `prod.yml` volumes, optional) a read-only secrets folder mount (for example `/etc/sideroom/secrets:/run/secrets/sideroom:ro`) so the `_FILE` form works on Compose; then OPERATOR.md and doc 12 drop the caveat.
3. Cosmetic: `/etc/sideroom/server.env` is also the systemd route's `EnvironmentFile` (INSTALL.md:145,162, "the secrets"). One host runs one route and both are `KEY=value`.

## Sentences for other people's files
`deploy/UPGRADE.md` step 3 (installer's sentence): add "Your Compose server env file, `/etc/sideroom/server.env`, lives outside `deploy/` and is kept."

## Owner questions (to Fable; defaults chosen)
- Default path `/etc/sideroom/server.env`, shared name with the systemd route: kept.
- Secrets: the value itself in a 0600 file, until a secrets mount exists.

## Closing text for SEC-AUD-569
Fixed (D-1042): the Compose `sr-server` service reads an optional operator-owned env file, `/etc/sideroom/server.env` (variable `SR_SERVER_ENV_FILE`), outside `deploy/`. Any `SR_*` setting (Discord sign-in, connections, GIF and lookup keys, log filter, metrics token, gateway and voice limits) can be set there and survives an upgrade; the fixed `environment:` list is unchanged and wins on a shared key. OPERATOR.md, doc 12 section 3 and GO-LIVE section 7 say so. Proven by verify's deploy_files.rs cases and prod-smoke; not run on a real engine. Open: the Compose 2.24 requirement and the missing secrets mount for `_FILE`.

## PROCESSES
None started (git, node and prettier one-shots only); `strays.sh` shows nothing of mine (see reply).
