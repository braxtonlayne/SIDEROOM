# Wave 90, seed builder: SEC-AUD-391 (D-901), the sweep seed's `FAIL suspended app`

T = 05:50 on 2026-11-17. Ready 05:51. Round 1 (lock 05:51 to about 05:54): the base seed and the one candidate fix, on two fresh databases. Round 2 (lock 06:05 to 06:07, taken when it was free; others had had or were taking their first): two more fresh databases.

## The base

On the brief's stack exactly as written (server env: `SR_DATABASE__URL`, `AUTO_MIGRATE`, bind, `PUBLIC_URL=http://localhost:5688`, `memory:` storage, activities on, metrics port; no `SR_ADMIN_REQUIRE_REAUTH`), the seed (`page-sweep.spec.ts` "seed the sweep world", 47 s) printed one non-ok line and no WARN:

```
FAIL suspended app: Error: POST /admin/developers/apps/<id>/suspend -> 403: {"code":900004,"message":"Set up two-factor authentication before using this admin action."}
```

26 steps ok, 1 FAIL.

## The path

- `POST /admin/developers/apps/{application_id}/suspend` is a row of `SENSITIVE_ACTIONS` (`crates/sr-api/src/sensitive.rs:244-248`, gate `StaffSecondFactor`; the same list is walked at `:652-658`).
- The route layer `reauth_gate` (`sensitive.rs:~527-545`) runs before the handler: for a listed route it builds the `StaffCtx` and calls `ctx.require_reauth()`. That is why `portal.rs`'s `admin_suspend` does not name it.
- `StaffCtx::require_reauth` (`crates/sr-api/src/routes/admin/ctx.rs:254-281`) returns Ok at once when `deps.require_reauth` is false; otherwise `NotEnrolled` (the Owner has no TOTP or passkey) gives the 900004 message at `ctx.rs:279`.
- The switch: `crates/sr-server/src/app.rs:1704-1761` reads `SR_ADMIN_REQUIRE_REAUTH` through `sensitive::require_reauth_from_env` (`sensitive.rs:558-570`). **Default is on.** `0`, `false` or `off` turns it off, and the server refuses to start with it unless `server.public_url` is a development host (RDY-SEC-010, D-357, D-198). CI sets it to `0` (`.github/workflows/e2e.yml:195`).
- The brief's stack did not set it, and `fix-visual-sweep.md` "How to run the sweep" did not name it (only the spec's comment did).
- Why the other staff steps pass: they call routes that are not on the sensitive list (Review decision, reports, feature switches and so on); only `suspend` is listed.

## Cause: (a), a stack setting

The brief's stack (and the how-to) lacks `SR_ADMIN_REQUIRE_REAUTH=0`, a development switch that already exists; the suspend route is a listed second-factor route and the sweep's Owner has no second factor.

## Changed

- `docs/dev/handoff/fix-visual-sweep.md` "How to run the sweep": the stack line now names `SR_ADMIN_REQUIRE_REAUTH=0`, with the reason and that it is for a development host only. No code, no spec change (the step's own comment already says it).

## Candidate tried (round 1, fresh DB `sr_p90_seed_ui2`, same stack plus `SR_ADMIN_REQUIRE_REAUTH=0`)

27 steps ok, no FAIL, no WARN, 45 s. Base 47 s. World keys: `world.json` has 14 leaves in both runs, key lists identical (the step's id is not stored in the world).

## Sentence for wave 91's brief (and the how-to)

"Add `SR_ADMIN_REQUIRE_REAUTH=0` to the sr-server env of every test stack (development host only: the server refuses it unless `server.public_url` is a localhost address); without it the sweep seed prints `FAIL suspended app` (900004) and the Review hub has no suspended application."

## Proof (round 2) and the shot

Two fresh databases (`sr_p90_seed_ui2`, `sr_p90_seed_ui3`), stack as the brief plus `SR_ADMIN_REQUIRE_REAUTH=0`: both seeds 27 steps ok, no FAIL, no WARN (`1 passed`), 26 s and 30 s (the base 47 s; these ran warm). World key lists identical to the base's on both (14 leaves; compared as sorted key paths, not ids).

Shot (1): `visual-sweep-shots/p90-seed/admin-review-applications-desktop-dark.png` (Admin, Review queues, All applications, 1280 Editorial). I opened it: three rows, Doodle Board released, Projector Bot development, Sketchy Relay shown as SUSPENDED with its reason, "Suspended by @owner" and an Unsuspend button. The sweep also wrote desktop-light, phone-light and phone-dark of the same page (not looked at in detail; not asked for). The sweep's capture printed an ENOENT for `data/*.png.json` (the shots folder had no `data/` sub-folder yet); the PNGs are written before that and are fine. Re-shoot: on a stack with `SR_ADMIN_REQUIRE_REAUTH=0`, from `e2e/`: `SR_E2E_BASE_URL=http://localhost:<vite> SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<url> DATABASE_URL=<url> SR_E2E_SWEEP=1 SR_SWEEP_ONLY='^admin-review-applications$' SR_SWEEP_OUT=<folder, with a data/ sub-folder> npx playwright test tests/page-sweep.spec.ts -g "theme |in (desktop|phone) .* admin-review-applications$" --project=chromium --workers=1 --retries=0 --reporter=line` (after the seed).

## Proposed rows

- SEC-AUD-391 (closed): the sweep seed printed `FAIL suspended app` (900004) on a stack without `SR_ADMIN_REQUIRE_REAUTH=0`; the suspend route is a listed second-factor route (`sensitive.rs:244`), the switch is default-on. Fixed by naming the switch in the sweep how-to and the stack lines (no code, no server change).
- Threat-model rows: none. Changelog: none (development tooling).
- Owner questions: none. Should the brief's stack env carry the line for every wave (commander's call)?

## Cleanup

Scratch databases dropped (`sr_p90_seed_ui`, `_ui2`, `_ui3`), `p90-seed-bin` deleted, ports 8688/5688/9388 free, `p80-base-bin` untouched, no stash touched.
