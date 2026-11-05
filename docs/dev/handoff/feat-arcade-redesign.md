# feat/arcade-redesign: The Arcade redesign (ARC-UI-010)

Builder "arcade". Branch `feat/arcade-redesign` from main `d7cf7380`. Not pushed.

The owner said the Arcade looked broken: a bare column with a title, three chips, one plain Emulator card and an
always-open "List a cartridge" form. Round 1 turned it into a games front page. The owner then said it "still clashes
with the rest of the site: you click the arcade button and the entire design changes". Round 2 rebuilt it in the
Lounge's design language and applied the owner-delegated decisions A5 and A6 (D-474, D-475). Every API call and
behaviour stays the same.

## Done

| Commit | What |
|---|---|
| `53632bf5` | `GameArt.tsx`: in-house inline-SVG cover art (Land.io, Arena 3D, Hold'em, Whiteboard, Tabletop, Emulator, plus generic art coloured by a hue taken from the title). `builtins.ts`: the built-in games catalogue. `GameCard` gets art, a status, a meta line, a footer, `focusable` and a `feature` size. |
| `8d2e58e3` | Round 1 page: Playable now (the Emulator plus the five built-ins, drawn without waiting for data), "List a cartridge" in a `Modal`, server picker only with more than one server, designed empty, loading and error states with Try again. |
| `74582b8f` | The Showroom's sample Arcade uses the same sections and cards; `arcade-room.spec.ts` is scoped to the Featured region. |
| `b6ed3825` | Round 1 handoff. |
| round 2 (this commit) | **The Lounge's design language.** Chrome: the Arcade now shows the Lounge's own `SectionBar` (it takes a `current` room, so "The Arcade" is the current one) and the desktop `TitleStrip` (back/forward, title, the section-bar toggle, inbox, help). On a phone it keeps only the section bar, like the Lounge. The page header is a Lounge `lng-header` (icon, 16px bold title, jump links styled as the Friends tabs). The pitch and the two ways to start a game are a kit info `Notice`, like the Lounge's notice banners. Section headings are the Lounge's small uppercase muted labels. Surfaces, text and links use the Lounge tokens (`--bg-base-low/lower/lowest`, `--text-normal/muted`, `--interactive-active`, `--text-link`), so Lounge Light, Dark, Onyx, Ash and Editorial all follow the member's theme. The body font is used everywhere (no serif display face). Radii are 8px or the kit's `--ui-radius`. Cards look like the chat's attachment cards: the lower surface, a 1px border, no lift on hover. Status uses the kit `Pill` (success or warning) and the Game/Mod badge is the kit `Tag`; capability and tier chips match the kit tag. The cover art is framed inset with a 6px radius, a hairline and a 12% wash of the theme's deepest surface, like an image in a message. Empty states are the kit block with no box, as in the Lounge. **D-474:** the Featured section and its jump link are gone while there are no picks; there is no empty state. **D-475:** "Each one says who owns its code, art and sound." is the second sentence of `arcade.featured.intro`, and `arcade.page.intro` is deleted. Retired strings: `arcade.featured.{empty,emptyHint,none,noneOther,onlyEmulator,startWhere,startHint}`, `arcade.soon.empty`, `arcade.cartridges.{empty,inServer}`, `arcade.hero.{eyebrow,howHeading}`. Decisions-log rows D-470 to D-475 are added to the Phase 41 table. The sweep gains `SR_SWEEP_DARK=<theme id>` so the dark pass can be shot in Lounge Dark. |

Sketch & Slander: add one `BUILTIN_GAMES` entry (`status: 'preview'` puts it under Coming soon, `'live'` under
Playable now) and the `arcade.builtin.<id>.name` and `.pitch` strings. Its count is "2 to 16" (D-473). Until someone
paints it, an unknown `art` falls back to the generic painting.

## Side-by-side comparison (round 2, before changing anything)

Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/arcade-compare/` (lounge-home, lounge-channel,
settings-user-profile and the round-1 arcade). Differences found, and what round 2 did:

| Area | Lounge / Settings | Arcade (round 1) | Now |
|---|---|---|---|
| Chrome | 28px section bar (SIDE ROOM / rooms) + desktop title strip | none: the page started straight away | the Lounge's own SectionBar and TitleStrip |
| Page header | 48px `lng-header`: icon, 16px bold title, pill tabs, bottom border | a large gradient hero card | `lng-header` with icon, title and jump links |
| Background | `--bg-base-low` / `-lower` / `-lowest` | `--ui-bg` / `--ui-bg-raised` with accent gradients | Lounge tokens; no gradients |
| Fonts | the body sans everywhere; headings 12-20px | `--ui-font-display` (serif in Editorial) for h1, h2 and card titles, up to 52px | the body font; 12px section labels, 16px card titles |
| Section headers | small uppercase muted labels (DIRECT MESSAGES, ACTIVE NOW) | 24px display headings | the Lounge's labels plus a muted line |
| Accent | buttons, active pill and links only | eyebrows, borders, chip tints, the hero glow | buttons and links only |
| Cards | attachment card: lower surface, 1px border, ~8px radius | 16px radius, raised surface, hover lift and shadow | 8px, lower surface, 1px border, no lift |
| Chips and badges | kit Pill, Tag and Badge | bespoke pills | kit Pill and Tag; chips match the kit tag |
| Spacing | 8-16px, dense | 40px between sections, 32px hero padding | 16px padding, 28px between sections |
| Empty states | kit block, centred, no box | dashed boxes | kit block, no box; Featured hidden when empty |
| Media | images inset with a radius in a message | edge-to-edge art with an overlaid ribbon | art inset with a 6px radius and a hairline; status beside the title |

## Visual check (AGENTS.md section 4)

- Before round 1: `visual-sweep-shots/arcade-before/`
- After round 1: `visual-sweep-shots/arcade-after/` (and `arcade-after/flows/`: the populated page and the dialog)
- After round 2, Lounge Light and Editorial (the sweep's default dark): `visual-sweep-shots/arcade-lounge/`
  (arcade, showroom-arcade, admin-arcade, lounge-home)
- After round 2, Lounge Light and **Lounge Dark** (`SR_SWEEP_DARK=lounge-dark`): `visual-sweep-shots/arcade-lounge-dark/`
  (arcade, lounge-home, lounge-channel)
- Before/after pairs: `arcade-compare/arcade-desktop-light.png` → `arcade-lounge/arcade-desktop-light.png`;
  `arcade-compare/arcade-desktop-dark.png` → `arcade-lounge/arcade-desktop-dark.png`; `arcade-after/arcade-phone-light.png`
  → `arcade-lounge-dark/arcade-phone-light.png`. Compared side by side with `lounge-channel-desktop-dark.png` and
  `lounge-home-desktop-light.png` from the same folders: the same bars, header, surfaces, type and pills.
- Fixed while iterating in round 2: the Emulator card's text sat far from its button (now 2:3 columns, the button
  right under the text); on a phone the header title was cut to "The …" (the title no longer shrinks and the jump
  links scroll sideways); the notice icons sat off their first line.
- The sweep flags nothing on arcade, showroom-arcade and admin-arcade. lounge-home's `403 GET /api/v1/rings` is
  the Lounge's own and not touched here.

## Tests and gates (round 2)

- vitest `src/arcade src/showroom src/emu src/lounge/__tests__ src/i18n`: 833 passed in 101 files. The new
  `arcade/__tests__/arcadePage.test.tsx` renders `/games` in the app router: the rooms nav marks The Arcade as current;
  with no picks there is no Featured heading and no Featured jump link; with a pick both appear, with the provenance
  line. In `arcade.test.tsx` the empty-Featured test became "renders nothing" plus "appears with its intro and the
  provenance line" (D-474, D-475).
- Web tsc, e2e tsc, eslint and prettier on the changed files, `check-css-tokens` (69 clean), `i18n-audit` (TOTAL 0),
  web build, `check-entrance-isolation` (clean), `check-bundle-budget` (all ok; first paint `app` unchanged at 246.8 KB
  gzip), `playwright test --list` (902 tests in 66 files), `arcade-room.spec.ts` against my stack (passed).

## Not done

- No inventory row for ARC-UI-010: the integrator should add it to the Wave 47 section the sketch branch adds to
  `docs/inventory/21-feature-phase.md`, to avoid a merge conflict.
- ARC-UI-011 ("Start in…" on cards, D-472) is a later wave, as decided.
- D-475's wording still says the hero keeps "Games night, any night". Round 2 removed that hero to match the Lounge
  (owner feedback), so the string is deleted; the pitch survives as the notice title. The row is copied verbatim from
  the Fable file; the integrator may want to amend its last clause.

## Owner questions (defaults applied)

1. **The title strip's tools on the Arcade** (the section-bar toggle, inbox and help) behave as in the Lounge. The
   inbox jumps into the Lounge. OK, or should the Arcade show the strip without tools?
2. **No server rail on the Arcade.** The Lounge's left rail is the server list; the Arcade keeps the section bar and
   title strip but not the rail, as the Study does. Should the rail stay visible across rooms?

Earlier questions 1 to 6 were answered by Fable (D-470 to D-475).

## Stack

sr-server 8203 and Vite 5203 (started by me) are stopped and the `sr_arcade_ui` database is dropped. The binaries copy
is in `C:/Users/braxt/Desktop/SIDEROOM_wt/arcade-bin`.
