# Wave 66 catalog (SEC-AUD-187..189; D-663, D-664)

Branch `fix/p66-catalog` (base `integration/phase66`). Items 1 and 2 done; item 3 (D-664) **stopped on purpose** with a question
(below). No Rust, no route, no OpenAPI change, no migration.

## Done (commits, oldest first)
- `SEC-AUD-187: the staff key audit, the staff message catalog and its door`: `scripts/staff-keys.mjs` (+ `.test.mjs`), the shared graph
  walk `scripts/lib/staff-graph.mjs` (+ `.d.mts`), `apps/web/src/i18n/messages/en.staff.ts`, `i18n/staffCatalog.ts` (+ test),
  `admin/StaffDoor.tsx` (+ tests), `vite/staffChunks.ts`, `test/setup.ts`, `i18n/catalog.test.ts`, `scripts/pseudo-locale.mjs`,
  `scripts/i18n-verify-diff.mjs`, and the move itself: 3202 keys into `i18n/messages/en-staff/` (80 files).
- `SEC-AUD-187: staff delivery rules f and g, and the CI lines for the key audit`: `scripts/check-staff-delivery.mjs` (+ test),
  `.github/workflows/web.yml` (step "Staff message keys": `node scripts/staff-keys.mjs --check`), `repo-checks.yml` (the unit tests).
- `SEC-AUD-188: the docs say the staff wording is gated; the e2e spec for the staff catalog`: `docs/dev/entrance-isolation.md` section 11
  ("Wording", how to add an Admin string), `docs/02`, `docs/07` (three places), `docs/12`, `docs/15` (T63), `docs/dev/owner-checklist.md`,
  `docs/dev/i18n-extraction.md`; `e2e/tests/p66-catalog.spec.ts`.
- `SEC-AUD-187: keep the first paint unchanged; rule f ignores text member code or the changelog also holds` (two findings of the first
  real build, below), a test for it, and the e2e click-through.

## How it works
- **The audit.** `scripts/staff-keys.mjs --report | --check | --write [srcDir] [--modules <json>]`. A module is staff-only by the SAME rule as the
  build: `sortModules` and `STAFF_ENTRIES` moved to `scripts/lib/staff-graph.mjs` and are used by both `vite/staffChunks.ts` (re-exported, its
  existing test is unchanged) and the audit. The audit reads the module graph from source (static and dynamic `import`, `export ... from`,
  `import type` skipped), rooted at the script of each `apps/web/*.html`; a module nothing reaches counts as member. **Proof that the source
  walk equals the build:** `SR_STAFF_MODULES_OUT=<file>` on the build writes the build's own staff module list; `staff-keys.mjs --check
  --modules <file>` gave the same answer (3202 staff / 10198 member / 186 unnamed) as the walk, on two builds.
- **Naming patterns (header of the script).** Generous, reading raw text (comments included, so an odd quote cannot hide a use): (1) exact
  dotted token (`'admin.limits.title'`, `Record<string, MessageKey>` tables, object fields, `t()` calls); (2) literal prefix before a closing
  quote (`'admin.limits.' + x`, names every key that starts with it); (3) template with `${...}` after a dotted literal start
  (`t(\`admin.nav.${id}\`)`, `${...}` matches `[A-Za-z0-9.]+`). Test files, `__tests__`, `src/test`, `.d.ts` and the catalogs are not naming
  modules. **What it cannot see:** a key built from a variable with no literal start (`${ns}.title`). `--report` lists those sites
  ("unresolved dynamic keys"); on this tree all twelve are file names or URLs (`${name}.gif`, `${asset}.png`, `.activities.${host}` ...), none
  names a message key, so `scripts/staff-keys.pins.json` (`{ "member": ["key.or.prefix."] }`) exists as a mechanism but **no key is pinned by hand**.
- **`--write`** (rule as asked): a fragment whose keys are all staff moves whole with `git mv`; a mixed one keeps its member keys and its staff
  keys go to `en-staff/<same name>` (the comment above an entry travels with it); first-paint fragments (named in `en.ts`, excluded in
  `en.rest.ts`) are never touched; a key in `en-staff` that a member module names goes back to `en/`; a second run changes nothing (the script
  runs the audit again and fails if it would still move something; a unit test covers it).
- **Loading.** `en.staff.ts` globs `./en-staff/*.ts` and registers them (`registerFragments`), without `markRestLoaded`. It is a **second staff
  entry** (`STAFF_ENTRIES` now has `src/admin/AdminApp.tsx` and `src/i18n/messages/en.staff.ts`), so member code may import it dynamically
  and the build puts it (and the fragments) in `/app/staff/<hash>.js` with no special case: the plugin now requires every listed entry to be a
  staff-only module. `StaffDoor` asks `/admin/me` first; only on a staff answer it runs `Promise.all([load(), loadCatalog()])` and renders the
  Admin app after both. `i18n/staffCatalog.ts` (only the door imports it) retries as the rest catalog does: one failure keeps the door waiting,
  the second settles it degraded (Admin opens with keys), the tries go on (600, 2000, 5000, 15000 ms), and a late success re-applies the locale
  so the keys turn into text. A member never reaches that line (`/admin/me` refused: tested). `MessageKey` is `string` (`messages/en.ts`), so
  the type already covers staff keys and costs nothing.
- **Pseudo-locale / other tools.** `scripts/pseudo-locale.mjs` `readEnglishCatalog` reads `en-staff` beside `en` (so `vite/pseudoLocale.ts`
  and `en-XA` cover staff text; a production build holds none of either); `i18n-verify-diff.mjs` reads both; `test/setup.ts` and
  `catalog.test.ts` register both. `scripts/i18n-audit.mjs` counts hard-coded text in `.tsx` source and never read the fragments, so it needed no
  change (TOTAL is unchanged).
- **The check.** `check-staff-delivery.mjs` (f): every staff message of 12+ characters that no member fragment holds, and that no member code or
  the changelog holds as prose (see "Found on the first real build"), must not occur in a non-staff file of `dist/app/` or `app.html` (the key is
  printed, never the text); (g): at least one of those texts must be inside `dist/app/staff/`. `scripts/staff-keys.mjs --check` runs in the Web job
  right after it.

## The numbers
- Keys: 13,586; **staff 3,202**, member 10,198, **unnamed 186** (reported, never moved). **Fragments moved whole: 44. Split: 36.** 80 files in
  `en-staff/`, 117 files changed under `i18n/messages`.
- First-paint chunk `app-*.js`: raw 659,516 bytes before and after (identical); gzip **214,611 before, 214,616 after (+5 bytes: the hashed names
  of lazy chunks differ), room 41,389 -> 41,384 bytes** (limit 250 KB, needs 8 KB). A first attempt kept the staff-loaded flag in `store.ts` and
  grew the chunk by 24 bytes gzip (99 raw); it was moved into the lazy loader, `store.ts` and `runtime.ts` are untouched.
- Rest catalog `en.rest-*.js`: 809,145 -> **615,219 bytes raw** (158,312 gzip after). The staff catalog chunk is 195,552 bytes raw (48,385 gzip),
  in `/app/staff/`; staff files 4 -> 6 JS + 1 CSS, 523 KB -> 714 KB raw.
- `check-bundle-budget`, `check-entrance-isolation`, `check-staff-delivery` all pass on the build of this branch. `check-staff-delivery` REPORT now
  lists 2 modules (`lounge/msg/staffActions.ts`, `lounge/shell/platformStaff.ts`); the catalog fragment `p55-staff-1.ts` that the base build listed
  is gone from it (its keys moved).

## Found on the first real build (both fixed)
1. Rule f fired 24 times on the first build: staff texts such as "Review queues" also appear as prose in the What's new page (the changelog) and
   the developer docs chunk. A text that member code or `CHANGELOG.md` holds as plain text cannot be attributed to the catalog, so it is no needle.
   (A genuine leak is still caught: on the BASE build, the current check reports 1,901 `f:` failures, one per staff text, and nothing else.)
2. The first design put the loaded flag into `i18n/messages/store.ts` (first-paint code): +24 bytes gzip. Moved out (above).

## Not done: item 3 (D-664), stopped as the task says
I read the two modules `check-staff-delivery` reports and `Operators.tsx`.
- `lounge/shell/platformStaff.ts`: imported by `lounge/shell/StatusMenu.tsx` (loose's file, not edited) and `admin/useStaffRevoked.ts`; it renders
  nothing. It decides whether the status menu shows "Platform Admin" by asking `GET /admin/me` once per session. **The client has no datum that
  says "staff" before a request**: READY and `/users/@me` carry no staff field (`Me { is_owner, platform_permissions, user_id }` is the `/admin/me`
  answer itself). The nearest thing, a READY guild with `is_staff_guild` (membership of the Staff Server), is not the same fact. So this module
  cannot be gated without a new request or a server field: stopped.
- `lounge/msg/staffActions.ts`: imported by `lounge/msg/Components.tsx`. Its buttons are **not staff-only in who presses them**: `offer` (the offer
  of an appointment goes to a not-yet-staff member), `tester` (an invitation in a DM) and `alert` (AutoMod and join-rate cards, pressed by server
  moderators in any server). Only the `review` and `card` kinds call `/admin/card-actions`; they exist only in Staff Server channels, where the
  store knows `is_staff_guild` of the guild in view. That part is about ten lines with no wording in them, and the words the Lounge shows
  (`lounge.components.staff*`) are named by `Components.tsx` for every kind, so they are member keys. A lazy staff entry for it would move ~300
  bytes and no text, and could not honour "loads only when the account is staff" for the other three kinds. I did not do a partial split.
- `help/Operators.tsx` (`/help/operators`): its route has **no guard** (`router.tsx`, a plain `/help/*` route inside the app, like the rest of the
  handbook), so it is open to members: left as it is.
- No test, no shot for item 3 (nothing changed). `docs/dev/entrance-isolation.md` "Not covered yet" now says exactly this.

## The command for the commander (after merging the other builders)
```
node scripts/staff-keys.mjs --write      # moves any new Admin-only key (ops' notice, caps' labels) to en-staff/; run twice: the second says 0 moved
node scripts/staff-keys.mjs --check
node scripts/build-web.mjs               # or: pnpm --filter @sideroom/web build
node scripts/check-bundle-budget.mjs && node scripts/check-entrance-isolation.mjs && node scripts/check-staff-delivery.mjs
git add apps/web/src/i18n/messages && git commit
```
Notes for the merge: the other builders add `en/p66-<name>-1.ts`; a fragment with only Admin keys moves whole, one mixed with member keys is
split into `en/` and `en-staff/` files of the same name (both are tiny). If a new Admin string is named from a member module too (a shared
component), it stays in `en/`, correctly.

## Tests
- `node --test scripts/staff-keys.test.mjs` (11: staff by one module, member when a member also names it, dynamic prefix and template, unnamed,
  tests are not naming modules, per-fragment counts, `--check` names the key and exempts first-paint fragments, patterns, whole move + split +
  idempotence, move back from `en-staff`, fragment parse/render) and `scripts/check-staff-delivery.test.mjs` (14: a..e as before; needles; f in a
  member chunk and in `app.html` (key printed, text not); g; prose skip). All pass.
- vitest: `admin/__tests__/staffDoor.test.tsx` (7: the Admin app renders only after the staff catalog registered, a member never asks for it,
  a degraded catalog still opens the app), `i18n/staffCatalog.test.ts` (4: the real module registers, retry rhythm, no error after one failure,
  late success re-renders). The existing vitest files of `src/i18n`, `src/admin` and `src/help` plus `p61PlatformStaff` ran in the lock: 61 files,
  436 tests, all pass. `tsc`, eslint and prettier clean on the files I changed.
- **Watched fail:** rule f on the base build (1,901 failures, see above); rule f/g and the `--check` rule by their unit cases (each fixture fails
  for the one rule). The door tests were written with the change (the new `loadCatalog` prop does not exist without it).
- e2e `p66-catalog.spec.ts` (chromium, built dist served by sr-server on 8442): 4 tests pass: a member fetches every non-staff file of the built app with the member session and none holds a title only the staff catalog has (Platform bans, Staff & roles, Cinema transcoding, Donation pricing, Crash reports; the spec first checks that they ARE in `dist/app/staff`); the Owner opens `/admin/access-log` and sees real titles, no `admin.x.y` text, and the staff catalog came from `/app/staff/`; the click-through (staff opens Admin, switches to Platform bans, Invites, Jobs, signs out, a member opens `/admin`: not found); the shots. First run of the first test failed on a title ("Ingest sources") that a member message also holds: replaced by titles that no non-staff file holds (checked by grep on the build). The sr-server was the `p66-ops` build (see Cleanup); the web build is of this branch.

## Visual check
Folder `visual-sweep-shots/p66-catalog` (48 PNG): Admin as the Owner (Dashboard, Limits & settings, Feature modules, Review queues, Runbook, and `/admin/bug-reports`, which this stack redirects to the Dashboard because the section is not offered here) and, as a member, the Lounge home and Settings > My Account; each at desktop 1280x800 and phone 375x812 in Editorial, Lounge Light and Lounge Dark. I looked at 10 of them (Dashboard light desktop, Limits & settings dark desktop and light phone, Review queues editorial desktop, Runbook editorial phone, Features light phone, the Dashboard dark phone, member Lounge light desktop, member Settings light desktop, and the not-found page of the first run): real wording everywhere, no message key as text, layouts unchanged. There are no wave 65 shots of these pages, so the comparison is against the wording itself: the same texts as in the base catalog (the moved fragments are byte-identical entries). The member pages show no key either (a wrongly moved key would show there). The click flow ran as the e2e click-through (above). First shot run took `/settings` for the member Settings, which is not a route (not-found); fixed to `/settings/user/account` and re-shot.

## Proposed inventory rows
- SEC-AUD-187 (D-663): done. The staff message catalog: `scripts/staff-keys.mjs` computes it from the module graph, 3,202 keys live in
  `i18n/messages/en-staff/`, served from `/app/staff/` and loaded by the staff door; `check-staff-delivery` rules f and g; CI step.
- SEC-AUD-188: done. The docs no longer say the catalog is ungated (entrance-isolation section 11, docs 02, 07, 12, 15 T63, owner checklist, i18n guide).
- SEC-AUD-189 (D-664): **open, with a question**: staff-looking Lounge code stays in member chunks because the client has no staff datum before a
  request and the card actions are not staff-only (see above).

## Proposed threat-model text (T63, already edited in `docs/15`)
Mitigation adds: the staff wording is a second catalog under `/app/staff/`, chosen by the module graph (`scripts/staff-keys.mjs`, `--check` in the
Web job), loaded by the door beside the Admin app, and the build check fails when a staff text is in a file outside `/app/staff/` or the staff
directory holds no catalog. Residual: a key a member module also names stays member (a shared label is not secret); the staff-looking Lounge
modules (`platformStaff.ts`, `staffActions.ts`: no wording of the Admin app, a status request and card-button calls) stay in member chunks (D-664 open);
a staff page already open keeps its text in memory; the `en-XA` pseudo-locale of a development or `SR_PSEUDO_LOCALE=1` build holds the text too.

## Docs outside my files
- `deploy/OPERATOR.md:217` (caps' file): "The labels of the Admin sections still travel in the message catalog every member loads (D-658)." ->
  "The wording of the Admin sections is in a staff catalog under `app/staff/`, served only to staff (D-663)."
- `deploy/UPGRADE.md:110`: "The Admin section labels still travel in the message catalog every member loads (D-658)." -> "The Admin section
  labels are in a staff catalog under `app/staff/` and are sent only to staff (D-663)."
- `docs/13-roadmap-and-phases.md` (wave 66 line) and `CHANGELOG.md`: the commander's.

## Changelog sentence
The words of the Admin pages are no longer sent to members: the message catalog is split, and the staff half lives with the Admin code, served only to appointed staff.

## Owner questions
1. Should the account carry a "platform staff" flag (READY or `/users/@me`, for example `is_platform_staff: bool`)? With it the Lounge could show the
   Platform Admin entry without asking `/admin/me` and a lazy staff entry for `platformStaff` becomes possible; today a member's page asks
   `/admin/me` once when the status menu opens and gets a 403. (Default if no answer: leave as is.)
2. The offer, tester and AutoMod card buttons are pressed by non-staff, so they are member code. Is it enough to split off only the
   `/admin/card-actions` call (a few hundred bytes, no text)? (Default: no, not worth a staff entry.)
3. `/help/operators` is open to every member: keep it so (the handbook says what the module switches do)? (Default: yes.)
4. A staff-only text of fewer than 12 characters ("Dashboard", "Jobs") is not checked by rule f (it would match member prose); it still lives in
   the staff catalog. Accept? (Default: yes.)

## Cleanup
Scratch database `sr_p66_catalog_ui` dropped; my server stopped (port 8442 closed). No cargo target of my own was created: the stack used the
`sr-server.exe` and `sr-cli.exe` that the ops builder had built in `p66-ops-scratch/bin` (a first try with the wave 58 `sr-cli` failed on a
migration it did not know), because no other build existed and a cold one costs the machine another half hour; this server code is not what
the catalog changes (web only). The server also needed `SR_SERVER__METRICS_BIND=127.0.0.1:9142` because another builder held 9100. Scratch files: only in my scratchpad folder `catalog/` and in `visual-sweep-shots/p66-catalog`.
