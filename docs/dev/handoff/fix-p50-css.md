# Handoff: wave 50 css fixes (XC-VIS-004)

Branch `fix/p50-css`. Nothing pushed. No Rust, no new strings.

## Done
1. **Select inside a dialog** (`packages/ui/src/primitives/Select/Select.module.css`): `.popup` z-index 960 to 1050
   (above Modal and Drawer backdrops at 1000, below the toast region at 1100). Menu (950) is untouched: it was not
   reported and is not used in dialogs here. A Select outside a dialog is still above the player dock (910) and tools
   (909/908) as before. Test: `packages/ui/src/primitives/layers.test.ts` (reads the stylesheets: popup > modal and
   drawer backdrop, < toast region). Click proof: Playwright, the "Report this message" dialog (same `ReportForm` as the
   guestbook report): before, the option click times out (covered by the backdrop); after, "Harassment, hate or
   threats" is picked, 6 of 6 (3 themes x desktop/phone).
2. **Settings do not scroll on phones** (`Settings.module.css`, the 720px block): `.main { position: static; min-height: 0; }`.
   The column child was growing to its content so `.content`'s `overflow-y: auto` never had a bounded height. Scroll
   top reached after a wheel: voice-channel tab 0 to 359 px (Save listening room visible), Appearance 0 to 2213 px, in
   all three themes; desktop unchanged (1615 on Appearance, same before and after).
3. **Party strip and chat card** (`watchparty.css`, `.wp-strip` and `.wp-card` only): background
   `var(--ui-bg-raised, var(--sr-bg-1, #14120f))` and `color: var(--ui-text, var(--sr-text, inherit))`. (`--sr-bg-1` is in
   fact defined in `tokens.css` for every theme, so Lounge never fell back; it simply used the Editorial near-black.)
   Lounge Light now 242/243/245 with dark text, Lounge Dark 43/45/49 with light text. Editorial moves from
   rgb(22,22,19) to rgb(28,27,23) (its own raised surface), text unchanged. `.wp-panel`/`.wp-controls` (the watch
   room itself, also `--sr-bg-1`) left alone.

Commits: see `git log` (subjects start `XC-VIS-004`).

## Visual check
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-css/{before,after}/`: `party-*`, `select-open-*`,
`select-picked-*`, `settings-room-*`, `settings-user-*`, each desktop and phone x lounge-editorial, lounge-light,
lounge-dark. All looked at in the after set (party light/editorial/phone dark, room settings phone, select open,
Appearance phone dark) plus the before select. Generator: `e2e/tests/p50-css-shots.spec.ts` (skips unless `SR_P50_CSS=1`;
`SR_P50_BEFORE=1` tolerates the broken click).

## Tests run
vitest `packages/ui` (17 files, 336) and `src/watchparty` (13 files, 153); web tsc; `check-css-tokens`, `i18n-audit`;
prettier on changed files; `pnpm --filter @sideroom/web build`, `check-entrance-isolation`, `check-bundle-budget`;
`playwright test --list` loads 912 tests. eslint was not runnable standalone in the ui/e2e packages (no binary); no
TS in web changed.

## Owner questions
- Menu (z-index 950) inside a Modal would have the same layering problem; raise it to 1050 too if any menu opens from a dialog.
