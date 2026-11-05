# Wave 47 decisions (Fable, owner-delegated, 2026-11-05), for the wave 47 integrator

Scope: the six owner questions in `docs/dev/handoff/feat-arcade-redesign.md` (branch `feat/arcade-redesign`, ARC-UI-010), the seven in `docs/dev/handoff/feat-p47-sketch.md` (branch `feat/p47-sketch`, ACT-SKS-001, rows D-460 to D-464), and the guest page (`/play`) frame at 375 px. Rules applied: `docs/01-decisions-log.md` wins where it already decides; Discord parity by default; D-385 (nothing app-side on a public page), D-389 (`byos_read` is one object per ask), D-396 (no game touches members' real messages), D-397 (a guest's page sees only what its own game produces), D-399 (developer freedom, capability flags enforced by the platform); WCAG AA; no new dependency or download (owner-only, none needed below). Code read on both branches at their handoff commits (`b6ed3825`, `49ea0bde`); the screenshots in `visual-sweep-shots/arcade-after/` and `visual-sweep-shots/p47-sketch/` looked at. Nothing edited.

Key findings that change the picture:
- The cover SVGs in `apps/web/src/arcade/GameArt.tsx` hold no `<text>` and carry `aria-hidden`; every ribbon, chip and label over or under them is drawn by the card with `--ui-*` tokens. The art is a picture, so WCAG AA has nothing to measure inside it and the status ("Playable now", "In development") is never carried by colour alone.
- The server picker is also the target of "List a cartridge in {server}" (`ArcadeSections.tsx` ~325): a server with no cartridges yet must stay choosable, which rules out "servers with cartridges" as the picker rule on its own.
- Activities start through `ActivityLauncher({ channelId })` (`apps/web/src/activities/ActivityLauncher.tsx`), which lists the shelf and calls `api.launch(channelId, applicationId)`; the Lounge opens it from the composer's `+` menu (`openDialog({ kind: 'startActivity', channelId })`, `Composer.tsx` ~978). A "Start in..." chooser from the Arcade is the same launcher given a channel and a preselected application: web only, no new route.
- Player counts: the manifests of Land.io (1 to 24) and arena3d (1 to 8) match `builtins.ts`; Hold'em's manifest declares none and its engine pins `maxSeats: 9` (`holdem/src/engine.ts` ~22); Sketch's manifest says 1 to 16 participants while D-463 says 2 to 16 players. The manifest bounds the instance; the card should say what the game needs to play a round, which is a different number.
- Admin > Arcade is visible to `can('OPERATOR')` only (`AdminApp.tsx` ~315) and already holds the Featured editor, so the Operator never needs the front page's empty box to find where picks come from.
- `arcade.page.intro` ("Games to play in a call, from the people who made them. Everything here declares who owns its code, art and sound.") is now used nowhere but its message file (`p42-arcade-room-1.ts`). Its second sentence is the Arcade's provenance promise (the tier chips: Original, Clean-room reimplementation, Desktop app only, Needs your own copy of the base game); the first is replaced by the hero pitch.
- `shelfObjectBytes` (`apps/web/src/activities/host.ts` ~568, `fetchCapped` ~313) refuses by `content-length` first and then by the decoded length, so a body sent without `content-length` is downloaded whole before the 1 MiB check. The shelf consent copy (`p46-integ-1.ts`) already says the game "gets that one file, or a link to it". The byos design note (`docs/dev/design/byos-read-activities.md`) lists option 3 as built, and D-446 still says "owner question on the CSP": this wave answers it.
- The Sketch engine already addresses messages to one participant (`send(to, msg)`, `EngineOptions`) and knows which players are guests (`Player.guest`), and `resume()` starts with `packs: []`. The parts needed to hand pack references to members, and only members, exist.
- The guest runner's frame is `.gp-frame { inline-size: 100%; border: 1px }` with no `box-sizing` inside `.gp-main { padding: 16px }` (`apps/web/src/public/guest-play/guest.css`), so at 375 px the frame's content box is 341 px and the element is 2 px wider than its column. The game's palette rule used to switch at 360 px (`@media (max-width: 360px)`, `sketch/client/style.css` ~208) and was laid out for 12 columns in 341 px: that is the "about 20 px wider than visible" of the question, and `505a2478`/`1a838e82` already fixed it in the game. What the `app/*-phone-guest-reveal.png` shots still show cut at the right edge is the scoreboard (`.board-list`, `overflow-x: auto`): a scroller that does not look like one. No page-level clip remains: in the reveal shot the game's fixed toast sits 16 px above the frame's bottom and the frame's bottom padding is visible.

Format per question: id, the decision, why, and for a change what the implementing agent does.

## 1. The Arcade redesign (ARC-UI-010, branch `feat/arcade-redesign`)

### A1: fixed hex palettes inside the cover art
**Keep: the paintings keep their fixed palettes; nothing is tinted from theme tokens.** A cover is content, not a surface: it must read the same in Lounge Light and the dark theme (the screenshots show it does), and a Land.io cover that went beige in the light theme would be a different painting, not a themed one. The token rule (`check-css-tokens`) exists so chrome follows the theme and stays AA against it; the art carries no text, is `aria-hidden`, and everything that carries meaning (the ribbon, the chips, the names) is drawn by the card from tokens, so AA is measured where it applies. The generic paintings' `hsl()` from the title hash is the same kind of thing. `check-css-tokens` does not scan `arcade/GameArt.tsx` by default and must not be pointed at it; the exemption is recorded in the decisions row so nobody "fixes" it later.

No code. (The head comment of `GameArt.tsx` already states the rule; leave it.)

### A2: the server picker rule
**Keep: the picker shows when the member is in more than one server, whatever those servers hold.** Two reasons: the picker is where a member chooses the server they are about to list a cartridge in, so a server with no cartridges must be choosable, and "servers with cartridges" would need every server's list fetched on page load for a section most members scroll past (N requests for a filter). One server means no choice, so no picker, as built.

No code.

### A3: a "Start in..." channel chooser on built-in cards
**Change, later wave (not this branch): live built-in cards and browser-playable Featured picks get a "Start in..." button that opens a channel chooser and launches the game there (ARC-UI-011, M, web only).** The cards and the hero say where games start, which is enough to ship the redesign, but a games page whose cards cannot start a game stays one step short of Discord's shelf, where the Activity is one press away in the channel you are in. The platform already has the pieces: `ActivityLauncher` takes a `channelId` and calls `api.launch(channelId, applicationId)`; what is missing is the channel choice. Not in this wave: the arcade branch is done and tested, and the chooser touches the Lounge's dialog state.

Design for ARC-UI-011 (after `feat/arcade-redesign` merges):
- `apps/web/src/arcade/ArcadeSections.tsx`: `BuiltinCard` (status `live`) and the Featured card (browser-playable only, as the ribbon rule already decides) get a `Button` "Start in..." (`arcade.start.button`), in the footer where the Emulator card has "Open the emulator".
- New `apps/web/src/arcade/StartInDialog.tsx` (a `Modal`, title `arcade.start.title` = "Start {game} in..."): first "This call: {channel}" when the member is connected to a voice channel, then the member's servers with their voice channels first and text channels after (Discord launches Activities in voice channels; the Lounge's `+` menu allows text channels, so both are offered), filtered to channels where the member may start an Activity (the same check the composer's `+` menu makes before `startActivity`). Picking a channel navigates to it and opens the existing `startActivity` dialog with the application preselected: `ActivityLauncher` gains an optional `applicationId` prop that skips the shelf and launches at once (the chooser was the confirmation), falling back to the shelf when the launch is refused (the error path it already has).
- Guests and the Showroom: no button (the Showroom's sample Arcade is static).
- Vitest: the button only on live cards; the dialog lists the current call first; picking calls `launch` with the card's `packageId`; a refused launch shows the shelf. Re-shoot `arcade` desktop and phone.

### A4: Hold'em "2 to 9 players", Whiteboard and Tabletop with no count
**Keep.** The rule to write down: a card's count is what the game needs to play a round, not the instance's participant bounds. Hold'em needs two seats and the engine deals nine at most (`maxSeats: 9`), so "2 to 9" is true whatever the manifest says. Whiteboard and Tabletop are boards: any number of people can be at one, so a count would be invented. When Sketch & Slander joins `builtins.ts` it shows "2 to 16" (D-463), not the manifest's 1 to 16. Land.io and arena3d match their manifests already.

No code now; the sketch entry is in the implementation list (item 6).

### A5: Featured while the Operator has picked nothing
**Change: the Featured section and its hero jump link are hidden while there are no picks (S, web, arcade branch).** The owner called the old page broken because it was bare boxes; an empty dotted box between the games and the cartridges on a phone page that is already 4400 px tall is the same thing with nicer borders. Discord shows no empty shelves. The one person who needs to know where picks come from is the Operator, and Admin > Arcade (Operator-only) already holds the Featured editor, so the front page does not need to teach it. The loading skeleton and the error state are unchanged (the section is decided after the Front Page loads).

Implementation:
- `apps/web/src/arcade/ArcadeSections.tsx` `FeaturedSection`: return `null` when `picks.length === 0`; `ArcadeHero`'s jump links take a `sections` list so "Featured" is left out in the same case (the nav must not point at an anchor that is not on the page).
- `apps/web/src/arcade/__tests__/arcade.test.tsx` ~102: the "Nothing is featured yet" assertion becomes "no Featured heading and no Featured jump link while there are no picks", plus one case with a pick that asserts both appear.
- `arcade.featured.emptyHint` and the empty-state markup go; `arcade.featured.empty` (`p42-arcade-room-1.ts`) is unused after this and goes too (`i18n-audit` stays at 0).
- `e2e/tests/arcade-room.spec.ts` is scoped to the Featured region on fixtures that have picks: unchanged.
- Re-shoot `arcade` desktop and phone, light and dark, on the fresh world (no picks) so the sweep shows the page without the box.

### A6: the dropped intro line (who owns each game's code, art and sound)
**Change: the provenance promise comes back as the second sentence of the Featured intro; the rest of the old intro is retired (S, web, arcade branch).** "Games night, any night" and the pitch are better copy than the old first sentence, but "Everything here declares who owns its code, art and sound" is the Arcade's content-tier promise (the Original / Clean-room / Desktop-only chips, the listing dialog's "another company's names, characters or assets are not accepted here"), and it should be said once where community games begin. The built-ins need no such line (they are SIDE ROOM's own, and "Built into SIDE ROOM" says so).

Implementation:
- `apps/web/src/i18n/messages/en/p47-arcade-1.ts`: `arcade.featured.intro` = "Community games the Operator picked. Each one says who owns its code, art and sound."
- `apps/web/src/i18n/messages/en/p42-arcade-room-1.ts`: delete `arcade.page.intro` (no remaining caller). `i18n-audit` at 0.
- The Showroom's sample Arcade uses the same section, so it inherits the line.

## 2. Sketch & Slander (ACT-SKS-001, branch `feat/p47-sketch`)

### S1: the shape of `shelfObjectBytes` (D-460: 1 MiB cap, shared yes, the app's CSP)
**Keep the shape; it also closes the CSP question D-446 left open. Two small hardenings (S, web, sketch branch).** Option 3 of the byos note is the right default for small files: no host is ever added to a game's policy without a reviewer seeing it, it works for every mount (the mount's CORS for the app origin is checked when the shelf is set up), and the member's one yes covers one object whether the game takes bytes or a URL (D-389). The 1 MiB cap is right for its purpose (packs are 64 KB; level data and config fit) and stays an SDK constant; bigger assets go through `shelfObjectUrl` on a reviewed `connect` host, exactly as D-446 says. Option 2 (a per-instance CSP from the member's mounts) is not wanted: it widens a reviewed policy to hosts nobody reviewed. The app's `connect-src` must name the storage host, as it must for emulator ROMs; a mount usable for ROMs is usable for packs, which is the right coupling. Not on the guest allow-list (D-397), and that stays.

Implementation (hardening only):
- `apps/web/src/activities/host.ts` `fetchCapped`: read `res.body` with a reader and abort the fetch (`AbortController`) as soon as the running total passes `max`, so a body without `content-length` never downloads whole; keep the `content-length` early refusal. The error the game gets says the cap: `'That file is over 1 MiB.'` (RPC `failed`) instead of the generic "not available", so a game can tell its player why.
- Vitest (`host.test.ts`, beside the two existing `shelfObjectBytes` tests): a streamed body over the cap is refused and aborted; a body under it with no `content-length` succeeds.
- `docs/dev/design/byos-read-activities.md`: the "owner decision" heading becomes the decided rule (option 3 for small files, option 1 for big assets, option 2 never); D-446's "owner question on the CSP" is answered by D-465 below.

### S2: a `pickShelfObject` picker instead of pasting "shelf id / file hash"
**Change, follow-up wave (not blocking integration): a member-only `pickShelfObject` bridge command that opens the app's own shelf picker and returns one object reference; the pick is the member's yes for that object (ACT-SDK, M, web and SDK, no Rust).** Pasting a snowflake and 64 hex characters into a game is a developer's workflow, not a member's; the shelf dialog already asks for a yes per object, and a picker is that same yes with the member choosing the file instead of the game naming it. It fits D-389 (still one object per ask), D-397 (never for guests) and D-399 (a flag the platform enforces). Not in this wave: the paste path works, the gate spec proves it, and the picker is new app UI.

Design:
- `packages/embedded-app-sdk/src/protocol.ts`: `pickShelfObject { accept?: string[], max_size?: number } -> ShelfObjectPick | null` where `ShelfObjectPick = { shelf_id, object_hash, name, size, content_type }`; needs `byos_read`; `accept` is file extensions or media types the picker filters on; `null` when the member cancels. Not in `GUEST_COMMANDS`.
- `apps/web/src/activities/host.ts`: the command opens an app-rendered `Modal` (never anything inside the frame, like the consent dialogs) titled "Choose a file for {name}" listing the shelves the member can open and their objects through the existing `listObjects(mountId, prefix)` of `storage/api` (the emulator's ROM list uses it), filtered by `accept`/`max_size`; one picker at a time (a second call while one is open is refused `failed`); on pick, `shelfAllowed.add(key)` so a following `shelfObjectBytes`/`shelfObjectUrl` on that object shows no second dialog; the game learns only the one picked object, never the listing.
- `packages/starter-packs/activities/sketch/client`: the lobby's pack panel gets "Choose a file..." before the paste field (`platform.canPickPacks` when `sdk.commands.pickShelfObject` exists); the paste field stays as the fallback for a shared link.
- Tests: SDK vitest for the command's shape; host vitest for cancel (null), pick (the yes recorded), guest (refused by the allow-list, `guest.test.ts` walks every command); `sketch-gate.spec.ts` keeps proving the pack arrives only through a presigned per-object URL.

### S3: guild packs after a host handoff (D-461)
**Change, follow-up wave: the host shares each loaded pack's reference with the member players (never guests), and a successor reloads the packs through its own member yes; until then it plays the built-in set as D-461 says (S/M, sketch bundle only, no platform change). Amends D-461.** Prompts must stay in the host's page (a guesser who can read the pack cheats), but a pack's reference (`shelf_id`, `object_hash`, name) gives nothing away: shelf ids and hashes are what a member pastes in the open, and a fetch needs that member's session and shelf access. Only members get the reference (`Player.guest` is known to the engine, and `send(to, msg)` addresses one participant), so D-397 holds: a guest's page never learns a shelf id. The successor asks the member exactly as a first load does (the existing `shelfObjectBytes` dialog), so the yes stays the member's, and the game never surprises them mid-stroke.

Implementation (`feat/p47-sketch` follow-up, after the merge):
- `host/engine.js`: `addPack` keeps `{ name, prompts, ref? }`; on `addPack` with a `ref` and on each member join (`!p.guest`), the host sends `{ t: 'packrefs', refs: [{ shelf_id, object_hash, name }] }` to that member; `removePack` sends the updated list. `resume()` takes `refs` from what the page kept and, at the next `pick` phase (never during `draw`), the client calls `platform.loadPack(ref)` for each, one dialog per pack; a decline or a failure drops that pack; `settings.builtIn` stays true until a pack loads. `used` keeps the current word only, as today (a repeat after a handoff is accepted).
- `client/main.js` and `platform/sideroom-platform.js`: keep the last `packrefs` in memory (not `store`: a reference is per instance); the reload runs only for the page that became host.
- `test/engine.test.js`: refs reach members and not guests; a successor with refs reloads at the next pick; a decline keeps the built-in set.
- `docs/01-decisions-log.md`: D-461 is amended by D-467 below; the engine head comment ("packs stay in the host's page: only their names and sizes are public") gains "and their references reach member players".

### S4: "Player" default names (D-462)
**Keep.** Discord would show the username when no display name is set, but here the same name reaches guest pages, and a handle is account data a guest must not see (D-397); the game cannot render one name for members and another for guests because every page draws from the same public state. "Player", "Player 2" with a rename box in the lobby (the screenshots show it is the first thing on the lobby, with Save) is the honest default. Guests keep the callsign the relay gave them, which is already a non-identifying name by design.

No code.

### S5: game defaults (D-463)
**Keep.** 3 rounds, 80 s to draw, 15 s to pick, 2 to 16 players, the scoring and public wrong guesses are the genre's defaults (the drawing-and-guessing party game players know works this way, and wrong guesses in the feed are half the fun and the "slander"). The host can change rounds and drawing time in the lobby; nothing else needs a knob yet. If a real table wants 4 rounds by default, it is one number in `shared/rules.js`.

No code.

### S6: QR through the existing Share link (D-464)
**Keep.** The link's `#key=` is made in the member's page and never crosses into the frame; a QR drawn by the game would need the key, which is exactly what D-397 and the guest design keep from it. The Share dialog already says "Scan it with a phone to use the phone as a controller or drawing pad" and the lobby's pink tip says where it is (`app/light-phone-share-dialog.png`, `game/phone-light-lobby-host.png`). Two presses instead of none is the price of the key staying where it is.

No code.

### S7: English-only text in the bundle
**Keep for this wave; a locale for all built-ins is one later item, not a per-game one.** Every built-in bundle (Land.io, arena3d, Hold'em, Whiteboard, Tabletop) is English with no i18n, and the app's `i18n-audit` never covered bundles. The right fix is one SDK change the platform makes once: `ready` carries the member's locale (and the guest runner the browser's), and each built-in grows a small string table; doing it for Sketch alone would make it the odd one out. Record it as unscheduled, not owner-only.

No code.

## 3. The guest page (`/play`) frame at 375 px

### G1: what the fix should be
**Change (S, two parts, sketch branch): the runner gives the frame the phone's width, and the game makes its scoreboard scroller look like one.** The "about 20 px" was the game's palette laid out for 360 px inside a 341 px frame (375 minus two 16 px gutters minus the border); `505a2478` and `1a838e82` already switch the palette to 6 columns below 360 px, so the game no longer lays out wider than the frame, and the page does not clip it (the toast and the frame's bottom padding are visible in the reveal shot). What still looks cut is the scoreboard: `.board-list` scrolls sideways and the last chip is sliced at the edge with nothing saying "more". A phone used as a drawing pad also wants every pixel of width, and the runner page's 16 px gutters are the join card's, not the game's.

Implementation:
- `apps/web/src/public/guest-play/guest.css`: `.gp-frame { box-sizing: border-box; }` (the 2 px overhang goes); `.gp-play`'s gutter at phone width: `@media (max-width: 480px) { .gp-main:has(.gp-play) { padding-inline: 8px; } }` (or a `gp-main--play` class set by `Runner.tsx` in the play phase if `:has` is avoided), so the frame is 359 px at 375 px; the join card keeps its 16 px. Tokens and logical properties only (the repo test guards both); nothing new loaded (D-385).
- `packages/starter-packs/activities/sketch/client/style.css` and `main.js`: `.board-list { scroll-snap-type: x proximity; } .board-list li { scroll-snap-align: start; }`; the client toggles `.board.more` when `list.scrollWidth > list.clientWidth + 1` (on render and `resize`) and `.board.more .board-list` gets `mask-image: linear-gradient(to right, #000 calc(100% - 1.5rem), transparent)` (`-webkit-mask-image` too), so a cut chip reads as "scroll for more"; with five players or fewer, nothing changes because nothing overflows.
- Re-shoot `app/*-phone-guest-*` through the gate spec (`SR_SKETCH_SHOTS`) and the `game-narrow` set at the new 359 px, and one 320 px frame (an iPhone SE at 100 % zoom) to confirm the 6-column palette and the fade.
- Not this item: the member-side call view clipping the bottom of the Activity panel at 375 px (the sketch handoff's note, `app/light-phone-panel-lobby.png`). That is the Lounge's call layout; the integrator files it as its own visual-check item against the Lounge, not against either branch.

## (a) Implementation list, by user impact

| # | Item | Size | Rust | Branch |
|---|---|---|---|---|
| 1 | A5 Featured section and its jump link hidden while there are no picks; empty-state strings retired; test updated | S | no | arcade |
| 2 | A6 provenance sentence into `arcade.featured.intro`; `arcade.page.intro` deleted | S | no | arcade |
| 3 | G1 guest runner: `box-sizing` on the frame, 8 px gutters in the play phase on phones; the game's scoreboard scroller gets snap and a fade when it overflows | S | no | sketch |
| 4 | S1 `fetchCapped` streams and aborts at the cap; the over-cap error names 1 MiB; byos design note reads as decided | S | no | sketch |
| 5 | S3 pack references shared with member players; a successor reloads packs through its own yes at the next pick (amends D-461) | S/M | no | sketch follow-up (after merge) |
| 6 | A4 when Sketch & Slander joins `builtins.ts`: `players: { min: 2, max: 16 }`, `status: 'live'` once ACT-SKS-001 is ticked, its `arcade.builtin.sketch.name`/`.pitch` strings, generic art until someone paints it | S | no | integrator, on the merged tree |
| 7 | A3 "Start in..." chooser (ARC-UI-011): `StartInDialog`, `ActivityLauncher` `applicationId` prop, buttons on live built-in and browser-playable Featured cards | M | no | new branch after arcade merges |
| 8 | S2 `pickShelfObject` bridge command, the app-rendered shelf picker, "Choose a file..." in the Sketch lobby | M | no | new branch after sketch merges |

Everything else keeps its default and needs no code. Steps for the integrator (no decision): add the ARC-UI-010 inventory row beside the Wave 47 section the sketch builder wrote (the arcade handoff left it for you); run `i18n-audit` after items 1 and 2 (two keys deleted, one changed); the sketch CI gate step is the ACT-SKS-001 tick as the handoff says; `check-css-tokens` is never pointed at `arcade/GameArt.tsx` (A1).

## (b) Owner-only

Nothing in this wave needs the owner: no dependency, no download, no immutable rule, no privacy choice. For the record:
- A locale for the built-in bundles (S7) is not owner-only, only unscheduled: one SDK `ready` field and a string table per bundle.
- Per-instance CSP from the member's mounts (option 2 of the byos note) is decided against (S1), not deferred.
- Unchanged from `fu3-fable-decisions.md` section (b): `sha2` in `sr-core`, a markdown crate, and the older list (analogue arena3d movement, a binary roster v2, the pinned `pcsx_rearmed` HLE start, `sr_p42_arcade_room`, the `lofty` bump, KTX2/Basis, a self-built Jolt, hls.js, `platform.public_profiles`, the ambience mixer, the node agent notice and the CurseForge EULA, sections 4a and 5 of the owner checklist).

## (c) Decisions-log rows

Wave 47 (Sketch & Slander, the guest runner): append after D-464 in `## Phase 41 (2026-11-01, The Study reframe; roadmap P28)` of `docs/01-decisions-log.md`, same 3-column header. The arcade rows follow them, so both branches' rows land in one place at integration (the arcade branch adds none of its own).

| ID | Decision | Status |
|---|---|---|
| D-465 | **D-460 stands as built and answers the CSP question D-446 left open: an Activity reads small shelf files (at most `SHELF_OBJECT_BYTES_MAX`, 1 MiB) as bytes through the member's page (`shelfObjectBytes`), and big assets through `shelfObjectUrl` on a storage host its reviewed `connect` list names; a per-instance CSP built from the member's mounts (option 2 of `docs/dev/design/byos-read-activities.md`) is not built and not wanted, since it would widen a reviewed policy to hosts no reviewer saw.** The member's one yes covers one object either way (D-389); the fetch runs under the app's own policy, so the storage host must be reachable by the app as it must be for emulator ROMs; the page stops reading at the cap and tells the game the cap. Never for a guest (D-397). (p47-sketch question 1) | Adopted (owner-delegated, Fable) |
| D-466 | **A member-only `pickShelfObject { accept?, max_size? } -> { shelf_id, object_hash, name, size, content_type } \| null` bridge command (needs `byos_read`) opens the app's own shelf picker, rendered by the app and never inside the frame, and the member's pick is their yes for that one object; the game learns the picked object only, never a listing. Not on the guest allow-list.** Follow-up after wave 47; pasting a shelf id and hash stays as the fallback. (p47-sketch question 2) | Adopted (owner-delegated, Fable); scheduled as a follow-up |
| D-467 | **Amends D-461: the host sends each loaded Guild Prompt Pack's reference (`shelf_id`, `object_hash`, name; never its prompts) to member players only, on load and on join; after a handoff the successor reloads those packs at the next pick phase through its own `shelfObjectBytes` yes (one dialog per pack) and plays the built-in set until then or when the member declines.** Prompts never leave the host's page; a reference gives a guest nothing because a guest never receives it (D-397) and could not fetch it. (p47-sketch question 3) | Adopted (owner-delegated, Fable); follow-up on the merged tree |
| D-468 | **D-462, D-463 and D-464 stand, and the built-in bundles stay English until one SDK change gives every built-in the member's locale at `ready`:** "Player" because a handle is account data a guest page must not see and the game draws one name for every page; the genre's defaults (3 rounds, 80 s, 15 s to pick, public wrong guesses) with rounds and time adjustable in the lobby; the Share link QR because the link's key never crosses into the frame. (p47-sketch questions 4 to 7) | Adopted (owner-delegated, Fable) |
| D-469 | **The guest runner gives a game the phone's width: on viewports up to 480 px the play phase keeps 8 px gutters (the join card keeps 16 px) and the frame's border counts inside its width, so a 375 px phone gives the game 359 px; a game that scrolls a row sideways must show that it does (snap plus an edge fade when it overflows), as Sketch & Slander's scoreboard now does.** The "20 px" at 375 px was the game's 12-column palette laid out for 360 px in a 341 px frame, fixed in the game by the 6-column rule; the page clipped nothing. (p47 guest page question) | Adopted (owner-delegated, Fable) |
| D-470 | **The Arcade's cover art (`apps/web/src/arcade/GameArt.tsx`, inline SVG) keeps fixed palettes and is never tinted from theme tokens: a cover is content that must read the same in every theme, it carries no text and is `aria-hidden`, and everything that carries meaning around it (ribbons, chips, names) is drawn by the card from `--ui-*` tokens, where WCAG AA is measured. `check-css-tokens` is not pointed at it.** (arcade question 1) | Adopted (owner-delegated, Fable) |
| D-471 | **The Server cartridges picker shows when the member is in more than one server, whatever those servers hold.** It is also where a member chooses the server to list a cartridge in, so a server with none must be choosable, and "servers with cartridges" would fetch every server's list on page load for a filter. (arcade question 2) | Adopted (owner-delegated, Fable) |
| D-472 | **Live built-in cards and browser-playable Featured picks get a "Start in..." button (ARC-UI-011, a later wave): an app dialog offers the member's current call first, then their servers' voice channels and the text channels where they may start an Activity; the pick opens the existing launcher with the application preselected, which launches at once and falls back to the shelf if refused.** Discord's Activity is one press away in the channel; the Arcade should not stop one step short. The cards keep saying where games start until it lands. (arcade question 3) | Adopted (owner-delegated, Fable); scheduled as a follow-up |
| D-473 | **A built-in card's player count is what the game needs to play a round, not the manifest's participant bounds: Hold'em "2 to 9" (two seats to deal, `maxSeats` 9), Sketch & Slander "2 to 16" (D-463), Land.io and arena3d as their manifests; Whiteboard and Tabletop show none because a board has no seats.** (arcade question 4) | Adopted (owner-delegated, Fable) |
| D-474 | **The Featured section and its hero jump link are hidden while the Operator has picked nothing; there is no empty state.** An empty box reads as broken on a long page and Discord shows no empty shelves; the Operator finds the picks in Admin > Arcade, which already holds the editor. (arcade question 5) | Adopted (owner-delegated, Fable) |
| D-475 | **The Arcade's provenance promise ("Each one says who owns its code, art and sound") is the second sentence of the Featured intro, where community games begin; the hero keeps "Games night, any night" and its pitch, and `arcade.page.intro` is retired.** The built-ins need no such line: "Built into SIDE ROOM" says whose they are. (arcade question 6) | Adopted (owner-delegated, Fable) |
