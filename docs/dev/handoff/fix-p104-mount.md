# Wave 104 handoff: mount (D-991; SEC-AUD-455's closing text)

Started 14:12, finished about 14:25 (machine clock).

## Step 1 (confirm first): verdict A
- Code read: a job's `mount_id` is `transcode_jobs.mount_id` (migration 1500, `mount_transcode` jobs only, set by `queue_mount_transcode` from a `storage_mounts.id`); `GET /admin/storage/mounts` (`routes/admin/storage.rs:109`, `sr_moderation::list_mounts`) lists `storage_mounts` rows, whose `id` is the same snowflake (mount 1 is the platform bucket, migration 1400). Same id space.
- Seeded world, signed in as Owner: the page's own jobs response gave `mount_id` "1" for all three seeded jobs; the page's own `GET /admin/storage/mounts?limit=50` gave status 200 and **an empty list** (`storage_mounts` has 0 rows on a stack with `SR_STORAGE__ENDPOINT=memory:`: the instance mount is not seeded there). So in the world the cell stays the id; a job's mount can be in the list once a mount exists.
- **Verdict A:** same id space; built. The spec makes two REAL mounts through the API (`POST /users/@me/storage/mounts`, provider s3, no connection test is made) under its own names ("p104 films" and a 60-character label), points the queued and the running job at them with one `UPDATE transcode_jobs SET mount_id` and leaves the failed job on mount 1 (not in the list). **No list was faked.** The mounts and jobs are deleted again in a `finally` (log line `mounts removed`).

## Done (three `D-991` commits: code, spec, cap 14ch; then this handoff)
`TranscodingSection.tsx` only (plus the new test and spec): `useMountLabels()` in `QueueTab` asks `GET /admin/storage/mounts?limit=50` once when the queue tab mounts (`apiFetch`, `retries: 0`) and keeps `Map<id, label>`; on any failure the map stays empty and nothing is said. The table renders at once with ids and gains labels when the list arrives. Cell: label span, a space, then `<span className="adm-mono">{id}</span>` when the map has the id and the label is not empty, else exactly the base. The label is a React text child (never HTML; tested). No string, no sheet, no route. The label span is `display: block; max-inline-size: 14ch; overflow: hidden; text-overflow: ellipsis; white-space: nowrap` (inline style, because no sheet is mine): a long label is cut to one line with an ellipsis, as the State cell clamp hides the rest of its text (textContent stays whole). A cut label takes one line, so no row is taller at 1024 and 900. At 375 the cut applies too (an inline style cannot be media-queried): "p104 films: the l..." although there would be room.

## withReauth (the second request)
`listMounts` in `admin/storage/api.ts` wraps the call in `withReauth`, which on a 403 `SrReauthRequired` shows the re-auth dialog. A section that had no prompt must never get one, so the section does **not** call `listMounts`; it calls `apiFetch` on the same route directly (only the type is imported from `admin/storage/api`). A 403 (re-auth, or an operator holding MANAGE_TRANSCODE without MANAGE_TAKEDOWNS) is swallowed like any failure: ids alone. Seen: the stack runs with `SR_ADMIN_REQUIRE_REAUTH=0`, so a closed window was never exercised on the stack; the unit test covers a 500 (no alert role, id alone). Dev mode StrictMode runs the effect twice: 2 requests seen at load in dev (1 in a build); the spec accepts 1 or 2.

## Figures, base against after (1024 / 900, Lounge Light; same database, same three jobs)
- Column widths job / asset / mount / state / priority / worker / created / actions: base 134.7 / 61.8 / 68.6 / 232 / 79.8 / 75.5 / 94 / 89.2; after 134.7 / 61.8 / **143.6** / 232 / 79.8 / 75.5 / 94 / 89.2. Only Mount moved (limit 160). Table 835.5 -> 910.5; wrapper client 585 (1024) and 496 (900), scrollWidth 835 -> 911, so it still scrolls in its region (`role=region`, `tabindex=0`, `aria-label` unchanged). The x of the columns right of Mount moves (State 591.8 -> 667), as listed. A first try with 16ch gave Mount 161.3: over the limit, lowered to 14ch.
- Row heights at 1024 and 900: 87 / 87 / 89.4 before and after (no row taller). State cell: three-line clamp unchanged (`-webkit-line-clamp: 3`, error block 51 px, text length 184 whole). Edge shade: `background-image` present on the wrapper at 1024 and 900 in both phases (Light and Dark), none at 375 in both. I did not re-cut the shade strips at scroll end (no shade rule changed): not measured.
- 375 (stacked): heads and column widths equal; rows 221.4 / 220.4 / 272.4 -> 238.4 / 237.4 / 272.4 (+17 for the label line; the failed row with no label equal); `main` scrollWidth 375 = clientWidth 375, document sideways 0 everywhere. Hit test at 375 (`openCollapsed`, `hitTestAll` over `main`): 12 rows, 0 not pass; the six table buttons are 44 px high.
- No alert with text on any page; no re-auth dialog.

## Tests
`apps/web/src/admin/__tests__/p104-mount.test.tsx` (5): label + id (textContent `Ada films 5550001`, id in `adm-mono`); label as text not HTML; list 500 -> id alone, no alert role; mount not in list -> id alone; no `mount_id` -> empty cell. **On the base file (put back from HEAD before the change) the first two fail (2 failed, 3 passed)**; with the change 5 passed. `admin/__tests__/cinema.test.tsx` (unedited): 23 passed.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-mount`, 6 PNG + `measure-*.json`; I opened all 6)
- `base-queue-1024-lounge-light.png`: ids only, Mount head over a wrapped 18-digit id (3 lines), shade at the right edge. Seen.
- `queue-1024-lounge-light.png`: wider Mount column; "p104 films" over its id; "p104 films: the l..." (cut) over its id; the failed row `1`; State clamp unchanged; shade on the right. Seen.
- `queue-1024-dark` is `queue-1024-lounge-dark.png`: the same in Dark, readable. Seen.
- `queue-900-lounge-light.png`: the same at 900; State text cut by the wrapper edge as the table scrolls; shade on the right. Seen.
- `queue-375-lounge-light.png`: stacked; `Mount` label, then label text and id on the next line; failed row id alone; buttons 44 px. Seen.
- `queue-1024-long-label.png`: the same page as the 1024 light shot (the running job carries the 60-character label on the same page as the short one), so it looks identical. Seen.

## STACK-ROUND
14:17:56 to 14:22:52 (acquire gave a slot at once; `status` read `mount 14:17`; after release `status` showed only `chip 14:17`). Seed: no `FAIL` line. Runs: after (twice with spec fixes), base (the file put back by `git checkout ccb3bacc --`, restored by `git checkout HEAD --`), after again with 14ch (final). Two early after runs failed on my own spec (a mount endpoint must be https; a too-strict alert and request count), fixed in the spec.

## TYPECHECK
`bash cli-agents/tc.sh run mount bash -c "cd .../p104-mount/apps/web && npx tsc --noEmit -p ."` at 14:23-14:24 after the last code edit: exit 0. Also e2e `tsc --noEmit` exit 0; prettier --check clean; eslint on both files exit 0; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok. No web build, no bundle check (Admin lazy chunk).

## Who imports what
`TranscodingSection.tsx` is imported only by `apps/web/src/admin/AdminApp.tsx:36` (the Admin lazy chunk). It now imports `apiFetch` (already in Admin) and a type from `admin/storage/api` (erased).

## STALE
None: `p103-trans-shots.spec.ts` asserts the head and `data-label` ("Mount") only (lines 240-241), never the cell text or a column width.

## Docs
Nothing in `docs/` states the Mount cell as the bare id except `fix-p103-trans.md` (history).

## For the commander
- Command: `cd e2e && SR_P104_MOUNT_ON=1 SR_P104_MOUNT_PHASE=after SR_E2E_BASE_URL=http://localhost:5803 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P104_MOUNT_OUT=<dir> npx playwright test tests/p104-mount-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (needs the sweep world and `psql`; phase `base` after putting `TranscodingSection.tsx` back to `ccb3bacc`).
- Closing sentence for SEC-AUD-455: "The Transcoding queue's storage column and its stacked label read Mount (key `admin.transcoding.col.storageMount`); the cell shows the mount's label and then its id, the label read from the Storage mounts list asked once when the queue opens (D-991), and the id alone when the list does not hold the mount or cannot be read."
- Changelog sentence: "The Admin Transcoding queue shows each job's mount by its label (cut to one line when long) before its id."
- Owner question: the instance mount (id 1) is not in the list on a memory-storage stack, and the list needs MANAGE_TAKEDOWNS. Should the job row carry a `mount_label` so the page needs no second request? That is a Rust change (a `mount_label` on `JobSummary`, joining `transcode_jobs` to `storage_mounts` in the `/admin/transcode/jobs` handler); not built, the planner's.

## Clean-up
Stack stopped by PID tree (sr-server 38260 / 7248, Vite 13720 tree), ports 8803 / 5803 / 9503 free, database `sr_p104_mount_ui` dropped, `p104-mount-bin` deleted, test mounts and jobs removed by the spec. No stash touched.
