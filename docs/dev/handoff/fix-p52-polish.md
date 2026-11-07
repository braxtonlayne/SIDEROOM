# fix/p52-polish (wave 52)

## Done
- **UTL-AUD-001** (`26ba1bc4`): the audio trimmer's range sliders and checkbox take `accent-color: var(--ui-accent)`, the Fade in/out
  number fields use the kit's input look (`--ui-bg-raised`, `--ui-border`, `--ui-radius`, focus ring), all logical properties
  (`apps/web/src/tools/audio/audio-tool.css`). New `AudioTool` prop `hideCancel`; the soundboard upload dialog passes it so the
  dialog's own Cancel is the only one. The drawer tool and the voice recorder keep their Cancel. Vitest: 2 cases in `AudioTool.test.tsx`.
- **XC-VIS-004**: the phone overflow was the audience Select in the Profiles page's Advanced (privacy) panel (`.us-field` row:
  label + hint + Select pushed the control to x 353..385). `.us-fields .us-field` now stacks label above select
  (`apps/web/src/settings/user/user-settings.css`). Measured at 375 px: no element offscreen in any theme.

## Not done
Nothing outstanding. Lounge Light's accent is the brand blue (themed, not the browser default).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p52-polish/`)
`before-*` and `after-*` for Editorial, Lounge Light, Lounge Dark, desktop and phone: `-sound-dialog` (upload dialog with trimmer),
`-drawer-audio`, `-profiles`, `-privacy` (the Advanced panel). `report-before.txt` / `report-after.txt`: Cancel count in the dialog 2 -> 1;
offscreen "▾" 353..385 on phone -> none. Every shot was looked at (the `-profiles` full-page shot only shows the top of the
scroll container; `-privacy` is the relevant one).

## Tests run
web tsc, eslint + prettier on changed files, vitest `src/tools src/settings/user src/settings/server` (86 files, 770 tests), css-tokens, i18n-audit (0).

## Owner questions
None. (Default chosen: a dialog that hosts the trimmer owns the only Cancel; there is no separate way to discard the trim except
choosing another file or closing the dialog.)
