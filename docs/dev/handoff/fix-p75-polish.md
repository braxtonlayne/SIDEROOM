# Wave 75 handoff: polish (SEC-AUD-296, SEC-AUD-297; D-784, D-790)

Branch `fix/p75-polish`, base `integration/phase75` (`cec9595e`). Web only: CSS, one TSX file, one local spec. No string, no route, no Rust, no migration, no stash touched (`git stash list` was empty).

## Done (commits)
- `2f165660` SEC-AUD-296 (D-784): `Avatar.module.css` `.status` border and `.statusIcon` background read `var(--ui-status-ring, var(--ui-bg))`. The token is set in the same rule as the row's background:
  - hover/selected: `lounge.css` `.lng-user-id:hover`, `.lng-dm:hover, .lng-dm[data-selected]`, `.lng-member:hover` (each `--ui-bg-hover`); `profile/profile-p11.css` `.lng-dm-identity:hover` (`--ui-bg-hover`); `shell/shell.css` `.lng-friend-row:hover` (`--bg-base-low`, that row's own hover colour; shell.css is not in the plan's file table, flagged here).
  - **Resting (found on the base, part of the item):** `lounge.css` `.lng-left-col` and `.lng-right` set `--ui-status-ring: var(--bg-base-lower)`, the colour they paint. On the base the ring was `#fff` on the `#f2f3f5` panel even at rest in Lounge Light.
  - No Lounge or theme name in `packages/ui` (D-385): `check-entrance-isolation` clean.
  - Also in this commit: `e2e/tests/p75-polish-shots.spec.ts` (local only, `SR_P75_POLISH=1`, `SR_P75_POLISH_OUT`, `SR_P75_POLISH_THEMES`).
- `a72101f4` SEC-AUD-297 (D-790): `BridgeSection.tsx`: the privacy notice and the `Async` block are in one `ss-form ss-bridge` column (760px max, 20px gap); the inner wrapper is now a fragment. No CSS file change, no string change. Shows while loading or failed.

## Confirmed on the base, then fixed (spec soft checks)
- 296 (Lounge Light, chromium, run on the base with the fix reverted via `git apply -R`, no stash): hovered member row ring `#ffffff` vs row `#e8eaed`; at rest ring `#ffffff` vs the panel `#f2f3f5`; all five kinds failed. Editorial and Dark were **not** run on the base (Light only, for time); the dots handoff measured Editorial `#23221d` on `#2c2a24` and Dark `#313338` on `#35373c`.
- 297 (base, desktop and phone, all three states): notice right edge 1168 vs first card 1100 (desktop), gap 0; phone 359/359 but gap 0.
- After (all three themes, desktop and phone; 2 passed): ring equals the surface in every measured state.
  - Lounge Light: member/DM rest `#f2f3f5`, hover and selected `#e8eaed`, header identity rest `#ffffff`.
  - Lounge Dark: rest `#2b2d31`, hover/selected `#35373c`, identity rest `#313338`.
  - Editorial: rest `#1c1b17`, hover `#2c2a24`, identity rest `#23221d`.
  - Bridge: notice right edge = first card right edge (1100 desktop, 359 phone), gap 20 in every theme.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p75-polish`: 206 PNGs after the fix (178 scale-3 avatar crops: member row rest/hover, DM row rest/hover/selected (desktop), DM identity rest/hover, each for online, idle, do-not-disturb, offline and the phone-glyph form; plus 18 full-page bridge shots: not connected, connected, loading failed, desktop and phone, three themes) and `measure.json`, `bridge-geometry.json`, contact sheets `_*.png`. Base shots (Lounge Light only): `.../p75-polish-base`. **I opened all of them through contact sheets** (six ring sheets, three bridge sheets, each shot visible at 140 to 480 px): no disc of another colour round any dot; bridge edges line up, no horizontal overflow at 375.
Not shot: the user panel (the spec's `.lng-user-id` dot probe found no labelled dot for the owner, so no measurement or shot; the rule is the same as `.lng-member:hover`), the selected DM row on a phone (the list is a drawer on a DM page), the whole-server-mode second notice (needs a whole-guild fake: not shot; it sits in the same column), friend rows (rule added, not shot).

## Tests and checks
- `e2e/tests/p75-polish-shots.spec.ts`: failed on the base (both tests), passes after (chromium, 3.6 min).
- Avatar has no test file and jsdom does not resolve CSS-module values, so no unit test; the spec's computed-colour check stands in.
- web tsc clean; eslint on `BridgeSection.tsx` clean; prettier clean on changed TSX and the spec (not run on the `.module.css`: prettier reflows it, so it was left alone); `i18n-audit --max 0` 0; `check-css-tokens` clean.
- Web build, then `check-entrance-isolation` (clean), `check-bundle-budget` (ok), `check-staff-delivery` (ok). **First paint: 215,898 bytes gzip after, 215,895 base (wave 73's figure): +3, room 40,102.**
- Not run: `check-doc-links` (no doc touched).

## Docs
No doc states the ring colour or the bridge page layout (searched `docs/`, README, i18n). Nothing changed.

## Proposed inventory rows
- SEC-AUD-296 (closes the dots 1 row): "The ring round a status dot is the colour of the surface under it: the avatar reads `--ui-status-ring` (fallback `--ui-bg`), the Lounge's rows set it on hover and selection and the left column and right panel at rest. Checked by computed colour, three themes, desktop and phone (D-784)."
- SEC-AUD-297: "On Server Settings > Discord bridge the privacy notice is in the same 760px column as the cards, with the cards' 20px gap (D-790)."
- Closes with them: the dots 1, 2 (kept as decided, D-785) and 3 (kept, D-786) owner questions.
- Threat-model rows: none.

## Changelog sentences
- 296: "The ring round a person's status dot now matches the row it sits on, so it no longer shows as a white or dark disc when you hover or select a member or a direct message."
- 297: "On Server Settings > Discord bridge, the box that says what the bridge shares now lines up with the cards below it."

## Owner questions
1. The ring follows the row's colour on hover. Offline stays grey (D-785); say if you want it brighter. Default: no.
2. The user panel and friend rows got the same hover token but were not shot; ok to leave? Default: yes.

## Cleanup
Stack stopped by PID (ports 8532, 5532, 9232 closed), lock released, `sr_p75_polish_ui` dropped, `p75-polish-bin` deleted. `.scratch/` in the worktree is untracked and never committed.
