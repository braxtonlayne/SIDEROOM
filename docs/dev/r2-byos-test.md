# The R2 test of "bring your own storage" (D-1029): a runbook

> **Written for a local cell, never run; for use on a deployed server.** This page and its spec (`e2e/tests/r2-byos.spec.ts`) were written in wave 109 for a local cell and have not been run. Live tests wait for a deployed server (the owner's rule, 2026-11-18). Before use, the origin (below it is `http://localhost:8860`), the stack's start and the clean-up steps are rewritten for that host. The no-credential rules hold unchanged: no agent ever handles a key, and the owner types the keys into the app's own form. An object-scoped token is enough (D-1032). The order of the checks on the day is in [deployment-day-checks.md](deployment-day-checks.md).

Since D-1032, an Object Read & Write token scoped to the one bucket is enough: the CORS check falls back to a live preflight when the token may not read the bucket's CORS rules, so no Admin token is needed.

For the owner and the commander. One page. **No agent ever handles a key**: the owner types the keys into the app's own form, in their own browser, on the local stack. The spec (`e2e/tests/r2-byos.spec.ts`) holds no credential and must never be given one.

## What the test is for

Real uploads, CORS, links, a shelf item, listing and deletes against a real Cloudflare R2 bucket, which no cell can do (a cell's storage is `memory:`). Everything happens under the prefix `sideroom-test/`, stays under a few hundred MB (the spec uploads about 65 MB: 1 KB, 200 KB, 5 MB, 60 MB and a 16 KB track; a few hundred requests in all), and is deleted by the app's own delete at the end.

## The owner, by hand (before the commander runs anything)

1. **Make an R2 API token scoped to the one bucket** (Cloudflare dashboard, R2, Manage API tokens): the permission "Object Read & Write", limited to that bucket. Keep the key id and the secret in your own password manager. Do not paste them into any chat.
   - *Known risk:* the app's "Check CORS" reads the bucket's CORS rules through the S3 API (`crates/sr-storage/src/cors_check.rs`, `check`), and R2 may refuse that to an object-only token. If the app then says the service refused to read the CORS rules, the mount never passes its CORS check and uploads answer `CORS_NOT_READY`. That is a finding, not a mistake of yours (see the report). To go on, you may instead make the token "Admin Read & Write" for this one bucket: say so to the commander, it is your decision, and revoke the token afterwards like any other.
2. **Set the bucket's CORS** (R2, your bucket, Settings, CORS Policy, Add) for the origin the commander gives you (`http://localhost:8860`). The rule the app needs (taken from `crates/sr-storage/src/s3admin.rs`, constants `CORS_METHODS`, `CORS_ALLOWED_HEADERS`, `CORS_EXPOSE_HEADERS`, and the text `manual_steps` in `cors_check.rs`: origin named, not `*`; methods GET, HEAD, PUT; headers content-type and range; exposed ETag, Content-Range, Accept-Ranges, Content-Length), as the dashboard's JSON:

   ```json
   [
     {
       "AllowedOrigins": ["http://localhost:8860"],
       "AllowedMethods": ["GET", "HEAD", "PUT"],
       "AllowedHeaders": ["content-type", "range"],
       "ExposeHeaders": ["ETag", "Content-Range", "Accept-Ranges", "Content-Length"],
       "MaxAgeSeconds": 3600
     }
   ]
   ```

3. **When the commander says the stack is up:** open `http://localhost:8860` in your own browser and sign in as the e2e Owner (the commander tells you where its test sign-in is written in `e2e/`). Go to User settings, Storage, and connect the bucket yourself:
   - provider: Cloudflare R2; the endpoint and the bucket as the dashboard shows them; **prefix exactly `sideroom-test/`** (with the slash: the spec stops on anything else); **label exactly `sideroom-test`**; your key id and secret.
   - Run the connection test and "Check CORS" in the app. The mount must show as connected with CORS passed. Then tell the commander "connected". Do not send the commander anything else.

## The commander (kept in step with `p109-commander-r2.md`)

1. Read the r2 handoff first (`docs/dev/handoff/fix-p109-r2.md`): if it found that the server logs a key, do not run.
2. A stack of the merged tree on its own port and database, outside the runner's lanes, runner stopped: one cell on 8860/9560 with database `sr_p109_r2` (the cell's public URL is `http://localhost:8860`, which is the origin in the bucket's CORS). The password is read inside the command and never printed.
3. One Playwright run against it with `SR_R2_ON` unset, so the global setup makes the e2e Owner and its stored state, and nothing else.
4. Tell the owner, in one message: the origin, the JSON above, the account to sign in with (where it is written, not its value), what to type into the form, and "tell me when the mount shows as connected". Never ask for a key, never open the form, never screenshot the Storage page, never read `storage_mounts`, never `cat`, `grep` or `tail` the server log.
5. Run the spec once, from the e2e folder of the merged tree, with the environment a shoot job sets (`SR_E2E_WEB=server`, `SR_E2E_SERVER_PORT=8860`, `SR_E2E_BASE_URL=http://localhost:8860`, `SR_CLI_BIN`, the two database URLs):

   ```
   SR_R2_ON=1 npx playwright test tests/r2-byos.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
   ```

   Run it only when the owner is at the keyboard and no builder session runs. A second identical run is allowed once, to tell a flake from a fault; no run with changes unless the owner asks.

## Reading the result

Each line `R2 step <n> <name>: ok` or `FAIL <what>` is one step; the run goes on after a FAIL. The steps and what each proves:

| step | proves |
|---|---|
| 1 upload through the app (CORS) | the app's upload (16 MiB multipart parts, presigned PUT from the browser) reaches the bucket from this origin: the number of PUTs, their answers, whether `access-control-allow-origin` is present (never its value), blocked requests ("CORS: the bucket does not allow this origin"), and that all five objects are listed with the right sizes |
| 2 message attachment and links | the image preview shows (`naturalWidth > 0`); the download link of a text, a PNG and a 5 MB file answers 200 with the right length. **A channel attachment cannot be tested here**: it goes to the instance's own storage, not to a member's mount (`crates/sr-api/src/routes/messages/attachments.rs` has no mount); the spec prints a note |
| 3 shelf item | the indexer takes the track, it lists on the music shelf, opens, and a ranged GET answers 206 with 1024 bytes (the player needs it); `content-range` and `accept-ranges` readable by the page (the CORS expose list); a ranged GET inside the 60 MB object answers 206. **A Study file is not tested** (the readers use the same presign; no PDF or EPUB is made) |
| 4 listing | each object of this run is listed exactly once under the prefix, every key is mount-relative, and a listing of `../` is refused. The app cannot list outside the prefix, so "nothing outside it" is shown only by that refusal and by the owner's dashboard check below |
| 5 delete through the app | the Delete button and its confirm remove each object; the list no longer shows it; its signed URL no longer answers 200; the shelf row of the track is gone |

The **last two lines** are `R2 RESULT objects of this run ... left under the prefix: N (0 is the pass)` and `R2 RESULT objects under the prefix in all: M`. The owner compares M with the dashboard. A line `R2 cleanup: could not delete <key>` names an object the app could not delete: the owner deletes that one key in the dashboard. A `FAIL` is reported, not chased. A line `R2 STOP: ...` means nothing was touched (no mount, wrong prefix, mount not ready).

## Clean-up, in this order (say each as done)

1. The spec's own deletes (above), then the owner looks at `sideroom-test/` in the Cloudflare dashboard (empty, or only what they put there).
2. The owner removes the mount in the app (User settings, Storage), then **revokes the API token in Cloudflare**.
3. The commander stops the cell by its Windows PID (`cell.sh down`), checks ports 8860 and 9560 are closed, runs `DROP DATABASE sr_p109_r2`, deletes `C:/Users/braxt/Desktop/SIDEROOM_wt/p109-r2-cell` by its literal path (its `server.log` goes with it, unread) and `p109-integ/e2e/.state` (the test account's session), then `strays.sh p109-r2-cell --ports 8860 9560`.

**Where a key could linger, and how each place is cleared**

| place | what could be there | cleared by |
|---|---|---|
| the test database | the secret, sealed (encrypted under the server's secret, `crates/sr-storage/src/mounts.rs` `SealedSecret`), the access key id and the endpoint in plain columns | removing the mount (the owner), then `DROP DATABASE` |
| the server's log | **nothing of the mount's fields, from the code**: the request trace logs the method, the path and the request id only (`crates/sr-api/src/router.rs` lines 243 to 256, with the path through `redact_path`; no headers, no body, no query), the CDN trace logs method and path without the query "it carries the signature" (`crates/sr-api/src/routes/cdn/mod.rs` lines 209 to 216), the secret's `Debug` prints `Secret(..)` (`mounts.rs` lines 174 to 178) and the sealed bytes print as a length (`mounts.rs` lines 226 to 230). The storage routes' log lines carry an error text and a mount id (`mounts.rs` 276 to 281, `mount_ops.rs` 146, 206): a provider's error text could name the bucket's host (the account id), never the secret. The log folder is deleted unread in clean-up 3 anyway | deleting the cell folder |
| the test run's artifacts | none: the spec switches the trace, screenshots and video off, prints no URL, and writes its generated files under the test's output folder and removes them at the end | `e2e/test-results` and `e2e/playwright-report` are not produced by this spec; the commander deletes them by literal path if they exist |
| the browser | the form's fields while typing; the browser's own saved-password offer; the session cookie | the owner's own browser: decline the "save password" offer, close the tab, clear the site data for `localhost:8860` |
| Cloudflare | the token | the owner revokes it |
| chat, files, shell history | whatever was pasted | nothing should be: if a key was pasted anywhere, the owner revokes the token at once |

## Costs

Under 70 MB uploaded and, for the downloads, about 6 MB read; a few hundred requests (every 16 MiB part of the 60 MB file is one PUT, plus listings and presigns); no egress to speak of on R2; a few cents at most.
