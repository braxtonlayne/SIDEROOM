# Wave 78, files builder: SEC-AUD-307 (D-804)

Status: done. Rule committed, build and the three checks green, shots taken and all opened, stack stopped, DB dropped, binaries deleted, lock released.

## What the rule is

One block at the end of `apps/web/src/styles/tokens.css` (after the `a` rule): `input[type='file']` (font and colour inherit, `max-inline-size: 100%`) and `input[type='file']::file-selector-button` drawn as the kit's secondary button (`Button.module.css`: `.secondary` + `.button`). Values copied, nothing imported:

| part | value | kit source |
|---|---|---|
| background | `var(--ui-bg-hover, var(--sr-bg-2))` | `.secondary { background: var(--ui-bg-hover) }` |
| border | `1px solid var(--ui-border, var(--sr-line))` | `.secondary { border-color: var(--ui-border) }` |
| colour | `var(--ui-text, var(--sr-text))` | `.secondary` |
| radius | `var(--ui-radius, var(--sr-radius))` | `.button` |
| font | `inherit`, weight 500 | `.button` |
| padding | 8px block, 14px inline | `.md` (36px high, 16px inline) |
| hover | border colour `var(--ui-text-muted, var(--sr-text-2))` | `.secondary:hover` |
| disabled | opacity .5, `cursor: not-allowed` | `.button:disabled` |
| focus | `outline: 2px solid var(--ui-focus, currentColor)`, 2px offset, on the input | `.button:focus-visible` |

Deviation from the plan's wording: the plan says the theme's raised background (`--ui-bg-raised`) and a hover background. The kit's real secondary button uses `--ui-bg-hover` and changes the border on hover, so the rule follows the kit. Logical properties only; no class, section or theme name. No `.tsx` change. No existing styles test needed a change.

## The inputs (`git grep -n 'type="file"' -- apps/web/src`, tests left out)

41 grep lines; one (`review/folderDrop.ts:72`) is a doc comment. **40 inputs: 25 hidden, 15 visible** (the Rings composer's is the kit `TextInput type="file"`). R = reached by the spec on a fresh stack; N = needs data the stack lacks.

| file:line | hidden how / visible | page | shot |
|---|---|---|---|
| admin/cinema/ArtworkPicker.tsx:94 | visible (`adm-file`, no CSS for the class) | Admin > Cinema title artwork | N (needs a title) |
| admin/cinema/SubtitlesPanel.tsx:315 | visible (`adm-file`) | Admin > Cinema subtitles | N (needs a title) |
| admin/cinema/tv/ChannelEditor.tsx:191 | visible (`adm-file`) | Admin > TV channel logo | N (needs a channel) |
| admin/cinema/tv/GuidePanel.tsx:414 | visible (inside `label.ss-field`) | Admin > TV guide import | N (needs a channel) |
| admin/library/PagesDialog.tsx:110, :256 | hidden (`hidden`) | Admin > Library pages | no |
| admin/sections/EntranceHero.tsx:90 | visible | Admin > Entrance | R |
| bugreport/BugReportDialog.tsx:294 | hidden (`.br-file { display: none }`) | bug report | no |
| cinema/submit/DraftPage.tsx:534 | hidden | Cinema submit | no |
| cinema/submit/VideoPicker.tsx:133, :145 | hidden | Cinema picker | no |
| developers/Packages.tsx:93, tabs/Activities.tsx:297, tabs/General.tsx:119 | hidden | Developers | no |
| developers/tabs/Media.tsx:117, :274 | visible (in a `label`) | Developers > Media | N (needs an application; the spec's try timed out) |
| library/Submissions.tsx:973 | visible (`label.rv-file`) | Library submission, a chapter | R |
| lounge/AddServerDialog.tsx:255, Composer.tsx:1012, events/EventCoverField.tsx:102, forum/NewPostDialog.tsx:177, shell/DmPanels.tsx:164, shell/GuildProfileDialog.tsx:83 | hidden | Lounge dialogs and composer | no |
| lounge/apps/CommandForm.tsx:318 | visible (`.lng-appform-file`: own rule `max-inline-size`, `font: inherit`; no button rule, so mine applies) | app command with a file option | N (needs an app) |
| lounge/rings/RingComposer.tsx:154 | visible (kit `TextInput type="file"`) | Rings composer | N (needs a ring) |
| profilepage/Inspector.tsx:352, StylePanel.tsx:192 | hidden | profile page editor | no |
| review/ArtworkField.tsx:52 | hidden | review | no |
| review/UploadDock.tsx:250 | visible (`webkitdirectory`, multiple) | Library submission, folder | R |
| settings/server/sections/ImageField.tsx:116 | hidden | server settings | no |
| settings/server/sections/SoundboardTab.tsx:382 | visible (`#ss-sound-file`, in the upload dialog) | Server settings > Expressions > Soundboard > Upload a sound | R |
| settings/user/sections/Profile.tsx:398, :410 | hidden (`.us-sr`, visually hidden) | My Account | no |
| storage/browser/UploadPanel.tsx:139, :150 | hidden | storage browser (drop zones) | no |
| tools/audio/AudioTool.tsx:231 | visible | Audio trimmer (drawer; also Soundboard, voice recorder) | R |
| tools/dev/HashTool.tsx:48 | visible | Hashes (drawer) | R |
| tools/notes/NotesTool.tsx:71 | visible | Private notes (drawer) | R |
| tools/studio/GifTool.tsx:125, ImageLoader.tsx:73 | hidden | studio tools | no |

`git grep "file-selector-button" -- apps packages`: only `packages/starter-packs/activities/tabletop/style.css:214` (the tabletop activity's own stylesheet, not loaded by the app). No global rule existed and no visible input of the app carried its own button rule. The hidden drop-zone inputs (storage browser, composer) are unchanged by construction.

## What I saw on the base (run: yes, block held back, same stack and spec with SR_P78_BASE=1)
Chromium answers for `getComputedStyle(input, '::file-selector-button')`. Base: Library inputs in the shipped theme bg rgb(107,107,107), white text, 2px white border, radius 0, font Arial, cursor default; in lounge-light bg rgb(240,240,240), black text, 2px black border, radius 0, Arial. The same on hash, notes, audio and Admin Entrance (tools: Arial against the page's Hanken Grotesk). Never the theme's. Base shots: `visual-sweep-shots/p78-files/base` (20; I opened one, the phone chapter shot; the probe values are the proof).

## After (computed values equal a probe element carrying the kit's tokens; asserted for all 18 probed inputs; the spec passes)
Shipped Library: bg rgb(35,37,29), text rgb(236,230,214), border 1px rgb(42,43,36), radius 2px, Hanken Grotesk, pointer. Lounge Light: bg rgb(232,234,237), text rgb(49,51,56), border rgb(201,204,209), radius 8px, the page's font. Lounge Dark: bg rgb(53,55,60), text rgb(219,222,225), border rgb(63,65,71), radius 8px. Hover (light): border rgb(92,94,102). Keyboard focus: solid 2px blue ring (shot).

## Build and checks (inside the lock)
`pnpm --filter @sideroom/web build` ok (63 s). `check-bundle-budget`: **first paint app-CeO6e4Xd.js 215943 bytes gzip = 210.9 KB, room 40057 bytes to the 250 KB limit (needs 8 KB)**; all other budgets ok. `check-entrance-isolation`: clean; landing 115.8/120, auth 121.4/150, invite 113.5/120, profile 115.1/150, guest-play 115.5/120 KB gzip (same as wave 77). `check-staff-delivery`: ok. The CSS counts toward the gzip figures of surfaces that load `tokens.css` (about 0.3 KB raw; no figure moved at one decimal). `check-css-tokens` clean, `i18n-audit --max 0` = 0, prettier clean, `check-doc-links` ok, e2e `tsc` clean. eslint: the e2e package has no eslint binary, so the spec is not linted. `styles/__tests__/textLink.test.ts` and `components/__tests__/entranceDangerText.test.ts` read tokens.css (only `--sr-*` colours and the `a` rule); not run, CI runs them.

## Pages without a theme root
None of the visible inputs sits on one (all are inside the app, which has `--ui-*`); the `--sr-*` fallbacks apply only where no theme root exists. Not shot.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-files`, 23, all opened)
Library chapter and folder inputs, shipped and lounge-light (D-797; `data-theme` read back from `.lib-root`), desktop and phone: 8. Hash, notes, audio in the drawer, Lounge Dark and Light: 6. Soundboard upload dialog: 2. Admin Entrance: 2. Hover, focus ring, a file set on the chapter input, a file set on the hash tool (dark and light): 5. Clicked through: tab to the chapter input (ring), `setInputFiles` on it (the page takes it; a fake .cbz is refused with "This is not a ZIP, 7z or RAR archive."), a file set on the hash tool (hash appears). Disabled state not reached (the chapter input is disabled only while uploading); the rule is the kit's. In every shot the button reads as one of the app's buttons and the file name is readable.

## Findings outside my files
- **A phone Library submission page already overflows sideways on the base** (105 to 113 px): `.lib-sectionbar-nav` (right edge 483 to 543 at 375 wide) and `.rv-table` (review.css, fenced). With the rule the page is 14 px wider (the button is larger than the browser's). The input never exceeds its panel (input width = parent width). The spec logs the elements as `OVERFLOW` and asserts the sideways-scroll limit only at desktop width. For the library builder/commander.
- `tools/utl-dev.css` (hash and notes fields) draws dark boxes in a light drawer (reads `--sr-*`; the plan's "left for later"); the button is readable on them. The notes field shows "No...en" for "No file chosen" (narrow field, pre-existing).
- Admin Entrance: the "Choose an image" label text touches the button (inline label layout, not mine).
- The light Library's left bar is still dark (library builder's item).

## Proposed
- Inventory row **SEC-AUD-307** (closed): every visible native file input draws its picker button as the kit's secondary button, one rule in `apps/web/src/styles/tokens.css` (D-804); 15 visible of 40 inputs.
- Threat-model rows: none. Fragments: none. Docs changed: none (no doc names this).
- Changelog sentence: "Choose file" buttons now look like the app's other buttons, in every theme.
- Owner questions: (1) I used the kit's `--ui-bg-hover` background (as the secondary button does) rather than the plan's `--ui-bg-raised`: keep? (2) Admin Cinema/TV, Developers > Media, the app command form and the Rings composer inputs were not shot (need data): next plan if wanted.

## Cleanup
Stack stopped by PID (ports 5563, 8563, 9263 free), `sr_p78_files_ui` dropped, `p78-files-bin` deleted, lock released, test-results removed. No stash touched (I ran no stash command and did not look at the list).
