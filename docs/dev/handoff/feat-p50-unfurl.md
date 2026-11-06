# Handoff: wave 50 unfurl (PRF-UNF-001, PeerTube parties and non-Safari HLS)

Branch `feat/p50-unfurl`, based on main `7ad12261`. Built to section 3 of Fable's wave-50 decisions (D-515 to D-519).
Nothing pushed, no PR.

## Done

- **D-515/D-516, server.** Migration `2360_party_peertube.sql` (`media_page_url`, `media_format` with the
  `direct`-only check). `sr-unfurl::embed`: PeerTube offers are `sync: true`; `peertube_video_id(url, hosts)`;
  `SR_MEDIA_HOSTS` (`MEDIA_HOSTS_ENV`, `media_hosts_from_env`, same `parse_hosts`). `sr-watchparty::external`:
  `PeerTube` resolver (fetcher + both host lists), `resolve_external` (async; a listed PeerTube link goes to
  `resolve_peertube`, everything else is the unchanged `resolve_with`), `resolve_peertube` (one `GET
  /api/v1/videos/<id>` through `SafeFetcher`, JSON class only, body at the 256 KB cap refused, final URL must stay on
  the instance, largest `files[]` at or under 1080p else first `streamingPlaylists[].playlistUrl`, chosen address must
  be https/public/port 443 and on the instance or `SR_MEDIA_HOSTS`). The four sentences of the spec, plus "That
  instance did not answer. Try again in a moment." (transport errors) and "That instance's answer could not be read."
  (oversize, not JSON, redirect off the host). Private/internal/password privacy ids (3-5) refuse like 401/403/404.
  `MediaView` gains `page_url` and `format` (`WatchPartyMediaFormat`); create, change-media and the row mapping store
  them. `PartyService::with_peertube`; `sr-server` wires it with the link-preview policy
  (`watchparty::peertube_resolver`). **Extra (security):** change-media checks membership and control rights before
  a PeerTube fetch, so a non-member cannot make the server call the instance (re-checked under the row lock as before).
- **D-517, CSP.** `cspOptionsFromConfig({ peertubeHosts, mediaHosts })` adds `https://<host>` for both lists to
  `connect-src` (app policy only; `listedMediaHosts` dedupes); `vite.config.ts` passes `SR_MEDIA_HOSTS`. Public and
  no-script policies unchanged; `security_headers.rs::no_other_policy_names_a_connect_host` pins it.
- **D-518, web.** `apps/web/src/media/hlsAttach.ts`: `canPlayListedHls`, `nativeHls`, `attachHls` (lazy hls.js and
  `createHlsEngine`, refuses unlisted hosts and no-MSE browsers, credentials omitted on both XHR and fetch loaders via
  the new `createHlsEngine(video, { omitCredentials })`). The listed hosts reach the client as the build constant
  `__SR_MEDIA_HOSTS__` (`SR_PEERTUBE_HOSTS` + `SR_MEDIA_HOSTS`; `EmbedsPlay` had no client-side host list to reuse).
  `ExternalStage` direct branch attaches hls.js after Load for an `hls` party on a non-native browser and a listed
  host; native and unlisted paths unchanged; failure shows the stage's error line. `EmbedsPlay` card: listed-host HLS
  plays through hls.js after the click; unlisted keeps the sentence; "Watch together" is offered for PeerTube
  (`sync` from the server) and the notice names "PeerTube". The stage gate links the PeerTube page ("Open on {host}",
  new fragment `p50-unfurl-1.ts`); start-dialog link label/hint updated (the hint no longer says the server fetches
  nothing).
- **Deploy/docs.** `SR_MEDIA_HOSTS` in `prod.yml`, `.env.prod.example`, `deploy_files.rs`, `INSTALL.md`, `FAQ.md`,
  `OPERATOR.md`. D-515 to D-519 copied to `docs/01-decisions-log.md` (after D-501). T55 copied to
  `docs/15-threat-model.md` as written (its named tests exist; it passes `threat-model.test.mjs` when numbered
  consecutively, so the commander must renumber it with T53/T54). Inventory PRF-UNF-001 ticked, with Fable's "later"
  items named (D-519).
- **e2e.** `watch-together.spec.ts`: a PeerTube party against a canned instance (`peertube.test`), resolution,
  gate, Load, shared play, seek, pause. The canned instance is server-side: `external::stub::e2e_fetcher`, honoured
  only by **debug builds** with `SR_E2E_PEERTUBE_FIXTURE=1` (`sr-server/src/watchparty.rs`); `deploy/compose/e2e-run.sh`
  exports it and `SR_PEERTUBE_HOSTS=peertube.test`. (Fable's spec imagined a browser-side `page.route` for the API;
  the API call is made by the server, so a browser route cannot answer it.) The spec skips without the variable.
  The sweep seeds a PeerTube link message in `lounge-channel`.

Commits: `a713809c` (server), `001ed108` (web), `b63d723d` (docs/deploy), `9defafb7` (e2e), plus the follow-up commit
with the phone gate fix, the "PeerTube" label, the startLink test update and this handoff.

## Not done / left

- Nothing of section 3 was left. D-519's "later" items stay later: PeerTube live, segment hosts outside the lists,
  re-resolving expired files, authz-matrix rows for external party bodies.
- The server announcement/party card does not link `page_url`: the chat party card (`party_cards.rs`) links the
  theatre and never named the media address; the theatre gate links the page instead.
- Environment: this worktree's `packages/wasm/dist` lacked `music/`; I copied it from `p50-skins` (local build output,
  no download) so `pnpm build` and Vite resolve. The commander's copy step should include it.

## Visual check

Shots in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-unfurl/`:
- `sweep/` (Lounge Light + Editorial) and `sweep-lounge-dark/` (Lounge Light + Lounge Dark): `lounge-channel` (the
  PeerTube card with Play here and Watch together) and `cinema-party`, desktop and phone. Flags: the console 404s on
  `lounge-channel` (the by-design "no watch party in this channel" 404) and the `cinema-party` 404s for the seeded
  title's progress/playback (no real asset), both pre-existing and unrelated.
- `flow/`: the card, the start dialog, the gate and the loaded stage, desktop and phone, Lounge Light, Editorial and
  Lounge Dark; the click-through (card, Watch together, Start party, Load, play, +10 s seek, pause:
  `started-from-card-desktop.png`, `playing-after-seek-desktop.png`); HLS on a listed host in Chromium:
  zero playlist requests before Load, one after, no cookie (`hls-after-load-desktop.png`; the stub playlist 404s, so
  the stage shows its error line, as intended). No horizontal overflow on any capture.
- Fixed: on a phone the gate with the new page link pushed "Always load embeds" into the party bar
  (before: `flow-before-gate-fix/`; after: `flow/gate-phone-*`).

## Tests run

- Rust: `sr-unfurl --lib` (39), `sr-watchparty --lib` (54), `sr-watchparty --test db -- --ignored` (31, incl.
  `a_peertube_party_stores_the_resolved_playlist_and_the_page`), `sr-api --lib watchparty` (7, incl. the PeerTube
  400 sentence), `sr-api --lib security_headers` (3), `sr-core --test deploy_files` (17); `cargo fmt --check`;
  clippy `-D warnings` on sr-unfurl, sr-watchparty, sr-server, sr-api (all targets).
- Web: tsc; vitest on `src/watchparty`, `src/security`, `src/media`, `src/cinema/player`, `embedsPlay`, catalog;
  eslint and prettier on changed files; `check-css-tokens`, `i18n-audit`, `check-doc-links`, `threat-model.test.mjs`
  (with consecutive numbering); `pnpm build`, `check-entrance-isolation`, `check-bundle-budget`; OpenAPI and the
  generated client regenerated, `mocks:check` green; `playwright test --list` loads every spec;
  `watch-together.spec.ts` 3/3 on Chromium.

## Owner questions (for Fable)

**Answered: D-532 (the card passes the title only when the unfurl carried one: built; both added sentences and the privacy-id refusal stand).** Questions below are kept for the record.

1. The card's "Watch together" passes the card title as the party title, so a party started from a card is called
   "Video from peertube.test" rather than the instance's video name. Prefer the API's `name` when the label is only
   the generic card title?
2. Transport failures and unreadable answers have two sentences of my own (above). Fine as worded?
3. Privacy ids 3-5 in a 200 answer are refused like a 403. Agreed?
