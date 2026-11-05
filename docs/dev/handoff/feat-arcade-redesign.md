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

## Not done

- **Active lobbies strip**: needs a server route listing the running instances a member can join across their
  servers (owner or integrator: new API).
- **Copy guest link on cards**: guest links belong to running instances; this could come with the lobbies route.
- **No inventory row** for ARC-UI-010 or ARC-UI-011: add them to the Wave 47 section the sketch branch creates.
- **Richer cover scenes**: the paintings are graded for low light but are still simple vector scenes. Painting
  fuller moody scenes per game (the concept's level of detail) is a follow-up if the owner wants it.

## Owner questions (defaults applied)

1. The Arcade follows the member's theme the way The Study does, so the concept's exact look appears in Editorial;
   Cinema and Library stay editorial in every theme. Should the Arcade instead always be editorial, like Cinema?
2. The two house picks are Land.io and Arena 3D, as in the concept (`hero: true` in `builtins.ts`). Should they
   rotate, or should the Operator choose them?
3. Play takes the member to the channel once the game starts. Should it stay on the Arcade instead?

## Stack

sr-server 8203 and Vite 5203 (started by me) are stopped and the `sr_arcade_ui` database is dropped. The binaries copy
is in `C:/Users/braxt/Desktop/SIDEROOM_wt/arcade-bin`.
