# Wave 78, files builder: SEC-AUD-307 (D-804)

Status: rule written and committed; the checks and shots below are filled in as they run (see "Where things stand").

## What the rule is

One block at the end of `apps/web/src/styles/tokens.css` (after the `a` rule): `input[type='file']` (font and colour inherit, `max-inline-size: 100%`) and `input[type='file']::file-selector-button` drawn as the kit's secondary button (`Button.module.css`: `.secondary` + `.button`). Values copied, nothing imported:

| part | value | kit source |
|---|---|---|
| background | `var(--ui-bg-hover, var(--sr-bg-2))` | `.secondary { background: var(--ui-bg-hover) }` |
| border | `1px solid var(--ui-border, var(--sr-line))` | `.secondary { border-color: var(--ui-border) }` |
| colour | `var(--ui-text, var(--sr-text))` | `.secondary { color: var(--ui-text) }` |
| radius | `var(--ui-radius, var(--sr-radius))` | `.button { border-radius: var(--ui-radius) }` |
| font | `inherit`, weight 500 | `.button` |
| padding | 8px block, 14px inline (the md button is 36px high with 16px inline padding; the native part has no min-height) | `.md` |
| hover | border colour `var(--ui-text-muted, var(--sr-text-2))` | `.secondary:hover` |
| disabled | opacity .5, `cursor: not-allowed` | `.button:disabled` |
| focus | `outline: 2px solid var(--ui-focus, currentColor)` + 2px offset on the input | `.button:focus-visible` |

Deviation from the plan's wording: the plan says "the theme's raised background" (`--ui-bg-raised`) and a hover background. The kit's real secondary button uses `--ui-bg-hover` as its background and changes the border on hover, so the rule follows the kit (the item says to copy the kit's values). Logical properties only; no class, section or theme name.

## The inputs (`git grep -n 'type="file"' -- apps/web/src`, tests left out)

41 grep lines; one (`review/folderDrop.ts:72`) is a doc comment. **40 inputs: 28 hidden, 12 visible** (plus the kit `TextInput type="file"` of the Rings composer, a 13th that is visible; it is not a literal `type="file"` line in the grep above but `type="file"` on a `TextInput`, so it is counted in the 40).

Reachability: R = a fresh stack reaches it (spec), N = needs data or a feature a fresh stack lacks.

| file:line | hidden how / visible | page | shot |
|---|---|---|---|
| admin/cinema/ArtworkPicker.tsx:94 | visible (`className="adm-file"`, no CSS for the class) | Admin > Cinema title artwork | N: needs a title |
| admin/cinema/SubtitlesPanel.tsx:315 | visible (`adm-file`) | Admin > Cinema title subtitles | N: needs a title |
| admin/cinema/tv/ChannelEditor.tsx:191 | visible (`adm-file`) | Admin > TV channel logo | N: needs a TV channel |
| admin/cinema/tv/GuidePanel.tsx:414 | visible (inside a `label.ss-field`) | Admin > TV guide import | N: needs a TV channel |
| admin/library/PagesDialog.tsx:110, :256 | hidden (`hidden`) | Admin > Library pages dialog | no |
| admin/sections/EntranceHero.tsx:90 | visible (`label.ss-field`) | Admin > Entrance | R (spec) |
| bugreport/BugReportDialog.tsx:294 | hidden (`.br-file { display: none }`) | bug report dialog | no |
| cinema/submit/DraftPage.tsx:534 | hidden (`hidden`) | Cinema submit draft | no |
| cinema/submit/VideoPicker.tsx:133, :145 | hidden (`hidden`) | Cinema video picker | no |
| developers/Packages.tsx:93 | hidden | Developers > packages | no |
| developers/tabs/Activities.tsx:297 | hidden | Developers > activities | no |
| developers/tabs/General.tsx:119 | hidden | Developers > general | no |
| developers/tabs/Media.tsx:117, :274 | visible (inside a `label`) | Developers > media (emoji, assets) | N: needs an application |
| library/Submissions.tsx:973 | visible (`label.rv-file`) | Library submission, a chapter's input | R (spec) |
| lounge/AddServerDialog.tsx:255 | hidden | Add a server dialog | no |
| lounge/Composer.tsx:1012 | hidden | composer (drop zone) | no |
| lounge/apps/CommandForm.tsx:318 | visible (`.lng-appform-file`, own rule: `max-inline-size`, `font: inherit`, no button rule) | an app command with a file option | N: needs an app command |
| lounge/events/EventCoverField.tsx:102 | hidden | event cover | no |
| lounge/forum/NewPostDialog.tsx:177 | hidden | forum new post | no |
| lounge/rings/RingComposer.tsx:154 | visible (kit `TextInput type="file"`) | Rings composer, photo and clip | N: needs a ring |
| lounge/shell/DmPanels.tsx:164 | hidden | DM panels | no |
| lounge/shell/GuildProfileDialog.tsx:83 | hidden | guild profile dialog | no |
| profilepage/Inspector.tsx:352 | hidden | profile page editor | no |
| profilepage/StylePanel.tsx:192 | hidden | profile page editor | no |
| review/ArtworkField.tsx:52 | hidden | review artwork field | no |
| review/UploadDock.tsx:250 | visible (`webkitdirectory`, multiple) | Library submission, the folder input | R (spec) |
| settings/server/sections/ImageField.tsx:116 | hidden | server settings images | no |
| settings/server/sections/SoundboardTab.tsx:382 | visible (`#ss-sound-file`, in the upload dialog) | Server settings > Soundboard > Upload a sound | R (spec) |
| settings/user/sections/Profile.tsx:398, :410 | hidden (`.us-sr` visually hidden) | My Account profile | no |
| storage/browser/UploadPanel.tsx:139, :150 | hidden | storage browser upload panel (drop zone) | no |
| tools/audio/AudioTool.tsx:231 | visible (`label.utl-aud-field`) | Audio trimmer (drawer; also Soundboard and the voice recorder) | R (spec) |
| tools/dev/HashTool.tsx:48 | visible | Hashes (drawer) | R (spec) |
| tools/notes/NotesTool.tsx:71 | visible | Private notes (drawer) | R (spec) |
| tools/studio/GifTool.tsx:125 | hidden | GIF tool | no |
| tools/studio/ImageLoader.tsx:73 | hidden | image loader | no |

`git grep "file-selector-button" -- apps packages`: only `packages/starter-packs/activities/tabletop/style.css:214`, the tabletop activity's own sandboxed stylesheet (not loaded by the app). No global rule existed. No visible input in the app carries its own `::file-selector-button` rule, so none is kept apart from that one.

## Where things stand

(filled in below as the run goes)
