# Wave 100, seed (D-952): a Transcoding row and an Arcade tier, made by a support function and shot

Branch `fix/p100-seed` (base `integration/phase100`). Files: `e2e/support/p100-seed.ts` (new), `e2e/tests/p100-seed-shots.spec.ts` (new), this handoff. No app file, no Rust, no cargo, no web build. Clock: started 04:53, round 1 05:06 to 05:08, round 2 05:18 to 05:19, handoff 05:25.

## Done

- `seedTranscodingRow(ctx?)` (sync): three rows in Admin > Cinema transcoding > Queue (queued, running at 42.5 %, failed with a long error holding an unbroken file name), names `p100-seed-<random>`; returns `{ suffix, jobs[{id,state,key,text}], remove() }`.
- `seedActivityTier({ owner }, tier?)` (async): an Arcade "Coming soon" game with a content tier (default `TIER_2_REIMPLEMENTATION`), name `p100-seed-<random>-original|reimpl`; returns `{ id, title, summary, tier, repoUrl, remove() }`.
- Both ran in rounds 1 and 2 on the stack and the rows showed. Each call makes new rows, never reuses one, so a second call in one world is safe. The spec removes what it made in a `finally`.
- Commits: `891f7cdb` (support file and first spec), then the round 2 spec (see `git log`) and this handoff.

## How each row is made (what I read, and what that means)

- **A Transcoding row: only by inserting into `transcode_jobs` in the stack's own scratch database (psql, `-c`, password by `PGPASSWORD`, the precedent is `jukebox.spec.ts` / `arcade-sandbox.spec.ts`).** No `sr-cli` command queues a job (`sr-cli --help`: nothing for transcode). The two product routes that queue one both need a stored video: a Cinema upload (`POST /admin/cinema/assets`, `crates/sr-api/src/routes/cinema_media/mod.rs:159`; `MediaService::begin_upload` `crates/sr-media/src/service.rs:298`, whose `complete_upload` at `:393` heads the object and only then queues the probe) and a mount conversion (`POST /storage/mounts/{id}/transcode`, `crates/sr-api/src/routes/storage/transcode.rs`, `queue`, which heads the key in the bucket before `queue_mount_transcode`). On the stack's `SR_STORAGE__ENDPOINT=memory:` (`crates/sr-server/src/app.rs:303`) presigned URLs are fake `memory://` ones, and no other route writes a `.mp4` key (chat slots and the shelf use the same fake URLs), so no video can be put there. A stack with a real S3 (Garage, `docs/dev/storage-live-test.md`) would allow the product route. The inserted row is a `mount_transcode` job of the instance mount (1), the table `GET /admin/transcode/jobs` reads; the spec checks all three ids show in the Admin table.
- **An Activity with a tier:** a featured or cartridge Activity takes its tier from the bundle's `sideroom.json` (D-568; `CartridgeInput` has no tier), so no API sets it. The one place a tier is set by an API is the Operator's Arcade Coming soon entry: `POST /admin/arcade/coming-soon` (`crates/sr-api/src/routes/arcade.rs:410`, `ComingSoonInput.tier`), set in Admin > Arcade by the "Content tier" select (`apps/web/src/arcade/ArcadeAdmin.tsx`), seen by members on the Arcade page (`/games`, `ArcadeSections.tsx` `ComingSoonSection`: the corner tag and, for the reimplementation, the "Desktop app only / Needs your own copy of the base game" line from `GameCard.tsx` `TierLine`). I took "Activity with a tier" to mean this (owner question 2).

## Figures (Lounge Light; the three Transcoding rows; every measure in `visual-sweep-shots/p100-seed/measure.json`)

| | 1024 x 768 | 900 x 768 | 375 x 812 touch |
|---|---|---|---|
| wrapper scrollWidth / clientWidth | 700 / 585 | 700 / 496 | 700 / 343 |
| wrapper role / tabindex | none / none | none / none | none / none |
| `adm-table-stack` (rows stack) | no | no | **no** (header row and columns stay) |
| document sideways scroll | 0 | 0 | 0 |
| seeded text whole (string in `textContent`) | yes | yes | yes |
| element cut (overflow not visible, scrollWidth > clientWidth) | none | none | none (the wrapper itself scrolls) |
| row heights (px) | 87 / 480 / 87 | 87 / 480 / 87 | 87 / 480 / 87 |
| hit test, 9 controls | 9 fail the 44 px probe (28 px buttons; desktop, informational) | same | **3 fail**: the three "Logs" buttons (56x28); the point 21 px below takes the Cancel or Re-run button of the same cell |

Arcade (Coming soon, `data-theme` read back `lounge-light`): both seeded cards whole (title, summary, tag "Original" / "Clean-room", tier line and its aria-label "Content tier: Clean-room reimplementation" for the reimplementation); no sideways scroll at 1024 or 375; hit test at 375: 6 controls, 3 fail, all the "Repository" link (72x17; the card body takes the point, not a control); "Notify me" passes. At 1024 the 28 px buttons fail the 44 px probe (informational). Admin > Arcade at 1024: the "Content tier" select changes from "Original" to "Clean-room reimplementation" and shows it; no cut, no sideways scroll.

## Faults (proposed rows; nothing fixed, I own no app file)

1. **(tables' row, SEC-AUD-441; low)** The Transcoding queue table is 700 px wide at every width and its wrapper (`div.adm-table-wrap`, `TranscodingSection.tsx`) scrolls sideways (700/585 at 1024, 700/496 at 900, 700/343 at 375); it has no role, name or tab stop, and is not `adm-table-stack` on a phone, so the Created and Actions columns are off screen at 375 and 900. Tables' `TableRegion` covers the stop; whether this table should stack on a phone is a choice for the commander.
2. **(new, low)** Admin > Cinema transcoding: a failed job's error text goes into the narrow State column (about 80 px) and wraps one word per line, so that row is 480 px tall at 1024, 900 and 375 (the others 87 px). Suggest a width cap and a clamp (the log has the full text).
3. **(new, medium)** Admin > Cinema transcoding at 375 touch: the "Logs" button (56x28) fails the hit test, and the taker is another control: the point 21 px below its centre lands on "Cancel job" or "Re-run job" in the same cell (`div.adm-actions`, stacked buttons, 28 px each). A tap meant for Logs can act on a destructive neighbour (Cancel has a confirm dialog; Re-run has a reason dialog). All three rows. This is the base: nobody changed the section.
4. **(new, low)** Arcade, Coming soon card at 375: the "Repository" link is 72x17 and fails the 44 px probe (the card body takes the point). A kit text-link target, not a neighbour.
5. **(new, low)** Admin > Arcade lists a Coming soon entry by its title only, so after adding it staff cannot see the tier they set (the candidates list above it does show a tier chip).

## Shots (8 in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-seed`, all opened, and the round 1 set as well, which is why the spec now takes each shot before the hit test scrolls the page)

`transcoding-1024-lounge-light.png`, `transcoding-900-lounge-light.png`, `transcoding-375-lounge-light.png`, `transcoding-375-table-lounge-light.png` (the table in view), `activity-tier-1024-lounge-light.png`, `activity-tier-375-lounge-light.png`, `activity-tier-375-reimplementation-lounge-light.png`, `admin-arcade-tier-1024-lounge-light.png`. The shipped Arcade theme and the light one are the same shot: the account's main theme and `sectionThemes.arcade` were both set to `lounge-light` and read back.

## Tests

- `p100-seed-shots.spec.ts`: round 1 (05:06) passed, 31 s; round 2 (05:19) passed, 32 s. Not run against the base: it has no app change to compare. The world was seeded once on `sr_p100_seed_ui` (no `FAIL` line, 28 `ok`); round 2 reused that world and its `e2e/.state` on the same database.
- Checks after my last edit: see the block at the end of this file.

## The commander's command

From `e2e/`, on a stack with `SR_ADMIN_REQUIRE_REAUTH=0` and the sweep world seeded on the same database (ports 8784 / 5784 / 9484):

    SR_P100_SEED_ON=1 SR_E2E_BASE_URL=http://localhost:5784 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
    SR_P100_SEED_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-seed \
    npx playwright test tests/p100-seed-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line

It needs `psql` (on PATH, or `PSQL`, or the default Windows install) and the stack's `DATABASE_URL`. No invoices, no rate source.

## How a later spec uses the two functions

    const tx = seedTranscodingRow();                                   // psql into the stack's DB; 3 rows in Admin > Cinema transcoding
    const tier = await seedActivityTier({ owner: ownerCtx.request });  // Arcade Coming soon entry with a tier
    try { /* shoot */ } finally { tx.remove(); await tier.remove(); }

## Owner questions

1. May a support function insert job rows with SQL into a test database? It is the only way on a `memory:` stack; the alternative is a stack with a real S3 bucket, which then uses the product route.
2. Is a Coming soon entry the "Activity with a tier" you meant? A built Activity's tier is set by its bundle, not by a route, so it cannot be seeded without a bundle.

## Docs

None changed: no fact in `docs/` changes. `check-doc-links.mjs`: see below.

## Cleanup and lock

Stack stopped by PID (the Vite listener was killed by its PID after the first round left port 5784 up), ports 8784 / 5784 / 9484 free (read), the database dropped, `p100-seed-bin` deleted, no media file written, nothing queued or running. `heavy.sh status` outputs: see the end of this file.

## Checks after my last edit (05:20, own tree)

- `npx prettier --check support/p100-seed.ts tests/p100-seed-shots.spec.ts` (from `e2e/`): exit 0.
- `npx tsc --noEmit -p tsconfig.json` (from `e2e/`): exit 0.
- `node scripts/check-doc-links.mjs`: "doc links ok", exit 0. (The doc was edited only by adding this block and the status line below; no link in it.)
- Not run: web typecheck, eslint, i18n audit (no web file changed), any build.

## Lock

`heavy.sh status` just before the hand-back (05:21): `tables 05:19`. No hold in my name, so no release was needed; the last release was at 05:19 (`released`, then `free`). No shell of mine is waiting on `acquire`. `git stash` was not used; I saw no stash entry.
