# Wave 84 handoff: kit (SEC-AUD-351, 352, 353; D-856, D-857)

T = 13:20 on 2026-11-16. No new item 15:05, checkpoint 15:25, finished 15:50, hard limit 16:05. Branch `fix/p84-kit`, CSS only (`packages/ui/src/primitives/**/*.module.css`), no kit `.tsx`, no token, no export, no Rust, no cargo. Everything is done; nothing is running.

Lock: round 1 14:12 to 14:41 (base runs, build, after runs, a rule fixed, after runs again); round 2 14:45 to about 14:51 (the wide shots and item 2 again: the first after run of these had been raced by a stray run of my own spec, see "Faults of my own").

## Done (commits)
- `80f06319` the rules of items 1 to 3; `3eafac08` the searchable select; `5f06fd4e` hit areas counted from the padding box, swatch rows, the pill's padding; `bffd822d` wrapped pill rows 12 px apart.
- New spec `e2e/tests/p84-kit-shots.spec.ts` (parts `hit`, `contrast`, `seg`, `wide`; `SR_P84_KIT=1`, `SR_P84_BASE=1`, `SR_P84_KIT_OUT`, `SR_P84_PART`, `SR_P84_ONLY`).

## The rules I ended with, per sheet
Every rule of item 1 is inside `@media (max-width: 720px)`. Item 2 is for every width.
- `Modal.module.css`: `.close` 44 x 44 with `margin-block: -6px; margin-inline-end: -6px` (the header is as tall as before, the title still wraps before the button).
- `Controls.module.css`: **D-857** `.choice[data-disabled] .input { opacity: 0.55 }` replaces the row's `opacity: 0.55` (cursor rule kept). Phone: `.choice` min-height 44; `.text` (the label) `flex: 1 1 auto; padding-block: 12px` (the whole row is the label); `.input { position: relative; margin-top: 13px }`, `.switch { margin-top: 11px }` (control and first line stay centred together: both centres 22 px down, offset 0 as on the base); `.input::before` (switch too) a 44 px tall hit area (`inset-block: -14px`, `-13px` for the switch, counted from the padding box inside the 1.5 px border).
- `Input.module.css`: `.control { min-height: 44px }`, password `.toggle` `top/bottom: 0`.
- `Select.module.css`: `.control { min-height: 44px }`; `.control > .trigger`, `.control > .chevron` `margin-block: -1px` (the 38 px trigger in the 40 px box becomes 44); searchable field: `.inputRow { padding-block: 0 }`, `.inputRow .input { min-height: 44px; margin-block: -1px }`.
- `Tabs.module.css`: `.tab { min-height: 44px }`.
- `Button.module.css`: `.sm::after, .md::after, .link::after`: `position: absolute; inset-inline: 0; top: 50%; height: 44px; transform: translateY(-50%)` (icon-only included; the drawn box is unchanged). The spinner and label are not touched.
- `Slider.module.css`: range `.input { height: 44px; margin-block: -12px }` (slider as tall as before); `.segment::before` the same 44 px hit area on the pill's label; `.segmented { row-gap: 12px }` (wrapped rows of pills would otherwise overlap their targets). **Item 3 (every width):** `.segmented { flex-wrap: wrap }`, `.segment { padding: 6px 14px; text-align: center }` with `min-height` lowered 32 -> 20 (md) and 24 -> 18 (sm, `padding: 3px 10px`) because the box is the content box (a pill is 32 / 24 px tall as before).
- `ColorPicker.module.css`: `.swatch::before` 44 px hit area; `.swatches { row-gap: 16px }` (two rows of 44 px targets cannot sit in an 8 px gap); `.hex` and `.dropper` min-height 44.
- `Permissions.module.css`: `.triBtn::before` 44 px hit area; `.tri { overflow: visible }` (it clipped the pseudo-element) with the two end buttons rounding their own outer corners; `.search` min-height 44.
- `Settings.module.css`: `.navToggle { position: relative }` and `.navToggle::before { inset-block: -4px }` (38 px box, 44 px target).

Rules taken out again: none. Rules changed after the first run: the checkbox, radio, switch and nav toggle insets (3 to 4 px short, because an absolutely positioned pseudo-element is measured from the padding box, inside the border); the pill's padding (the first version made every pill 44 px tall: content-box `min-height`); swatch row gap and pill row gap (added: a neighbour's target covered the next row's).

## Base figures (before the CSS)
- Member's Settings at 375, `p83-settings-shots.spec.ts`: 277 controls under 44 px in Lounge Light, 277 in Lounge Dark (as wave 83). Server settings (`p83-guild-shots.spec.ts`, all tabs, roles, channels, three Admin pages): 837 (Lounge Light) and 935 (Lounge Dark) rows under 44 (they include Admin and the long role). The four dialogs' close at 375: 32 x 32.
- My hit test (375, Lounge Dark, 49 tabs: member's Settings, a server's settings, a role, a text channel): 1190 controls, **975 fail, 215 pass**.
- Streamer Mode's locked rows: label 3.14 (Light), 4.21 (Dark), 5.06 (Editorial); description 2.32, 2.84, 3.23; row opacity 0.55. Appearance's "Show avatars in Compact mode" the same figures.

## Hit-test table (failing controls, base -> after, per kind; 49 tabs, Lounge Dark, 375)
| kind | base fail | after fail | what is left and why |
|---|---|---|---|
| kit Button (sm, md, icon, link) | 253 (+4 link) | 0 (kit-button) / 2 link | 2 `link` Buttons ("Edit username" rows) are 15 px tall and 14 px apart: two 44 px targets cannot both hold (a neighbour's target in the way) |
| switch | 181 | 0 | |
| checkbox | 12 | 3 | native table checkboxes of Members (13 px, not the kit's `Checkbox`) |
| radio | 19 | 0 | |
| segmented pill | 60 | 3 | the preview (`pp-root`) or the next switch sits directly under the control and paints over the 6 px overhang |
| tab | 72 | 0 | |
| select trigger (kit) | 26 | 0 | |
| text field (kit Input) | 48 | 3 | app fields: `ss-select` toolbar fields and Support's bare `sup-custom` input (not the kit's) |
| swatch | 84 | 0 | |
| permission three-state | 84 | 0 | |
| Settings nav toggle | 49 | 0 | |
| range | 12 | 1 | two sliders stacked; the target of one is covered by the other's |
| native `select` | 16 | 16 (+15 `ss-select`) | app classes `ss-select`/`us-select` (guild's D-865, not the kit): 40 px |
| other buttons / links | 55 | 11 + 10 | app classes `ss-name ss-link`, `ss-role-chip`, `us-link`, links in `ss-card`, one kit FileDrop button (`_drop`, 36 px; **not covered, listed**) |

After total: 1194 controls, **44 fail, 1150 pass**. Every kit kind passes except the cases named in the third column; what remains is app-side (the guild builder's `.ss-select`, `ss-name`/`ss-link`, `us-link`, Support's bare input) or a neighbour's box in the way. The Settings shell's own close (round X) passes. Not covered: the kit `FileDrop` button and a multi-select chip's remove button.

Coverage note on the coordinator's relay (support builder): the `Select` trigger and caret (38 px in a 40 px box) and the kit Button and field on the in-app template page are covered by the rules above (trigger 44, md Button target, Input 44); I did not shoot the template page (not in my walk). `entrance.css` untouched.

## The two wave 83 specs again (after folders `after-settings`, `after-guild`)
- **1280, Editorial, member's Settings (20 tabs): 19 tabs 0 px moved. Profiles moved**: the "Preset" and "View my page as" segmented controls did not fit one line (their labels wrapped to two lines on the base); with item 3 their pills now wrap onto a second row (Preset 322 x 40 -> 314 x 72, the card 47 px taller). Item 3 allows exactly this; no control that fitted one line moved.
- **1280, server settings (19 tabs, roles, channels): 0 px moved on every tab**, Admin's Accounts, Limits & settings, audit log (Lounge Dark, 1280) 0 px.
- 375, boxes that moved, by cause: **a choice row** (single-line row 20 -> 44 px, +24 per row: Notifications +72, Streamer Mode +96, Advanced +24, Privacy and Activity Privacy +120 and +97 with their switches); **a field** (+4 per field: server Overview, Members toolbar, Admin Limits and the audit log toolbars, Invites, Language); **a tab** (+9: Emoji & stickers, Integrations, Community panel headers, role and channel tab strips); **a wrapped segmented control** (Profiles, Privacy & Safety, Activity Privacy, Accessibility: one row -> two, e.g. Spam filter 40 -> 82); **a slider** 0 (negative margins keep it). Everything else follows from those (cards and what is under them move by the sum). No sideways scroll anywhere (document and scrollers 0 after, as before). The checks of "nothing else moved" were by box log: the only boxes that moved are those and their ancestors/followers.
- The Settings shell's header at 375: unchanged (the toggle's target is a pseudo-element).

## The four dialogs (`after-dialogs`, 375)
Close control 44 x 44 on create channel, create server, invite people and the tools drawer (base 32 x 32), inside the dialog (e.g. 301,31 in 15..360), every title wraps before it (shots opened). At 1280 the close is 32 x 32 as before.

## Item 2 (D-857): after
The row is not dimmed; the input is dimmed to 0.55 (reads as off, does not vanish); `cursor: not-allowed` stays; boxes 0 px.
| theme | label before -> after | description before -> after |
|---|---|---|
| Lounge Light | 3.14 -> 11.39 | 2.32 -> 5.82 |
| Lounge Dark | 4.21 -> 10.21 | 2.84 -> 5.90 |
| Editorial | 5.06 -> 13.89 | 3.23 -> 7.62 |
Streamer Mode's three locked rows and Appearance's "Show avatars in Compact mode"; the row, input and label boxes are identical to the base (0 px). The walk met no disabled checkbox or radio (all disabled rows are switches); `sideroom` not measured.

## Item 3 (SEC-AUD-353): done
Chosen: **wrap the pills onto a second row** (plus 6 px of padding above and below a label and `text-align: center`, for a pill that is alone on its row and still too wide). Privacy & Safety, Profiles and Activity Privacy at 375 and 1280: before, 9 pills had a two- or three-line label with 0 px between the text and the pill's top and bottom edge ("People who are not friends", "Members only", "Single page", "Public web", "Shares a server"); after, every pill's label is one line (14 px to the sides, pill 32 px). At 1280 a control that fitted one line is unchanged (0 px): Privacy & Safety, Activity Privacy and the other tabs' controls show the same box; the two Profiles controls that wrapped are the ones that changed.

## The build
`npx pnpm@10.12.1 --filter @sideroom/web build`, run twice (once with the first CSS, once with the last): first paint `app/app-xAV8vqA3.js` 218 315 bytes gzip, room 37 685 (first CSS); `app-D3a3mwmq.js` **218 321 bytes gzip, room 37 679** (final). Wave 83: 218 309 / 37 691: +12 bytes (hash, the CSS is in the CSS file). The script prints no first-paint CSS size. `check-bundle-budget`, `check-entrance-isolation` ("every public surface is clean") and `check-staff-delivery` all passed on both builds. `apps/web/dist` was ignored; deleted.

## The wide shot check
Both runs 57 PNGs each (`visual-sweep-shots/p84-kit/base`, `.../after`; also `after-dialogs`, `base-dialogs`, and the wave 83 specs' own shots): signed out (login, join, invite) at 375 and 1280; Lounge Dark and Lounge Light at 375, Lounge Dark at 1280: My Account, Notifications, Voice & Video, Streamer Mode, server Overview, a role's Permissions, Members, create channel, invite people, the Lounge with composer (toolbar hovered at 1280), Admin Limits & settings; the Library, Cinema and Arcade front pages (shipped themes set explicitly and read back: `data-theme` 39 of 39 rows match). **I did not open all 114 and did not open every 375 shot of the clean run: I opened 22 shots: 17 from the raced first after run (their theme labels were wrong, the drawing is what the final CSS draws, except the Slider changes made after it) and 5 from the clean second run (Members, Lounge, Admin Limits, a role, Streamer Mode at 1280) and compared the rest by box log and byte equality (20 base/after pairs are byte-identical, mostly 1280).** Signed out at 1280: 0 px moved on all three pages; at 375 only the sign-in page changed (+8 px: its two fields 40 -> 44) and the join and invite pages 0 px. Library, Cinema, Arcade: 0 px moved at 375 and 1280. 1280 pages in Lounge Dark: My Account, Notifications, Voice & Video, Streamer, Overview, Admin Limits 0 px. The 1280 Members, role Permissions and Lounge shots of the second round came from a freshly seeded world (different members, a shorter role list, other times), so their box diff against the base is not a CSS comparison; the first after run (same world as the base) showed them at 0 px except message times.

## Faults of my own (for the record)
- In round 1 I killed a run of my spec by its bash parent; its node child kept running and raced my restarted run on the Owner's theme (PATCH), so that run's wide shots had wrong themes. I made them again in round 2 alone (shots of the raced run are in `visual-sweep-shots/p84-kit/raced`, do not use them) and checked `data-theme` in every row.
- The `hit.jsonl` of the final after run (49 tabs) is a clean run alone; `after/wide-raced.jsonl` and `contrast-raced.jsonl` are the raced ones.

## Tests and checks
Single vitest files `forms.test.tsx` (18), `overlays.test.tsx` (17), `lounge-kit.test.tsx` (11), `Tabs/Tabs.test.tsx` (2): all pass, none changed (they assert no class or style I changed; jsdom applies no CSS, so they could not fail without the change). prettier clean on my files, `i18n-audit --max 0` TOTAL 0, `check-doc-links` ok, e2e `tsc` clean, my spec run with `--project=chromium --reporter=line` (base and after). Docs: no sentence in `docs/10-design-system-and-theming.md` or `docs/dev` states a kit control's size or a disabled row's opacity (`git grep` found only unrelated layout constants), so no doc is made wrong; `docs/10` should gain the D-856 rule (commander).

## Proposed inventory rows
- SEC-AUD-351: every kit control has a 44 px touch target on a phone (Modal close, choice rows, fields, tabs, Button sm/md/link by hit area, segmented pill, swatch, three-state button, Settings nav toggle). Closed for the kit; what is left in the table above is app-side or a neighbour in the way (listed).
- SEC-AUD-352: a disabled choice row dims its control, not its text (label 10.2 to 13.9:1, description 5.8 to 7.6:1 in the three Lounge themes). Closed.
- SEC-AUD-353: a segmented control's pills wrap. Closed.
- SEC-AUD-348 closes (351 and the lounge builder's 355 landing); SEC-AUD-349 closes for rows 12, 15, 19, 21; SEC-AUD-350 closes for the kit's part of it (the kit's 44 px target on a server's settings: the `ss-select` and `ss-name` leftovers belong to guild).

## One changelog sentence
On a phone, switches, checkboxes, buttons, fields and tabs are easier to hit and a dialog's close button is bigger, a locked setting's text is readable, and a segmented control's options wrap instead of crowding the edge.

## Owner questions
1. The kit's phone rows are 24 px taller per single-line switch or checkbox row (44 px target): Settings tabs with many switches grow by 70 to 120 px. Keep, or add a looser rule for stacked switch lists (D-856 allows only the 44 px)? Default: kept.
2. A native table checkbox (Members) and the kit `FileDrop` button and multi-select chip remove stay under 44: a wave 85 item (a kit `.tsx` for the chip, the guild builder's table).
3. Default chosen: swatch rows 16 px apart and wrapped pill rows 12 px apart on a phone (only way two rows of 44 px targets do not overlap).

## Cleanup
Scratch database `sr_p84_kit_ui` dropped, `p84-kit-bin` deleted (`p80-base-bin` untouched), stack stopped by PID, lock released, `apps/web/dist` deleted. No stash touched.
