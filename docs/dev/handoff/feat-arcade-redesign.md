# feat/arcade-redesign: The Arcade redesign (ARC-UI-010, with D-472 Play)

Builder "arcade". Branch `feat/arcade-redesign` from main `d7cf7380`. Not pushed.

Three rounds, each one answering owner feedback:
1. "The arcade page looks like dogshit": a games front page with art, sections, a dialog for listing, and
   designed states.
2. "It still clashes with the rest of the site": moved into the Lounge's look.
3. **The owner's concept art** (`SIDEROOM/design/arcade-concept.webp`, "use it as the desired vibe"), which
   supersedes round 2. The Arcade is now a sibling of the editorial rooms (The Study, the Cinema, the Library), laid
   out like the concept, with Play built in (D-472).

Every API call and behaviour from before is kept.

## Done

| Commit | What |
|---|---|
| `53632bf5` `8d2e58e3` `74582b8f` `b6ed3825` | Round 1. In-house SVG cover art, the built-in catalogue, the card, Playable now, "List a cartridge" in a dialog, designed empty/loading/error states, the Showroom's sample Arcade in the same style, `arcade-room.spec.ts` scoped to the Featured region. |
| `9532e5bd` | Round 2 (the Lounge look; its chrome change was reverted in round 3). Kept from it: **D-474**, the Featured section and its jump link hidden while there are no picks, and **D-475**, the provenance sentence in `arcade.featured.intro` with `arcade.page.intro` deleted. Also decision rows D-470 to D-475 and `SR_SWEEP_DARK` for the sweep. |
| round 3 (this commit) | **Chrome:** the rooms' `SIDE ROOM / The Lounge · The Arcade · The Study` bar (the Cinema and Library pattern, with the Arcade current and underlined in the accent). Under it, the room's pill row: Playable now, [Featured], Server cartridges, Coming soon, then a rule and "+ List a cartridge". The "+ List a cartridge" opens the same dialog through `CartridgesSection`'s `openSignal`. **Hero:** a thin-rule "House games" eyebrow, the display headline "The Arcade.", the italic subline "Play something new.", a line on how Play works, and the two house picks (Land.io, Arena 3D) as large cards with the painting behind a wash of the page colour. **Sections:** thin-rule small-caps labels with a muted note and a rule to the edge. **Cards:** horizontal (painting on the left, display-face title, a small-caps tier tag at the top right where the manifest declares a tier, pitch, players, capabilities as one quiet line, Play at the bottom right), with square corners and hairline borders like the Library's panels. **Server cartridges:** wide rows (art, title, "Listed by {name}" only when the app knows the name, SHA-256 only when the cartridge has a 64-hex object hash, the existing open and unlist buttons). **Coming soon:** row cards with "In development", Notify me (unchanged) and the repository link. **Art:** every cover gets the concept's low light (a warm grade, a lamp glow, a vignette, and a CSS grade of saturation, sepia and brightness), and the whiteboard paper is warmed. **Theme:** `--ui-*` tokens only, so like The Study it follows the member's theme: in Editorial the serif display face and gold accent give the concept's look, and Lounge Light and Dark get the same layout in their own face and accent. **Play (D-472, ARC-UI-011):** new `StartInDialog`. It lists the member's call first ("Your call"), then the chosen server's voice channels, then its text and announcement channels (the composer's + menu rule). Channels come from the live store, or from `GET /guilds/{id}/channels` when the Arcade was opened before the Lounge connected. Picking a channel opens the existing `ActivityLauncher` with the game preselected: the new `applicationId` prop launches once, at open. A refusal falls back to the shelf with the reason, and a successful start takes the member to that channel. Built-ins are matched to the shelf by their manifest name (`shelfName`); a Featured pick uses its own `application_id`. Play shows only on live built-ins and browser-playable picks. The Showroom shows no Play button (no button there reaches the server). On a phone the two hero cards are hidden, since they are the first two cards of the list right below. **D-475** is amended for the new header, and **D-472**'s status is now "built in wave 47". |

## Concept comparison: what was kept, what was left out and why

| Concept | Here |
|---|---|
| SIDE ROOM wordmark bar with the room links | the rooms' text bar, as on Cinema and Library (the door-logo wordmark belongs to the Cinema's own header) |
| Pill row + "+ List a Cartridge" | kept |
| "The Arcade." + italic "Play something new." | kept, with the eyebrow "House games" (not "Featured games": Featured is the Operator's section, D-474) |
| Two big hero cards | kept (Land.io, Arena 3D) |
| Swarm-Safety-Signed / vouch counts / "2 Active Lobbies" chips | **left out**: the built-ins carry no signature, vouch or lobby data in any API. Real trust badges still show on Featured picks through `TrustBadges`. |
| Launch Lobby / Play | **Play** (D-472, above) |
| Copy Guest Link | **left out**: a guest link is minted for a running instance (`ShareLinkDialog`), and a card has none |
| "Receipt Roulette" | left out (cut, D-396); Sketch & Slander appears once its `BUILTIN_GAMES` entry lands |
| Tier tags on cards | only where the manifest declares one (Land.io, Arena 3D: "Original") |
| Active lobbies in your servers | **left out**: no API lists running instances across the member's servers (only `GET /channels/{id}/activity`, one channel at a time). It needs a server route. |
| Cartridge row with owner, SHA-256, vouches, "Use cartridge" | owner and SHA-256 only when known; no vouches (cartridges have none); the existing "Get {title} from the shelf" keeps its name |
| "Side Room Cinema" footer | left out: The Study has no footer, and Cinema's is its own |
| Photographic, moody art | in-house SVG with the low-light treatment; no photos, no downloads |

## Visual check (AGENTS.md section 4)

All shots are in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/`.

- **Rooms comparison, before round 3** (Editorial and Lounge Light, `arcade-rooms-before/`; Lounge Dark,
  `arcade-rooms-before-loungedark/`): arcade, study, cinema-home, library-home at desktop and phone. Finding: The Study follows
  the member's theme (serif in Editorial, sans in the Lounge themes), while Cinema and Library stay editorial in
  every theme. The task asked for the Arcade to follow the theme, so it follows The Study's model, with the Cinema
  and Library's bar and type in Editorial.
- **After round 3**: `arcade-concept/` (arcade, showroom-arcade, admin-arcade, study, cinema-home; Lounge Light and
  Editorial) and `arcade-concept-loungedark/` (arcade, study, cinema-home; Lounge Light and Lounge Dark).
  `arcade-concept/flows/` holds a populated page (a Featured pick with tips, a cartridge with owner and hash,
  Coming soon) and the Start in dialog, desktop and phone. A scratch probe shot them and was deleted afterwards.
- **Before/after pairs**: `arcade-before/arcade-desktop-dark.png` (the original) → `arcade-after/` (round 1) →
  `arcade-lounge/` (round 2) → `arcade-concept/arcade-desktop-dark.png` (round 3). The same chain exists for
  `-desktop-light` and `-phone-*`.
- The sweep flags 0 issues on arcade, showroom-arcade and admin-arcade in every theme.
- Fixed while iterating: the hero text over its art (the wash now runs from the left, and the body keeps to 75%); the
  capability pills on cards (now one quiet line); a title squeezed to "Lant ern Keep" by its tag (the head now
  wraps); two-line pills on tier chips (now 4px corners); the whiteboard paper's glare; the duplicated hero cards on
  a phone.
- Clicked through in the browser: Play → the Start in dialog (shot in `flows/`). The launch itself was proved by
  `startIn.test.tsx`, not clicked live: the fresh test world has no approved built-in Activity on its shelf. Also
  clicked:
  "+ List a cartridge" from the pill row and from the empty state; list and unlist; Notify me; Bitcoin QR; Open the
  emulator.

## Tests and gates (round 3)

- vitest `src/arcade src/showroom src/emu src/activities src/lounge/__tests__ src/i18n`: 929 passed in 113 files.
  New tests:
  - `startIn.test.tsx`: the channel order (voice, then text and announcement, never categories); the call comes
    first; launch with the shelf-matched id, then navigate to the channel; a refused launch falls back to the shelf
    and does not navigate; a Featured pick uses its own id.
  - `arcade.test.tsx`: Play on live built-ins, with no tabindex once a card has its own control; Play on
    browser-playable picks only; the hero cards.
  - `arcadePage.test.tsx`: the rooms bar with the Arcade current, and D-474 and D-475 in the app router.
  - Existing assertions changed only where the structure changed. "Playable now" on a card is now its
    `data-status="live"`, because the section label carries the words.
- Web and e2e tsc, eslint and prettier on the changed files, `check-css-tokens` (69 clean), `i18n-audit` (0), web
  build, `check-entrance-isolation` (clean), `check-bundle-budget` (all ok; first paint `app` unchanged at
  246.8 KB gzip), `playwright test --list` (902 in 66 files), and `arcade-room.spec.ts` against my stack (passed).

## Round 4: Fable's review (on `integration/phase47`)

`SIDEROOM_wt/arcade-fable-review.md`: shippable only with fixes 1 to 6, and the three owner questions answered as
D-476 to D-478 (D-479 for the card wording). Done on `integration/phase47` (worktree `p47-integ`), with Sketch &
Slander already live there.

| Commit | What |
|---|---|
| `b4b34b58` `46efbca7` | **D-476.** `arcade` is a themed section. It is added to `SECTIONS` (shipped default `lounge-editorial`), `THEMED_SECTIONS`, `SECTION_OF`, the Appearance > Sections picker ("Arcade"), the server's `THEME_SECTIONS` (so Admin > Platform appearance and `GET /sr/appearance/defaults` carry it) and the theme validator's `SECTIONS`. The new `ArcadeThemeProvider` resolves it like `CinemaRoot`: the member's Arcade choice, then the platform default, then Editorial. The member's Lounge theme does not carry over. `/games` and `/games/emulator` both use it, so the emulator stays in the Arcade's look. The decisions log gets rows D-476 to D-479, and D-472's status now points to D-478. |
| `d56c4bc2` and the round-4 polish commit | **Fixes 1 to 10.** <br>(1) Hero cards are paintings: the wash sits low and to the left, the top half is clear, the body is 62% wide and anchored low, and there is no capability line. <br>(2, D-479) Member cards show only `guest_play`, `storage`, `byos_read`, `byos_write` and `ingame_voice`, and never a reserved flag. The Showroom's genre tags opt out with `allFlags`. <br>(3) One tier tag in the card's top corner, over the painting, on every card kind. A clean-room game's tag is the short "Clean-room" so it fits the art column. Desktop-only and needs-base-game notes are a meta line with icons, and tags are 12 px. <br>(4) A trust badge that fails, or has no record yet (404), prints nothing on an Arcade card (`useBuildTrust` treats a 404 as "no record"), and an empty `.arc-trust` box is hidden. <br>(5) On a phone the rooms bar wraps like The Study's, and the pill row is one scrolling line with the "+" pinned after it. <br>(6) One-line pitches, four cards per row at 1280, 7 rem thumbnails on a phone. <br>(7) A desktop-only Featured pick ends in "Plays in the desktop app." <br>(8, D-477) `heroSlots()`: the Operator's browser-playable picks at positions 1 and 2, then Land.io and Arena 3D. A pick that is the same game as a built-in, by shelf id (the shelf is read only when there are picks) or else by name, takes that built-in's slot. Admin > Arcade says "Positions 1 and 2 are the Arcade's two large cards." <br>(9, D-478) Start in: board tools (Whiteboard, Tabletop) list text channels first. A voice channel the member is not in is joined first (`joinCall`: the voice controller's `join`, then waiting for `connected`), and a failed join launches nothing and says "That call could not be joined." on the row. <br>(10) A hidden "Your servers" label with one server, voice and text in separate labelled lists, and voice rows named "…, a voice channel" so a text #general and a voice General can be told apart. <br>Minor: the cartridge Unlist button is visible as "Unlist", with the title in its `aria-label`. |

**Visual check (round 4).** All shots are in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/`.

- `arcade-r4/`: Editorial for the default dark pass, and Lounge Light, where the Arcade is now Editorial too (D-476).
- `arcade-r4-loungedark/`: Lounge Dark (`SR_SWEEP_DARK=lounge-dark`). In both folders: arcade, showroom-arcade,
  admin-arcade, settings-user-appearance, study and cinema-home, at desktop and phone.
- `arcade-r4/flows/`: a populated page at 1280, 375 and 320 px (a Featured pick taking Land.io's hero slot, a
  desktop-only pick, a cartridge row, Coming soon), the board-tool Start in dialog, and the failed-join row.

The sweep flags nothing on the Arcade routes or Appearance. study's flag is a 404 on a shelf thumbnail in the
`memory:` storage, and it appears on main too. Before/after for this round: `arcade-concept/` → `arcade-r4/`.

**Tag check on the final code** (`arcade-r4/flows-final/`, at 1280, 375 and 320 px): every card has exactly one corner tag. It is the tier tag; a cartridge's Game or Mod now leads its meta line. A tag never wraps (nowrap, 6 px padding, 0.04em tracking; the painting column is at least 7 rem). The Playwright probe measured every tag: one line, inside the painting column, never overlapping the title. "In development" is the eyebrow above the title, not a tag.

**Clicked through live.** Play Whiteboard → Start in lists text before voice → choosing the voice General joins
first. In this stack voice cannot connect, so the row says the call could not be joined and **no** launch request
is sent; the probe checked this. The successful join → launch → go-to-channel path is covered by
`startIn.test.tsx` only. **Untested on a real deployment:** whether `voiceController().join` connects from the
Arcade page with the Lounge not mounted.

**Gates (round 4).** Results:

- web tsc, e2e tsc, eslint and prettier on the changed files: pass.
- vitest (arcade, showroom, emu, activities, lounge, settings/user, i18n): 1186 passed in 145 files.
- `packages/ui` theme and token tests: 75 passed.
- `check-css-tokens`: 69 files clean. `i18n-audit`: 0.
- web build: ok. `check-entrance-isolation`: clean. `check-bundle-budget`: all ok, first paint `app` unchanged at 246.8 KB gzip.
- `playwright test --list`: 903 tests in 67 files.
- `arcade-room.spec.ts` against my stack: passed. Its fixture now has `guest_play` active plus a reserved `ingame_voice`, and it asserts that plumbing and reserved flags stay hidden (D-479).
- Rust (`sr-features`, `sr-theme`): `cargo fmt --check` clean, `clippy -D warnings` clean, `cargo test` passed (139, 16, 3 and 1 tests).

## Round 5: Fable's second review (D-480 to D-482, ship gate)

`SIDEROOM_wt/arcade-fable-review-2.md` says: ship as wave 47 once its gate items are done.

- **D-480.** `ArcadeShell.tsx` now exports `ArcadeRoomBar`. `/games/emulator` renders it, lazy-loaded, above
  `EmuPage` or `EmulatorSwitchedOff`, inside the Arcade theme. `.emu-page` now uses `.arc-page`'s inline padding and
  width, so the two pages line up. `emuPage.test.tsx` checks that the rooms bar is there with The Arcade current, and
  that the page is editorial for a Lounge Light member.
- **Shots.** `emulator-*` are in `arcade-r4/` (Editorial and Lounge Light) and `arcade-r4-loungedark/`, at desktop and
  phone. Looked at: same bar as the Arcade, same left edge, same face, nothing flagged.
- **Re-shoots from the final commit.** `arcade-r4/arcade-*`, `showroom-arcade-*` and `admin-arcade-*` in both
  folders, and `arcade-r4/flows-final/` (populated at 1280, 375 and 320 px, plus the board-tool Start in). Looked at:
  - One tier tag per card, never wrapping (`5bdd2637`).
  - The Start in dialog shows "Your servers" here because the sweep's account is in two servers (the label is hidden
    only with one).
- **Gate (c), phone section head.** Below 40 rem the "+ List a cartridge" action takes its own full-width line under
  the note.
- **Same-wave polish.**
  - The cartridge SHA-256 shows its first 12 hex characters plus "…". The full hash is in the `title` and in a
    visually hidden span.
  - The Emulator card's link is visible as "Open", with `aria-label` "Open the emulator", so it never wraps.
- **Docs.** The decisions log gets D-480 to D-482, and D-476's status now says it covers `/games/**`. The inventory
  gets **ARC-ART-001** (D-481, scheduled after wave 47, scoped as Fable wrote it), and ARC-UI-010's line now says
  "editorial room by default".
- **Gates.** web tsc, eslint, prettier: pass. vitest (arcade, showroom, emu): 183 passed. `check-css-tokens`: clean.
  `i18n-audit`: 0. Bundle budget: emulator 271.8 KB gzip (was 271.7, budget 320); first paint `app` 246.9 KB (was 246.8: the route file gained one lazy import).

## Not done

- **Active lobbies strip**: built in wave 49 (ARC-LOB-001, D-483 to D-487): see `docs/dev/handoff/feat-p49-lobbies.md`.
- **Copy guest link on cards**: guest links belong to running instances; this could come with the lobbies route.
- **No inventory row** for ARC-UI-010 or ARC-UI-011: add them to the Wave 47 section the sketch branch creates.
- **Richer cover scenes**: the paintings are graded for low light but are still simple vector scenes. Painting
  fuller moody scenes per game (the concept's level of detail) is a follow-up if the owner wants it.

## Owner questions

Round 3's three questions are answered by Fable (D-476 to D-478). New, with defaults applied:

1. (Answered by D-480: the Emulator keeps the Arcade theme and carries its rooms bar.)
2. Fix 11, the richer cover scenes, is scheduled as ARC-ART-001 (D-481).

## Stack

sr-server 8203 (rebuilt from `integration/phase47` with the `THEME_SECTIONS` change) and Vite 5203, both started by me, are stopped and the `sr_arcade_ui` database is dropped. The binaries copy
is in `C:/Users/braxt/Desktop/SIDEROOM_wt/arcade-bin`.
