# feat/p36-act-tabletop handoff (ACT-TT-001, Tabletop map and dice)

Branch `feat/p36-act-tabletop` on `integration/phase36` (5b242383). Everything lives in
`packages/starter-packs/activities/tabletop/` plus this note. Commits: d2a2ad08 (core + tests), 69165160 (SDK transport
+ two-SDK tests, on top of the cherry-picked act-channel stub 14990342 = their 6beb30a2), 174aa030 (UI, build, live case).
The stub commit touches only `packages/embedded-app-sdk/src/{data.ts,data.test.ts,index.ts,sdk.ts}`; it is the same
change act-channel owns, so it drops out cleanly when their branch merges first (or on conflict take theirs).

## Done

- Map: square or hex (pointy-top, odd rows shifted) grid over an image; cell size, offset, distance per cell, unit.
- Tokens: a member's own (avatar thumbnail when readable, else a coloured initial), a picked picture, host props
  (monsters, hidden ones), sizes 1 to 3.
- Fog of war: the host paints with a round brush or rectangle, cover all, clear all, off. Run-length encoded in `host:fog`.
  Players do not see hidden tokens or tokens under fog; the host sees through it.
- Initiative: scores, tie-break, start/next/back/stop, rounds, out, "roll initiative" (d20 into the entry), the member on
  turn can end their own turn; the host does the rest.
- Dice tray: dice buttons, the full wave-32 grammar (kh/kl/dh/dl, explode, success counting, fudge, parentheses). The
  grammar is a port of `crates/sr-tools-wasm/src/dice.rs` (same SplitMix64 stream and draw order, `src/dice.js`) because a
  sandboxed bundle cannot reach the app's wasm. Each roll is a record `host:roll:<n>` with who, label, expression, 16-hex
  seed, dice and total; every seat re-rolls the record locally and shows "checks out" (or a red mismatch). "Check a roll"
  lets anyone re-roll an expression with a seed from the log. The same seed on the tools page gives the same dice.
- Measuring: drag between two cells; squares count diagonals as one, 5-10-5 or straight line, hexes count steps; the ruler
  is shared (`ruler:<member>`) so others see it.
- Shared-state design: all authority is under `host:*` (host-only by the SDK). Members send intents with `sdk.input`
  (`move`, `token-add`, `roll`, `init`...), and the host's pure rule engine (`src/table.js handleInput`) checks ownership and
  bounds and writes. Identity comes from the channel (`InputEvent.from`), never from the message.
- Bundle: `node packages/starter-packs/activities/tabletop/build.mjs [outdir]` writes the folder and `<outdir>.zip` (stored
  zip, vendored SDK transpiled into `src/sdk/`, `sideroom.json` declaring `datachannel`). Checked with `unzip -t`.
- Live-spec case: `live/tabletop-live.case.ts` (host sets map, guest adds a token and drags it, both boards show the new
  cell, guest rolls and both seats show the same seed and a passing check). The board exposes `data-tokens` and `data-cell`
  on `[data-testid=board]` for it. NOT run (nobody but act-channel may run Playwright); it needs `@playwright/test`, so copy it
  into `e2e/tests/support/` as its header says.
- Dev preview without a call: `vite` in the folder, then `?loopback` (solo host) or `?loopback=2` (host and guest side by side).
  Looked at once in the Browser pane on 5176 (map, tokens, roll). Server stopped.

## Tests run

- Vitest, 7 files, 85 tests, all green: `cd apps/web && ./node_modules/.bin/vitest run --root ../../packages/starter-packs/activities/tabletop --maxWorkers=2`
  (apps/web is used because it has jsdom for `ui.test.js`; the other six also run from `packages/embedded-app-sdk`).
  - `fog.test.js` (9): brush on square and hex, rectangles, encode/decode round trip, malformed input.
  - `initiative.test.js` (13): order and tie-break, turns, rounds, out, removal, back across a round, sanitising.
  - `dice.test.js` (13): SplitMix64 reference vector, grammar, error codes from the Rust tests, the roll record (seed hex,
    reproducible, verify fails on a changed total/die/seed).
  - `table.test.js` (28): host rules, token ownership, roll record in shared state and its 50-record trim, loopback pair.
  - `sdk.test.js` (4): two real `SideRoomSDK` instances on act-channel's `LoopbackHub`: move a token, map image chunks,
    fog, roll.
  - `ui.test.js` (11): buttons and drag gestures in jsdom (canvas stubbed): token drag, guest cannot move others' tokens,
    measuring, fog painting.
- Prettier check clean on the folder. No Rust, no Playwright, no cargo.
- Not run: web typecheck/lint/build (nothing in `apps/web` touched; the folder is plain JS outside the workspace),
  `check-entrance-isolation` (no web code touched).

## Open, and what the owner or act-channel must settle

1. **Late joiners get nothing once a map image exists.** `ActivityData` answers `sync` with one `snap` envelope, and an
   envelope is capped at 16 KiB (`MAX_DATA_BYTES`). The map image is up to 60 x 12 KB chunks as `host:img:<id>:<n>`, plus
   token thumbnails (up to 8 KB each). act-channel's snapshot must be sent in several envelopes (or the SDK must page it).
   Until then only participants present when the map was set see it. This is the main blocker for real use.
2. **SDK identity.** The Activity needs its own member id and the host id. The stub keeps them private on `ActivityData`;
   `src/transport.js identity(sdk)` reads `sdk.self`/`sdk.hostId`, falling back to `sdk.data.self/host`. Ask act-channel to
   expose `sdk.self` and `sdk.hostId` (I used `sdk.data` only as a fallback).
3. **Images from a mount and avatars.** The Activity CSP (`crates/sr-api/src/routes/activities/proxy.rs`) allows images
   only from `self`, `data:` and `blob:`, and `connect-src 'self'`. So the Activity cannot load a mount file or an avatar by
   URL. What is built instead: the host picks a file from their device, it is shrunk (longest side 1600 px, JPEG steps)
   and shared as chunks; tokens use 64 px thumbnails; member avatars are tried through a canvas and fall back to an initial
   if blocked (they will be, until the client offers pictures). To really read "an image in a mount" the host page needs an
   SDK command that returns bytes or a `blob:`/data URL for a mount file the member may read (for example
   `commands.readMountFile`), or an `img-src` allowance for the app's media host. Owner decision; not built.
4. **Fog is a courtesy.** All participants hold the shared state (the room-key model, same as the card table), so a
   modified client can read what is under the fog and hidden tokens. The UI says so. Stronger fog would need per-member
   replies (`sdk.input.reply`) of only the visible cells; possible later.
5. **The roll seed is chosen by the host's client** (CSPRNG, fresh per roll). A roll is checkable (dice follow from the
   seed), but a modified host could pick a seed. A commit-reveal between roller and host would close that; not built.
6. **Not tested in the real client or against the SFU** (act-channel's relay was not available to this branch). The only
   transports exercised are the SDK `LoopbackHub` and this folder's loopback room. The live case is the first real proof.
7. Strings are the Activity's own table (`src/strings.js`, English only); a bundled Activity cannot use the app catalog.
8. `src/dice.js` must be kept in step with `dice.rs` if the grammar changes (no shared fixtures yet; the tests mirror the
   Rust unit tests and the SplitMix64 reference vector).

## Decisions

- Intents through `sdk.input`, authority in `host:*` keys, rather than members writing token keys directly: the SDK cannot
  restrict a non-host key to its owner, so ownership is enforced by the host's rule engine.
- Plain browser JS (JSDoc-free) for the bundle: no build step apart from vendoring the SDK, so what is tested is what ships.
- The map image travels in the shared state (one more reason item 1 matters); the alternative (a URL) is blocked by CSP.
