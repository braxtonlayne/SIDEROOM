# Wave 74 handoff: dots (SEC-AUD-294; D-779)

Branch `fix/p74-dots`. **No product file changed** (no `packages/ui`, no CSS, no string, no Rust, no route, no migration, no web build needed). One local-only spec and this handoff. One fault found (below), reported, not fixed.

## Done (commits)
- `SEC-AUD-294: status dot shots spec (local only)`: `e2e/tests/p74-dots-shots.spec.ts` (`SR_P74_DOTS=1`, out `SR_P74_DOTS_OUT`, default `visual-sweep-shots/p74-dots`; `SR_P74_DOTS_THEMES`, `SR_P74_DOTS_ONLY=header` and `SR_P74_DOTS_PEOPLE` only exist for reruns).
- This handoff.
- prettier clean; tsc shows no error for the spec; the repo has no eslint config for `e2e/` (there is no `eslint` binary or config reachable from `e2e`), so none was run.

## Confirmed by reading: where the dot is drawn
`packages/ui/src/primitives/Avatar/Avatar.tsx:51` draws the dot at `max(8, size * 0.28)` px, or `max(12, size * 0.4)` for the phone glyph; `Avatar.module.css:30-43` puts it 2px outside the bottom-inline-end corner with a 2px `var(--ui-bg)` border (the "ring"). The `size` prop is 24, 32, 40, 56 or 80.

| avatar | dot (px) / glyph | who draws a dot | where |
|---|---|---|---|
| **24** | 8 (core 4 inside the ring) / 12 | `ChannelHeader.tsx:125,133` | the header of a 1:1 DM, desktop and phone. **The smallest.** |
| 32 | 8.96 / 12.8 | `MemberList.tsx:201`, `HomeSidebar.tsx:153` (DM list), `StatusMenu.tsx:97` (user panel), `DmPanels.tsx:270`, `FriendsPage.tsx:58`, `CreateDmDialog.tsx:187`, `ActiveNow.tsx:36` | member list, DM list, user panel, DM side panel member row, Friends row, Create DM dialog, Active Now |
| 40, 56 | would be 11.2 and 15.7 | nobody (every `<Avatar size={40/56}>` I found passes no status; `ProfileAvatar`'s `size?: 56 \| 80` is never called with 56) | none today |
| **80** | 22.4 / 32 | `ProfileAvatar` (`profile/parts.tsx:33`) | profile popout (`ProfilePopout.tsx:166`), full profile (`FullProfile.tsx:305`), DM side panel (`DmPanels.tsx:43`) |

Inside the popout the avatar box measures 92 px, not 80 (`.lng-profile-avatar`, `lounge.css:1399`), so the dot sits 4 px inside the box corner there (measured `out(-4,-4)`). It is still whole.

Wave 73's spec (read, not its PNGs) shot the 32 px avatar in the member list (desktop, phone), the hovered member row, the DM list (desktop, phone), the hovered DM row, the user panel (one status), and the 80 px profile card for the **phone glyph only**. It never shot 24 px, nor 80 px with the four statuses. This wave covers 24 px and 80 px (four statuses and the glyph each, plain and, for 24 px, hovered), plus decorations. The other 32 px surfaces (Friends, Create DM, Active Now, DM side panel) use the same component and size as the member list and were not shot again.

## Decorations: they exist in code, not in the product
`git grep -n -i decoration -- apps/web/src packages crates e2e | grep -v text-decoration`: the client draws `user_profile.avatar_decoration` (a slug) as an image `/cdn/avatar-decorations/<slug>.png` (`lounge/profile/decoration.tsx`), inside `Avatar`'s `decoration` slot (`inset: -12%`, DOM order before the dot, so **the dot is drawn above it**). It is drawn **only by `ProfileAvatar`** (80 px popout, full profile, DM side panel). The member list and the DM list pass no `decoration`, so a decorated avatar never appears there (the task's "member list, DM list" decorated shots do not exist as states). Settings has a control (`settings/user/sections/Profile.tsx:153`). But `crates/sr-chat/src/profile.rs` ships **no decoration** (`decorations are empty until artwork is supplied`; D-290/OR-4: closed, effects only), so the server refuses every slug (`avatar_decoration: "not in the profile catalogue"`) and no account can get one.
So the decorated shots are **faked**: `page.route` adds `user_profile.avatar_decoration = 'standin-ring'` to every `GET /users/{id}/profile`, and `/cdn/avatar-decorations/*` is answered with an inline SVG: (a) "ring" is the repository's own story decoration (`packages/ui/src/stories/Misc.stories.tsx:59`, a gold ring); (b) "wedge" is the same ring plus a solid violet wedge over the bottom-right corner, **my own worst case** (not a repository asset), to see whether a decoration can cover the dot. Nothing was downloaded.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p74-dots`: **276 PNGs** (plus 24 contact sheets `_*.png` I made to look at them, `measure.json`, `measure-member32.json`). Real stack (sr-server and sr-cli from `p72-base-bin`, Vite 5523, DB `sr_p74_dots_ui`), the owner plus five accounts (online, idle, do-not-disturb, offline, phone) as in wave 73; Editorial, Lounge Light, Lounge Dark (account theme set); desktop 1280x800 and phone 375x812.
- `header24-<status>-<desktop|phone>-<theme>` (30), `header24-hover-...` (15, desktop), `zoom-header24-...` (30, close-up at scale 3 with room around the avatar), `zoom-header24-hover-...` (15).
- `profile80-...` (30, whole page) and `zoom-profile80-...` (30).
- `deco-profile80-<ring|wedge>-...` (60) and `zoom-deco-profile80-...` (60).
- `zoom-member32-<plain|hover>-offline-<theme>` (6): a one-off probe (not in the spec, deleted) to put numbers on the hover ring at 32 px (the account was offline then, so the dot is the grey one and the row is dimmed).
- **I opened all of them**, through contact sheets (each sheet holds 10 to 20 shots at 200 to 600 px; the 1x header strips at 600 px, the zooms at 240 px, the whole-page shots at 220 to 450 px) and, for every theme, the plain, hovered, phone, decorated (ring and wedge) and glyph shots. Colours and edges are read from the zooms, the numbers from `measure.json`.
- Phone: the DM header is the only phone surface with a 24 px dot; the 80 px popout on a phone is opened from that header button. The member list on a phone (32 px) is wave 73's.

## What I saw (per size, theme)
Whole = the dot is not cut by the avatar mask, the decoration or the box (`measure.json`: `clip.inside` true everywhere; the dot is 2 px outside the 24 px avatar corner and 4 px inside the 80 px box). Contrast is the dot against the surface the avatar sits on (computed backgrounds), unhovered unless noted.

| size, theme | whole | reads as its colour | ring colour = surface under it | decoration |
|---|---|---|---|---|
| 24 Editorial | yes | green 6.9, amber 7.1, red 3.9, grey 3.0 (hovered 6.2, 6.4, 3.6, **2.7**). 4 px core: each still reads in the 1x strip, idle vs offline told apart | plain: identical. **Hovered header button: no** (ring `#23221d` against the hover pill `#2c2a24`, a dark disc, dRGB 9) | n/a |
| 24 Lounge Light | yes | 4.4, 3.8, 4.2, 3.7 (hovered 3.7, 3.2, 3.5, 3.1) | plain: yes. **Hovered: no, a white disc on the grey pill (ring `#fff`, pill `#e8eaed`, dRGB 23): conspicuous in `zoom-header24-hover-*-lounge-light`** | n/a |
| 24 Lounge Dark | yes | 4.0, 6.7, 3.4, 3.4 (hovered 3.7, 6.3, 3.2, 3.2) | plain yes; hovered a faint dark disc (dRGB 4) | n/a |
| 80 Editorial | yes | 7.4, 7.7, 4.3, 3.3 | ring `#23221d` on the popout card `#1c1b17`: a slightly lighter ring (dRGB 7), faint | ring and wedge: dot above the decoration, whole, not shifted; the wedge sits under it |
| 80 Lounge Light | yes | 4.4, 3.8, 4.2, 3.7 | yes (dRGB 0) | same |
| 80 Lounge Dark | yes | 4.3, 7.3, 3.7, 3.7 | faint (dRGB 7) | same |
| phone glyph 24 / 80, three themes | yes, a 12 px (24) and 32 px (80) green phone glyph on a page-colour disc | green in every theme | same disc rule as the dots | same |
| phone layout (header 24, popout 80) | same numbers as desktop | same | same | same |
| 32 (probe, offline row) | yes | not the point | **hovered row: ring `#fff` on `#e8eaed` (Light, dRGB 23), `#23221d` on `#2c2a24` (Editorial, 9), `#313338` on `#35373c` (Dark, 4)**; the plain row is also off in Light (white disc over the `#f2f3f5` panel) | n/a |

The 4 px core of the 24 px dot (8 px box, 2 px ring) is small but each status reads in all three themes, and the phone glyph is large for a 24 px avatar (half its width) but whole.

## Faults
1. **The dot's ring is `--ui-bg`, not the colour of the surface under it, so on a hovered row it is a visible disc** (worst in Lounge Light: white on the grey hover). `packages/ui/src/primitives/Avatar/Avatar.module.css:36` (`border: 2px solid var(--ui-bg)`) and `:43` (`.statusIcon` background). Seen at 24 px on the hovered DM header button (`zoom-header24-hover-*-desktop-lounge-light.png` is the clearest; also `-editorial`, `-dark`) and measured at 32 px on the hovered member row (`zoom-member32-hover-offline-lounge-light.png`; wave 73 looked at hovered rows at 32 px and called them fine; the same CSS means the DM row hover is the same, not measured). It does not cut the dot or hide a status. Smallest fix: give the ring a variable, `border: 2px solid var(--ui-status-ring, var(--ui-bg));` (and the same for `.statusIcon`'s background), and set `--ui-status-ring: var(--ui-bg-hover)` on `.lng-member:hover`, `.lng-dm:hover`, `.lng-dm-identity:hover` and the header identity button's hover (`lounge.css:671`, `:1359`, `profile-p11.css:271`, the header's own rule). Severity: low (cosmetic halo).
2. **Observation, not a fault by any set rule:** the offline dot on Editorial's hovered header pill is 2.73:1 (`#6f6c5f`-ish grey on `#2c2a24`); wave 73's 3:1 test covers online, idle and do-not-disturb only (offline stays muted by design). If the owner wants offline at 3:1 too, that is a value change next wave.
3. **Observation:** the smallest dot's coloured core is 4 px (`Avatar.tsx:51`, `max(8, ...)` with a 2 px ring). Readable in the shots; a `max(10, ...)` would give a 6 px core if the owner wants it easier.

No dot is cut, no decoration covers or moves a dot (the dot is later in the DOM than the decoration, and the wedge worst case shows the dot on top of it), no status reads as the wrong colour.

## Tests
No product test changed. The spec is a local shot script; it passed (full run, then the Dark theme rerun after an unrelated flake: the other accounts' sessions had dropped after about 12 minutes, so the rerun was Lounge Dark alone; then a header-only rerun for the hovered close-ups, with accounts reused because `/sr/register` was rate limited by address after about 20 registrations). I did not run anything against the base: there is no fix to fail without.

## Docs
Nothing to change. `docs/open-question-answers.md` (OR-4) and `docs/05-api-and-gateway.md` already say the catalogue holds no decorations. No doc states a dot size.

## Proposed inventory rows
- SEC-AUD-294 (close when looked at): "The status dot looked at on the smallest avatar that draws one (24 px, the DM header) and on the 80 px profile avatar, four statuses and the phone glyph, three themes, desktop and phone, plain and hovered; and on a decorated avatar (faked profile answer, stand-in image: the catalogue holds no decoration, D-290). All whole, all read as their colour, no decoration covers the dot (D-779)."
- New open row (fault 1): "The dot's ring is the page colour (`--ui-bg`), so on a hovered row or header button it is a visible disc (Lounge Light worst): give the ring a variable the hover rules set to `--ui-bg-hover` (`Avatar.module.css:36,43`)." Optional rows for observations 2 and 3.
- Threat-model rows: none.

## Changelog sentence
None for members (a cosmetic fault found, not fixed).

## Owner questions
1. The dot's ring is the page colour, so it shows as a disc on hovered rows and on the hovered DM header (clearest in Lounge Light). Fix next wave (small, two CSS lines plus the hover rules)? Default: yes.
2. Offline dot at 3:1 on a hovered Editorial surface (2.7 now)? Default: leave offline muted.
3. Make the 24 px dot a little larger (core 6 px)? Default: leave.

## Cleanup
Stack stopped by PID (ports 5523, 8523; none listening), `sr_p74_dots_ui` dropped, lock released, `p74-dots-bin` deleted. No cargo target was made. Untracked `.scratch/` in the worktree is never committed. No stash touched (none created).
