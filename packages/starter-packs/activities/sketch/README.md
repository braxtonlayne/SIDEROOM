# Sketch & Slander (ACT-SKS-001)

The drawing and prompt-guessing party game, built in to SIDE ROOM (wave 47). One player per turn draws a secret
prompt; everyone else races to guess it. Phones join from the Activity panel's **Share link** QR code as guests and
become drawing pads.

| Folder | What |
| --- | --- |
| `shared/` | prompts (built-in, original), guess matching and hints, pack validation, canvas, ink, scoring |
| `host/engine.js` | the game, run in the host's page (host-authoritative; `resume()` for `host_handoff`) |
| `client/` | the page: `main.js` (screens), `pad.js` (pointer-event drawing), `net/session.js` (host/guest wiring) |
| `client/platform/` | SIDE ROOM (Embedded App SDK, vendored as `sdk/`) or the local stand-in |
| `tools/bundle.mjs` | builds `dist/` with every check of the server and the Activity CSP |

    pnpm --filter @sideroom/activity-sketch test       # vitest
    pnpm --filter @sideroom/activity-sketch typecheck
    node tools/bundle.mjs --dir out                    # then serve out/ and open
                                                       # ?user=alice and ?user=bob (&guest=1) in two tabs

Rules that hold the game to the platform (see `docs/01-decisions-log.md` D-460 to D-464):

- Secrets (the prompt, the three choices) go only to the drawer and come from the host's own randomness; the shared
  state has a mask. Guild pack prompts stay in the host's page.
- Guests use only the handshake and the data channel (`GUEST_COMMANDS`); `test/guest-sdk.test.js` plays a round with a
  guest SDK that throws on anything else.
- Prompt packs come from one shelf object through `sdk.commands.shelfObjectBytes`: the member's page fetches the
  per-object URL and hands the bytes over, so the manifest has no `connect` list. A pack is JSON
  `{ "name": "...", "prompts": [...] }`, a JSON array, or one prompt per line (64 KB, 5 to 1000 prompts, 48 characters each).
- Nothing reads chat, the Scrapbook or profiles (D-396).
