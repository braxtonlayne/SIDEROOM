# Wave 73 handoff: status (SEC-AUD-286..288; D-768, D-769, D-773)

Branch `fix/p73-status`. Web only (`packages/ui` values, one test, docs); no migration, route, Rust or string.

## Done (commits)
- `SEC-AUD-286, 287`: `packages/ui/src/tokens/themes.ts` (values), `tokens-contrast.test.ts`, `docs/10-design-system-and-theming.md`, `docs/specs/lounge-chat.md` (the glyph sentence: one clause).
- `SEC-AUD-286, 287: status shots spec (local only)`: `e2e/tests/p73-status-shots.spec.ts` (`SR_P73_STATUS=1`, out `SR_P73_STATUS_OUT`).
- `SEC-AUD-288`: `CHANGELOG.md` (Phase 73), `deploy/UPGRADE.md` (wave 73 section). See "Changelog sources".
- This handoff.

## Hover token
Every theme's hovered member row (`.lng-member:hover`) and hovered DM row (`.lng-dm:hover`, also `.lng-dm-identity:hover`) use `--ui-bg-hover` (lounge.css:671, :1359; profile-p11.css:271). Values: Lounge Dark `#35373c`, Light `#e8eaed`, Ash `#404249`, Onyx `#1a1a1d`, Editorial `#2c2a24`, Side Room `#23251d`.

## Confirmed on the base: pairs that failed (test widened to every theme and to the hovered row)
Base values, ratio:
- Lounge Light online `#23a55a`: on `--ui-bg-raised` / `--bg-base-lower` `#f2f3f5` 2.87; on `--bg-base-lowest` `#e3e5e8` 2.52; on hover `#e8eaed` 2.64.
- Lounge Light do-not-disturb `#f23f42` on `--bg-base-lowest` `#e3e5e8`: 2.99 (not in the plan).
- Lounge Ash do-not-disturb `#f23f42` on hover `#404249`: 2.66 (not in the plan).
- Editorial do-not-disturb `#b4503f` on hover `#2c2a24`: 2.84 (as predicted). Side Room's 3.07 on its hover passed.
- New amber check on the base: Editorial, Side Room, Side Room Broadcast idle `#c2b08f` (hue 39, saturation 0.29) fail the 0.5 floor. Lounge `#f0b232` and Lounge Light `#b07800` pass (hue 41, saturation 0.87 and 1.0); band set: hue 30..50, HSL saturation at least 0.5.
No pair is left out of the test; none needed.

## Values (old -> new, ratios)
- Editorial and Side Room idle `#c2b08f` -> `#d9a441`: Editorial 7.08 (base) / 7.66 (raised) / 8.26 (lowest) / 6.38 (hover); Side Room 8.40 / 7.54 / 6.90 (hover). Hue 39, saturation 0.67.
- Editorial and Side Room do-not-disturb `#b4503f` -> `#cc5c47` (same hue, a little lighter and more saturated): Editorial 3.94 (base) / 4.26 (raised) / 4.59 (lowest) / 3.55 (hover); Side Room 4.67 / 4.19 / 3.83 (hover). Side Room changed too so the family shares one value (its old 3.07 passed). Still plainly red beside the amber.
- Lounge Light online `#23a55a` -> `#168a49`: 4.41 (white) / 3.97 (raised) / 3.49 (lowest) / 3.66 (hover). Hue 146, saturation 0.72: the "clear, saturated green" test passes. (The plan's `#1a9650` is 3.01 on the lowest surface: too tight.)
- Lounge Light do-not-disturb `#f23f42` -> `#e5383b`: 4.23 (white) / 3.81 (raised) / 3.35 (lowest) / 3.51 (hover).
- Lounge Ash do-not-disturb `#f23f42` -> `#fa5f61`: 3.28 on hover `#404249`, 3.72 on its base, 4.32 on raised.
- Unchanged: `--ui-warning`, `--ui-danger`, `--ui-success`, Lounge Dark and Onyx, all offline values, Editorial online `#34c25f`.
`themes.ts` comments name no section; `check-entrance-isolation` is clean.

## Tests
`packages/ui/src/theme/tokens-contrast.test.ts` (single file, 71/71 pass): the all-surface status check now runs for every theme id and lists `--ui-bg-hover`; new "the idle colour is an amber" per theme. Watched fail on the base values (6 failures: Lounge Light, Ash, Editorial all-surface; Editorial, Side Room, Side Room Broadcast amber) before the values changed. `packages/ui` tsc, eslint, prettier clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok.

## Shots
`visual-sweep-shots/p73-status/new` (72) and `/base` (72; base `themes.ts` from `1a0ae040` put in my own tree for one run, then `git checkout`; no stash). Per theme (Editorial, Lounge Light, Lounge Dark): member list desktop, hovered list, profile card, user panel, DM list desktop and phone, member list phone (real swipe), a hovered member row and a hovered DM row for each of the five (online, idle, do-not-disturb, offline, phone), a scale-3 shot of each row, of the glyph and of the whole list. **I opened all 72 of `new`**; of `base` I opened two (zoom-list editorial and light) as proof. Result: in all three themes each dot reads as its colour on a hovered row (green, amber, red, grey; the phone glyph green). Editorial: idle is now plainly amber, no longer a tan near the grey offline dot; dnd red and amber are told apart. Lounge Light: online is a deeper green, still clearly green beside red and amber. Lounge Dark is unchanged by design. Side effect: in Lounge Light the rail's add-server, explore and music icons (they use the online token) are the deeper green too (visible in `dmlist-*-lounge-light.png`), still green and readable.
Not done: a dot on an avatar with a decoration and on the smallest avatar size (no decoration seeded; the dot geometry is untouched).

## Bundle
`pnpm --filter @sideroom/web build` ok. First paint `app/app-CK0YCX0z.js` 215,895 bytes gzip (room 40,105); wave 72's was 215,870: +25 bytes, from the base merge or the values; no static import added. `check-bundle-budget`, `check-entrance-isolation`, `check-staff-delivery` all ok.

## Docs updated
`docs/10-design-system-and-theming.md` (status colours paragraph: the values, the hovered row as a surface, the two checks), `docs/specs/lounge-chat.md` (glyph sentence). Searched `docs/` and `apps/web/src` for the old hex values: `#c2b08f` elsewhere is `--sr-gold` or `--ui-warning`, unchanged; `docs/open-question-answers.md` and old handoffs are history.

## Changelog sources
Prose clauses: from `p73-prose`'s committed handoff ("Changelog sentences"). Status clause: mine. Developers and Library clauses and the UPGRADE section: from the plan (uploads had committed nothing at 14:57). prose2 (SEC-AUD-291, bio preview) had no handoff: left out, the commander adds it.

## Proposed inventory rows
- SEC-AUD-286: "A hovered member or DM row is a surface a status sits on: `tokens-contrast.test.ts` checks online, idle and do-not-disturb at 3:1 on it and on every surface, for every theme. Lounge Light online `#168a49` and do-not-disturb `#e5383b`, Lounge Ash do-not-disturb `#fa5f61`, Editorial and Side Room do-not-disturb `#cc5c47` (D-769, D-773)."
- SEC-AUD-287: "Editorial's and the Side Room themes' idle colour is an amber `#d9a441` (hue 30 to 50, saturation at least 0.5, checked); `--ui-warning` unchanged (D-768)."
- SEC-AUD-288: "Changelog and upgrade notes of wave 73."
- Threat-model rows: none.

## Owner questions
1. Editorial's idle amber `#d9a441` and do-not-disturb `#cc5c47`: keep, or separate them more? Default: keep.
2. Lounge Light's rail icons (add server, explore, music) are now the deeper green; fine, or give them their own token? Default: fine.
3. A dot on a decorated avatar and on the smallest avatar is still unshot: seed a decoration next wave?

## Cleanup
Stack stopped by PID (ports 5512, 8512), `sr_p73_status_ui` dropped, lock released, `p73-status-bin` deleted. No cargo target. Untracked `.scratch/` in the worktree is never committed. No stash touched.
