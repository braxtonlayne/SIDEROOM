# Handoff: feat/p36-act-cards (ACT-CT-001, Card table: Texas Hold'em)

Branch `feat/p36-act-cards`, base `integration/phase36` (5b242383). Owns only `packages/starter-packs/activities/holdem/*`.
The branch also carries one cherry-picked commit from `feat/p36-act-channel` (6beb30a2, the SDK data API first slice) so the
integration test can run against the real `sdk.state` / `sdk.input` and `LoopbackHub`; drop it if act-channel merges first.

## Done

ACT-CT-001 is built end to end (no Rust, no web app changes, no migrations):

- Hand ranker, TypeScript (`src/ranker.ts`): best five of five to seven cards, all nine categories, wheel and steel wheel,
  kickers, split detection, a plain-English label for the showdown line.
- Betting state machine (`src/engine.ts`, pure, immutable): seats, play-money stacks, blinds, dealer rotation (skips busted
  seats), heads-up blind rules, check/call/raise/all-in, minimum raise, short all-ins that do not reopen the betting
  (and several that add up to a full raise that do), side pots, uncalled-bet return, split pots with the odd chip left of
  the button, folded-out hands, stand-up mid-hand, rebuy, blind changes between hands.
- Dealer (`src/host.ts`): the host owns the table and the deck, applies inputs (identity is the SDK's `from`, never the
  payload), publishes the public view under `host:table`, and sends each player's hole cards with `sdk.input.reply(member,
  bytes)` to that member alone. A `sync` input re-sends the cards to a reloaded player. Errors go back to the sender only.
- Player side (`src/client.ts`), table UI (`src/view.ts`, `style.css`, `strings.ts`), entry (`src/main.ts`), manifest
  (`pack.json`, `kind: activity`, `capabilities: ["datachannel"]`, same shape as the whiteboard's).
- The lobby (and every screen) carries the notice: "Not cheat-proof. Everyone in this room holds the room key, so a modified
  client could read another player's hole cards. This is a friends' table, not a casino." (`data-testid=holdem-warning`).
- Dev mode: `vite` on 5175, open `/?seat=1`; three players share the page over the in-memory hub (host is Ann). I drove a full
  hand, a raise/all-in with a side return, and the auto-deal in the browser pane (no Playwright), no console errors.
- Live-spec case: `live/holdem.case.ts` (`holdemCase(a, b)`, plays one hand: both sit, host deals, each sees only their own
  hole cards, check/call to showdown, reveal, chips add up). For the integrator to paste into `e2e/tests/activities-live.spec.ts`.
  Not compiled or run here (no Playwright, no 8080/5173 in this wave).

## Tests run (all in `packages/starter-packs/activities/holdem`, vitest `--maxWorkers=2`)

- `ranker.test.ts` (18): the standard seven-card cases per category, wheel, no wrap, steel wheel, flush-over-straight,
  quads kickers, two sets, three pairs, ties, ordering across categories, a random symmetry check, category frequencies.
- `engine.test.ts` (28): blinds, heads-up order, rotation, turn and legality errors, min-raise ladder, big-blind option, side
  pots (three all-ins with different stacks), uncalled bet, splits and odd chip, short all-in reopen rules, stand-up, rebuy,
  short blind, and 40 random games of up to 12 hands each with chip conservation checked after every action.
- `table.test.ts` (11): dealer + clients over an in-memory hub: hole cards go only to their owner (state writes never
  contain a hole card, the deck or `holes`), reveals only at a contested showdown, out-of-turn / stale / spoofed actions
  tell only the sender, guests cannot write `host:*` or reply, resync after a reload, late joiners get no cards, message
  validation, document size under 16 KiB for nine seats.
- `sdk.integration.test.ts` (2): the same through the real `SideRoomSDK`, its `LoopbackHub`, and `sdkTransport`; the relay
  carries the hole cards only as `reply` envelopes addressed by member id; a nine-seat all-in hand fits `MAX_DATA_BYTES`.
- 59 tests pass. `eslint src`, `prettier --check`, `tsc --noEmit` clean; `vite build` gives 36 kB JS / 4 kB CSS (13 kB gzip).
- `pnpm -r --if-present typecheck` (all 11 projects, e2e included) passes.

## Integrator actions (I did not commit these: shared files)

1. `pnpm-workspace.yaml`: add `  - packages/starter-packs/activities/*` (the whiteboard and the other two will want the same
   line). Then `pnpm install --lockfile-only` to add the importer rows. I ran it locally with that edit and reverted it.
2. Build the bundle: `cd packages/starter-packs/activities/holdem && vite build`, zip the contents of `dist` (index.html at the
   zip root, allow-listed file types only), upload where `testActivityBundle` is used. `dist` is git-ignored.
3. Paste `holdemCase` into `activities-live.spec.ts`.

## Reconcile with act-channel (what I assumed)

- Identity: the SDK exposes `sdk.isHost` but no member id; `main.ts` reads `member_id` (or `user_id`) from the ready config and
  `host_id` for guests, the same convention as the whiteboard's `identityFromConfig`. If act-channel exposes `sdk.self` /
  `sdk.host` instead, only `startInRoom` in `main.ts` changes.
- Everything else touches the SDK in one file, `src/transport.ts` (`sdkTransport`): `sdk.state.get/set(patch)`,
  `sdk.input.send/reply`, `sdk.on('state'|'input')`. The host's reply to itself is local in the SDK; the table relies on that.
- A guest also receives its own `host:table` writes as state events; the player side ignores older `rev`s.

## Decisions

- Chips are integers; default 1000 stack, blinds 10/20, up to 9 seats (2 to 10 configurable by the host between hands).
- The button moves to the next funded seat (no dead-button rules). No burn cards. No action timer: a player who leaves the
  call is removed by the host (`HostController.kick`, or they press Stand up); the host UI has no kick button yet.
- Auto-deal of the next hand after 7 s is on by default for the host and can be unticked.
- No third-party code, no new dependency (vite/vitest/eslint/typescript versions as `apps/web`). Strings live in `strings.ts`
  (the bundle has no i18n catalog; moving them later is mechanical).
- A stood-up player with chips in the pot while all-in stays in the hand and leaves after it.

## Still open

- Live-spec case unrun (waits for the integration run). Browser smoke only in dev mode.
- No host "remove player" button and no turn timer (a vanished player stalls the hand until the host removes them).
- Right-to-left layout of the table is only approximated (seats use logical insets; the centring transform is flipped for `dir=rtl`).

## Owner questions

None blocking. Optional: should a vanished player be auto-folded after a timeout (say 60 s), and should the table keep running
if the host leaves (today the instance ends with its host, per the roadmap's host-authoritative model)?

## CHANGELOG lines

- New built-in activity: Card table, Texas Hold'em with play-money chips. Seats, blinds, a rotating dealer, side pots and
  showdown; hole cards go only to their owner. It says plainly that it is a friends' table, not cheat-proof.
