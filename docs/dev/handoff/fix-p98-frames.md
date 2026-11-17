# Wave 98, frames: D-946 Circuit and Study at 700 (a record; no app change)

2026-11-17. Clock: started 16:30; spec written and typechecked 16:31; lock `frames` 16:31 to 16:33 (one round: stack, seed with no FAIL line, my spec once); released 16:33; shots opened after. Ports 8761 / 5761 / 9461 free (netstat), `sr_p98_frames_ui` dropped, `p98-frames-bin` deleted, scratch folder removed. Only file besides this handoff: `e2e/tests/p98-frames-shots.spec.ts` (new). No app file, no web build, no cargo.

## What I measured (700 x 812, touch, Owner, account theme `lounge-dark` set by PATCH and read back; skin read back from `data-skin`)
Spec result: 1 passed (21.7 s). Skin frame theme read back: Circuit `lounge-onyx`, Study `lounge-light`.

| | Circuit | Study |
|---|---|---|
| root clientHeight / scrollHeight | 739 / 974 | 723 / 836 |
| scrollTop at the end, read back / max | 235 / 235 | 113 / 113 |
| root `::before` / `::after` | none | none |
| `.sr-ply-bd-backdrop` (content rows) | not in this skin | 8 to 731 (h 723) |
| `.st-dots`, `.st-grain` | n/a | 8 to 731 (h 723), both |
| `.nc-body`, `.nc-shell`, `.nc-shell-plate` | 0 to 974 each (whole) | n/a |
| `.st-body` | n/a | 74 to 834 |
| root / dock / document scrollWidth vs clientWidth | 684/684, 684/684, 700/700 | 668/668, 684/684, 700/700 |

The figures are the same at the start and at the end of the scroll (the layers scroll with the content).

Hit test (`openCollapsed`, then `hitTestAll`, shared tool, no private probe): Circuit 26 controls, 26 pass, 0 fail, 0 not-reached, 0 skipped. Study 22 controls, 22 pass, 0 fail, 0 not-reached, 0 skipped. No taker to name: nothing failed.

## Finding per skin
- **Circuit at 700: no fault.** Every frame layer (`.nc-shell`, `.nc-shell-plate`, both inside `.nc-body`) is 974 px tall, the full scroll height. The end shot shows the outer cyan border and the hazard stripe at the foot, the queue panel whole. 26 of 26 pass, no sideways scroll.
- **Study at 700: hit test and width clean; one frame layer ends part-way (low severity).** The polka dots and paper flecks (`.st-dots`, `.st-grain`, children of `.sr-ply-bd-backdrop`, absolute `inset: 0` in the scrolling root) are the root's padding-box height (723) and so end at content row 731, 105 px before the content ends (836). The skin's cream ground and pink rim are the root's own background and border (`background-attachment` scroll on the root, whole at every scroll position): the end shot shows the pink rim and cream ground continuous to the foot, the foot of the skin whole. What stops is the faint dot/fleck texture over the lowest 105 px (about y 618 down in `study-700-end.png`); it is faint (dots at opacity 0.55, flecks 0.6) and I cannot see the cut by eye in the shot, only in the figures. I did not decode pixels (no image tool here), so "faint, not visible by eye" is my reading of the shot. At 375 wave 95 recorded Study at 723 / 1250: the same layers would stop 527 px short there; wave 95 judged the 375 end shot by eye only. This is the same family as Valve's fault (D-940).
- Crowding or cut in any shot: none that I see. Circuit's marquee title and the Study title are clipped by design (ellipsis / marquee), same as the wave 93 shots at 375. There is no base at 700 to compare with (the 375 shots of wave 93 are the nearest). Not compared further.

## Would D-940's way fit Study (read from `study.css`, nothing changed)
Yes. The two layers are plain backgrounds (`radial-gradient` tile at 18 px; two radial-gradient tiles at 7 x 9 and 11 x 7 px, then `opacity`). They could move onto the root's own background as extra `background-attachment: local` layers, as Valve does (`valve.css` at 2012). Snag: the root's background is two layers with `padding-box` / `border-box` clips, and the dots carry `opacity: 0.55` / `0.6` as element opacity, which a background layer cannot do; the tile colours would need `color-mix(... transparent)` instead. Alternative with no sheet-wide change: size `.sr-ply-bd-backdrop` to the content (e.g. put the art inside `.st-body`, which is already 74 to 834). Either is a small change for a later wave.

## Proposed row (SEC-AUD-439, suggested severity low, UX)
"Study's polka-dot and flecks texture (`.st-dots`, `.st-grain`) ends at the first screen's height when the skin scrolls: at 700 x 812 the layers are 8 to 731 against a scroll height of 836 (105 px short); at 375 the same layers stop about 527 px short (723 / 1250, wave 95's figure). The cream ground and pink rim are whole. Shot `visual-sweep-shots/p98-frames/study-700-end.png`. Fix as D-940 (the texture as `local` layers on the root, or the art placed inside `.st-body`)." Circuit: no row.

## Shots (`visual-sweep-shots/p98-frames`, 4 PNG plus `frames.json`): opened all 4
`circuit-700-start.png`, `circuit-700-end.png`, `study-700-start.png`, `study-700-end.png` (named start/end, not top/end).

## Checks (own tree, after my last edit, 16:31; spec unchanged since the run at 16:33)
e2e `npx tsc --noEmit -p tsconfig.json` exit 0 (16:31); prettier on the spec: unchanged, exit 0; spec run: 1 passed. `check-doc-links.mjs` 16:34 exit 0 ("doc links ok").

## Commander's command (from `e2e/`, stack with the sweep's world, API 8761 / Vite 5761 / metrics 9461, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
`SR_E2E_BASE_URL=http://localhost:5761 SR_P98_FRAMES_ON=1 SR_P98_FRAMES_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p98-frames npx playwright test tests/p98-frames-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Changelog sentence
Test harness only: a new local spec shoots and hit-tests the Circuit and Study player skins at 700 px and records their frame figures.

## Owner questions
None (the Study texture row is a later-wave choice: fix as D-940 or leave, it is faint).

## Lock
After release (16:33) `heavy.sh status` printed `free`; at 16:34, just before the reply, it printed `cue 16:33` (no hold in the name `frames`). No shell of mine is running an acquire.
