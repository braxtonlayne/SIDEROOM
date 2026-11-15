# Wave 79 handoff: tools (SEC-AUD-312, D-814)

## Done
- Commit `dc7341ba`: `apps/web/src/tools/utl-dev.css` reads the theme first on every colour, radius and font line (`var(--ui-x, var(--sr-x, literal))`, literals kept). Mapping as the contract: text, text-muted (unchanged, already `--ui-text-muted`), bg-0 to `--ui-bg`, bg-1 to `--ui-bg-raised`, bg-2 to `--ui-bg-hover`, line to `--ui-border`, radius to `--ui-radius`, font-body to `--ui-font-body`, font-mono to `--ui-font-mono` (the theme sets have it). Gold as border/accent-color/die border/coin/wheel border: `--ui-accent`. Gold as text (`.ud-btn-primary`, finished `.ud-clock`): `--text-link` then `--ui-accent`. Gold as fill (`.ud-btn[aria-pressed=true]`): `--ui-accent` with `--ui-accent-ink` on it. The wheel pointer reads `--ui-text`. I did not measure accent-as-text on white because text uses `--text-link` first.
- `tools/tools.css`: no change. Its only `--sr-*` reads are `--sr-overlay-zoom` (a zoom factor, not a theme colour) in two lines; nothing to do.
- No `.tsx` change. The notes file field showed "No file chosen" in full in both phone shots (`.ud-root input` is already 100% wide).
- Spec `e2e/tests/p79-tools-shots.spec.ts` (local only, `SR_P79_TOOLS=1`; `SR_P79_BASE=1` = base case).

## Confirmed on the base (run, change held back; folder `visual-sweep-shots/p79-tools-base`, 58 shots, run cut by my 9 min timeout before the end of Lounge Dark phone)
Lounge Light, hash tool: drawer background `rgb(255, 255, 255)`, tool text `rgb(236, 230, 214)` (cream on white), field `rgb(22, 22, 19)`, border `rgb(42, 43, 36)`, radius 2px, button `rgb(28, 29, 24)`; the kit's tokens there are `rgb(242, 243, 245)` / `rgb(49, 51, 56)` / `rgb(201, 204, 209)` / 8px. Lounge Dark: field `rgb(22, 22, 19)` in a `rgb(43, 45, 49)` drawer, 2px radius.

## After (shots `visual-sweep-shots/p79-tools`, 71, one run, spec passed)
Lounge Light: field `rgb(242, 243, 245)`, text `rgb(49, 51, 56)`, border `rgb(201, 204, 209)`, radius 8px, text on field 11.39:1 (all equal to a probe carrying the kit's tokens; asserted for all 17 tools x themes, plus button background, text on drawer >= 4.5, no sideways scroll). 17 tools in Lounge Light and Dark at 1280x800 and 375x812 (68) plus hash, notes and pomodoro in Editorial desktop (3). Typed/set: JSON, regex, base64, url, jwt, md table, counter, diff text; a file in hash; a roll of dice; a coin flip; a running pomodoro. Click-through (spec, desktop Light and Dark): open drawer, switch json, hashes, dice, stopwatch, start and stop a timer: ok.
I opened 12 of the 71 shots myself (hash light/editorial/base, regex light, notes dark phone, pomodoro light phone, wheel light, dice dark phone, diff light phone, plus the base hash); the other 59 are covered by the spec's computed assertions and the same CSS, not by me looking. Say so honestly.

## Findings, not fixed (not mine)
- `tools/dice/WheelTool.tsx:53-55` paints the wheel face with inline `var(--sr-bg-1/2, #14120f/#1c1914)`: the wheel is a near-black disc in Lounge Light (visible in `wheel-lounge-light-desktop.png`). Needs a `.tsx` change to `--ui-bg-raised`/`--ui-bg-hover` first (D-814).
- Regex and Diff show a tall empty gap under the first field (an empty `.ud-field`/`p` in their markup, pre-existing, same on base).

## Imports, Side Room, tools list
- `utl-dev.css` is imported by `tools/dev/parts.tsx` and `tools/timers/TimerPanel.tsx`; `tools.css` by `tools/drawer/DrawerHost.tsx`, which `DrawerGate.tsx` loads with `lazy()`. Dev tools load by `import()` from `registry.ts`. All lazy: no web build.
- Side Room: the drawer is a Lounge overlay, so it is opened under the main theme; the Library, Cinema and Arcade rooms did not offer a tools drawer in my try and I shot no Side Room room (not tried; not found). Editorial is the Lounge's sideroom-like set; `--ui-bg-raised` there differs from old `--sr-bg-1` (`#1c1d18` vs `#161613`): the fields in the editorial hash shot are that slightly lighter value.
- Tools using `.ud-*`: audio (own css), base64, counter, diff, hash, json, jwt, mdtable, regex, url, coin, dice, loot, wheel, notes, pomodoro, stopwatch, countdown (timers: TimerPanel, VoiceSyncControl).

## Checks
prettier clean (css, spec), e2e tsc clean, `i18n-audit --max 0` 0, `check-css-tokens` clean, `check-doc-links` ok. No vitest asserts a colour in tools (not run). No heavy run beyond the lock rounds (spec runs 4 min).

## For the commander
- Inventory SEC-AUD-312: "The tools drawer's own parts (utl-dev.css) read the theme first; field, border, text, button and accent follow Lounge Light and Dark (D-814)." Closes it. Wheel face left open (WheelTool.tsx).
- Changelog: The hash, notes, timer, dice and developer tools in the drawer now match the theme you chose instead of drawing dark fields in a light drawer.
- `docs/10-design-system-and-theming.md`: "A part that is not a room (a drawer, a dock, a panel) reads `var(--ui-x, var(--sr-x, literal))`; a room maps its own tokens once on its root."
- Owner questions: none. Stashes: none touched. Scratch DB `sr_p79_tools_ui` dropped, `p79-tools-bin` deleted. The lock was released a moment before I stopped the stack by PID (stopped right after).
