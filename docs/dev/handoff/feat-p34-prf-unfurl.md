# Handoff: feat/p34-prf-unfurl (wave 34, PRF-UNF-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p34-prf-unfurl`, branch `feat/p34-prf-unfurl`, base `integration/phase34`.
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p34-prf-unfurl-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`,
one cargo command at a time. Nothing pushed, no PR, nothing merged. The scratch database `sr_p34unf` was used for the
Postgres suites and dropped at the end.

## What was built (PRF-UNF-001: unfurling with privacy-enhanced embeds, a native player, Watch Together)

**Card, "Play here" (nothing third-party loads on render).**

- `crates/sr-unfurl/src/embed.rs` (new). Provider detection from the address alone, no fetch: YouTube
  (`watch?v=`, `youtu.be`, `embed`, `shorts`, `live`, start time `t=`/`start=`) becomes
  `https://www.youtube-nocookie.com/embed/<id>[?start=n]`; Vimeo (`vimeo.com/<id>`, channels, groups, unlisted hash,
  `player.vimeo.com/video/<id>`) becomes `https://player.vimeo.com/video/<id>?dnt=1[&h=hash]`; PeerTube only for hosts
  named in `SR_PEERTUBE_HOSTS` (`/w/<id>`, `/videos/watch/<id>`) becomes `https://<host>/videos/embed/<id>`. https
  only, no userinfo, no odd port, strict host match (`youtube.com.evil.example` and `evilyoutube.com` are refused,
  tested). Direct media is decided by the content type the unfurl request saw (`video/mp4|webm|ogg|quicktime`,
  `audio/mpeg|ogg|opus|webm|aac|mp4|wav|flac`, `application/ogg`, the HLS types), never by the extension.
- `crates/sr-unfurl/src/fetch.rs`, `transport.rs`: new `BodyClass::Audio` (audio and HLS content types). Like video,
  only the response headers are read, which is exactly the "HEAD content-type path" of the roadmap (the fetcher has no
  separate HEAD verb; a media response never has its body read, and the tests assert one request and no body).
- `crates/sr-unfurl/src/unfurl.rs`: `UnfurledEmbed.sr_play` (`mode embed|direct`, `provider`, `src`, `media
  video|audio|hls`, `sync`). A provider link still gets a card (provider name, "Play here") when its page cannot be
  read (consent walls); the page's own `og:video` iframe address is dropped when the provider's embed is offered.
  `Unfurler::with_peertube_hosts`.
- `crates/sr-chat/src/message.rs`, `embed_rules.rs` (one field, one line): `Embed.sr_play` is carried through the
  stored message and is cleared on every client-written embed, so a bot or webhook cannot forge a play offer (test in
  `rich_tests.rs`).
- `apps/web/src/lounge/msg/EmbedsPlay.tsx` (new), `Embeds.tsx`: the card shows a notice naming the host, a "Play here"
  button, "Watch together" (only where a party can drive the player) and an "always play without asking" checkbox. The
  click creates the provider iframe (`autoplay=1`, `strict-origin-when-cross-origin` referrer because YouTube rejects
  embeds without one, sandbox without top navigation) or the native `<video>`/`<audio>` (`controlsList=nodownload`,
  no download item, context menu suppressed). The client re-validates the server's address (only the two embed hosts,
  or a PeerTube-style `/videos/embed/<id>` path, or an https file); a forged address renders the ordinary card.
- "Load embeds" setting: `EmbedsPrefs.ts` (account settings blob key `load_embeds`: `ask` default, `always`),
  `EmbedsSettings.tsx` (card in Settings > Text & Images, one registration line in `TextImages.tsx`), and the
  remember checkbox on the card and on the theatre gate. A missing or unknown value is `ask`.

**CSP (app policy only).**

- `apps/web/src/security/csp.ts`: `EMBED_FRAME_ORIGINS` (`https://www.youtube-nocookie.com`,
  `https://player.vimeo.com`) in `frame-src` after `'self'`; `media-src` gains `https:`; PeerTube hosts from
  `SR_PEERTUBE_HOSTS` (`cspOptionsFromConfig({ peertubeHosts })`, wired in `vite.config.ts`). The built
  `dist/app/csp.txt` was checked: `media-src 'self' blob: https:` and `frame-src 'self' https://www.youtube-nocookie.com
  https://player.vimeo.com`. `PUBLIC_CSP` and `NO_SCRIPT_CSP` and the Caddyfile are untouched
  (`node deploy/scripts/render-caddyfile.mjs --check` passes; `csp.test.ts` asserts neither mentions a frame or media
  source). `connect-src`, `script-src` and `img-src` did not change.
- `crates/sr-api/src/security_headers.rs`: tests pin the exact hosts to the ones `sr-unfurl` produces
  (`YOUTUBE_EMBED_HOST`, `VIMEO_EMBED_HOST`), the two directive lines in `csp.ts`, and that the API, public and
  no-script policies carry no `frame-src`, `media-src`, `youtube`, `vimeo` or `https:`.

**Watch Together (migration 1700, `sr-watchparty`).**

- `1700_party_media_kinds.sql`: `watch_parties.media_kind` (`cinema` default, `embed`, `direct`), `media_url`,
  `media_provider`; the "media or channel" check now accepts external media, and a check ties the URL to the kind.
  External media stores NULL `title_id`/`asset_id` (no fake catalog ids).
- `crates/sr-watchparty/src/external.rs` (new): `POST /watch-parties` and `PUT /watch-parties/{id}/media` take
  `external: { url, title? }` in place of `title_id`/`tv_channel_id`. The server fetches nothing: it requires https, no
  userinfo, a public-looking name (no IP literals or private ranges, no single-label or `.local/.internal/.lan/.home`
  names, via the unfurl `Policy`), canonicalises a YouTube/Vimeo link to the embed address (kind `embed`) and treats any
  other https link as `direct`. A PeerTube link is refused with a clear message (see below). `MediaView` gains `kind`,
  `url`, `provider` (cinema parties unchanged). The party card and announcement use the external title. Playback state,
  control modes, buffering rule, host handover and the rest are the existing machinery, unchanged.
- Web: `frameProtocol.ts` (pure: YouTube `command` envelope and `listening` handshake, Vimeo `method`/`addEventListener`,
  exact-origin checks, address validation), `externalPlayers.ts` (`FramePlayer` extrapolates the position between the
  frame's reports, confirms `play()` within 1.5 s or reports "needs a click"; `MediaElementPlayer` for direct files),
  `externalHost.ts` (the seam between the controller and the stage), `ExternalStage.tsx` (the gate: nothing loads until
  Load or the remembered setting; then the iframe, whose messages are accepted only from its own window at the
  provider's exact origin, or the media element), `runtime.ts` (`routedPlayer` picks the Cinema session or the external
  host by media kind), `Theatre.tsx`, `ControlBar.tsx` (no scrubber for an unknown length; the +/-10 s buttons seek),
  `controller.ts` (`reapply()`, media key includes the URL, and a `coarseRate` branch: YouTube only has fixed speeds, so
  a 5% nudge cannot be expressed and drift past 400 ms is corrected with a hop instead), `StartPartyDialog.tsx` (paste a
  link, or start from a card's address; the server's 400 reason is shown).
- The provider's own controls are hidden (`controls=0`, `disablekb=1`) so the party bar is the only control and the two
  cannot disagree; no provider script is ever loaded.

## Tests that ran (all green)

| Gate | Result |
|---|---|
| `cargo test -p sr-unfurl` | 37 + 4 passed (provider table, hostile hosts, start times, content-type table, HEAD-style path, PeerTube config, fallback card) |
| `cargo test -p sr-watchparty` (lib) | 43 passed, incl. the external-input table |
| `cargo test -p sr-watchparty --test db -- --include-ignored --test-threads=1` (scratch DB) | 30 passed (4 new: embed party synced and stores no catalog ids; cinema/embed/direct moves; unsafe or ambiguous input refused and control enforced; channel party follows channel rules) |
| `cargo test -p sr-chat` | 353 + 8 passed |
| `cargo test -p sr-api --lib rich_tests` and `--lib watchparty` (Postgres) | 12 and 6 passed (new: play offers on unfurled messages, forged `sr_play` stripped) |
| `cargo test -p sr-api --lib security_headers` | 2 passed (hosts pinned) |
| `cargo test -p sr-api --test openapi_names --test route_matrix` | 2 and 6 passed (no new routes, matrix unchanged, KNOWN_CLASHES still empty) |
| `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on sr-unfurl, sr-watchparty, sr-chat, sr-api, sr-server | clean |
| OpenAPI and client | regenerated (`openapi.json`, `schema.d.ts`; `mocks:generate` produced no diff) |
| `pnpm -r --if-present typecheck` (includes e2e) | clean |
| `pnpm -r --if-present lint`, prettier check | clean |
| web vitest, whole suite, `--maxWorkers=2` | 365 files passed (six files first failed only for the missing `packages/wasm/dist`, copied from `p32-integ`, then passed) |
| `node scripts/build-web.mjs`, `check-entrance-isolation.mjs`, `check-bundle-budget.mjs` | every surface ok, all budgets ok |
| `render-caddyfile.mjs --check` | ok |

New Vitest files: `csp.test.ts` (extended), `embedsPlay.test.tsx` (click-to-load, forged address, the preference,
settings card), `frameProtocol.test.ts`, `externalPlayers.test.ts`, `external.test.ts` (the real controller driving a
fake YouTube frame: play, pause, seek, drift by hop, two clients within tolerance, stall), `externalStage.test.tsx`
(gate, origin and window filtering, exact target origin, release on unmount), `startLink.test.tsx`.

## Not run, and why

- **`e2e/tests/watch-together.spec.ts` was written and typechecks but was NOT executed.** Running Playwright needs the
  built `sr-server` plus `sr-cli`, a web server and a bootstrapped database (long-running processes); the brief's
  resource limits forbid background servers on this machine. It is a CI run. The spec stubs the provider
  (`https://www.youtube-nocookie.com/**` fulfilled with a page that speaks the documented YouTube postMessage protocol),
  so it does not reach YouTube, and serves the repo's generated WAV tone for the direct case. It covers two contexts
  for each kind: gate shows the host and loads nothing (asserts no request reached the host before the click), Load,
  host presses "Play for everyone", both positions agree (0.6 s direct, 1.0 s frame), pause and seek propagate.
  Deviations from the roadmap wording: the direct file is a WAV, not an MP4 (Playwright's Chromium has no H.264),
  and it is addressed as an https host that `serveTone` intercepts, because the server correctly refuses http and
  localhost addresses for a party.
- A live YouTube or Vimeo iframe was never loaded (no network by design). The command names and message shapes follow
  the providers' documented iframe APIs; the first real-browser check is the owner's or CI's.

## Still open, with reasons

- **PeerTube in Watch Together.** The card plays a PeerTube embed (operator-listed hosts), but a party cannot drive it:
  its embed API is a separate channel protocol. The server refuses it as party media with an explanatory message and the
  card hides "Watch together" (`sync: false`).
- **HLS (`.m3u8`) plays natively only (Safari/iOS).** Elsewhere it would need hls.js to fetch playlists and segments,
  which is a `connect-src` to arbitrary origins; the policy deliberately does not open that. The card says so.
- **The party queue stays Cinema-only** (external media is a single item; the table has no external columns), and an
  external party never auto-advances (no known length, so "ended" reports are not accepted).
- **Authz rows for the party kinds**: no new route exists (`POST /watch-parties` and `PUT .../media` take the new
  field), so `route_matrix.tsv` is unchanged. `crates/sr-server/tests/authz_matrix.rs` is prf-profiles' file this wave,
  so the rows are not added there; the same rules (channel permission to start, control mode to change media, blind
  callers get 404) are covered by Postgres tests in `crates/sr-watchparty/tests/db.rs`. If the owner wants matrix rows,
  they are one entry each for create and change-media with `external` bodies.
- **Voice-channel entry point.** "Watch together" on a card and the paste-a-link field in the start dialog (which the
  voice call bar already opens, preselecting its voice channel) are the entries; there is no separate "party card in the
  voice channel" UI beyond the existing channel card, which now names the external title.

## Decisions and choices to review

1. **Referrer on the provider iframe is `strict-origin-when-cross-origin`**, not `no-referrer`: YouTube refuses to play an
   embed with no referrer (error 153). Only the app's origin is sent, and only after the click. Vimeo follows `dnt=1`.
2. **`media-src https:` is broad by the roadmap's own wording**; it applies only to the app document and only the native
   player uses it (after the click or the setting). "No download control" is a UI promise, not DRM: the URL is the URL.
3. **The server validates a party link but cannot verify its content type** (it fetches nothing, by design). A `direct`
   party whose file is not media simply fails to play in each member's browser, with the stage's error line. The names
   it accepts exclude private ranges and local suffixes so members' browsers are not pointed at their own networks.
4. **Cards stay private by default**: `load_embeds` defaults to `ask`; ticking "always" on the card or gate saves it.
5. **`SR_PEERTUBE_HOSTS`** is read by `sr-server` at run time (the offer) and by the web build (the `frame-src` entry):
   the same value must be given to both (documented in `deploy/FAQ.md`). Default empty, so out of the box only YouTube
   and Vimeo are offered.

## Files outside the Split row that were touched (all small, for integration awareness)

`crates/sr-chat/src/message.rs` and `embed_rules.rs` (the `sr_play` field and its clearing); `crates/sr-api/src/routes/
watchparty/mod.rs` (two schema registrations), `.../guilds/channels.rs` and `crates/sr-server/src/watchparty_pg_tests.rs`
(`external: None,` in struct literals), `crates/sr-api/src/routes/messages/tests/rich_tests.rs` (one test),
`crates/sr-db/migrations/1700_party_media_kinds.sql` (mine), `apps/web/vite.config.ts` (one option),
`apps/web/src/store/types.ts` (`sr_play` type), `apps/web/src/lounge/MessageItem.tsx` (passes `channelId`),
`apps/web/src/settings/user/sections/TextImages.tsx` (one card), `apps/web/src/lounge/msg/msg.css`,
`packages/api-client` (generated), `deploy/FAQ.md`.

## Open owner questions

1. Should PeerTube links get Watch Together (needs a small channel-protocol driver for its embed API, which I could not
   verify without loading an instance), or stay card-only?
2. Is native-only HLS acceptable, or should a later wave allow hls.js with a `connect-src` for operator-listed media hosts?
3. `media-src https:` as written, or narrow it to operator-listed media hosts (then a pasted file link from an unlisted
   host would not play)? The current setting follows the roadmap text.
4. Do you want authz-matrix rows for the party kinds once prf-profiles' wave lands (see above)?

## CHANGELOG lines (for the integrator's "Phase 34" section)

- Links to YouTube, Vimeo (and PeerTube instances the operator names) get a "Play here" button. The privacy-enhanced
  player (`youtube-nocookie.com`, Vimeo with Do Not Track) loads only after you press it, or always if you choose
  "Load embeds" in Settings. Direct audio, video and HLS links play in the browser's own player with no download item.
- Watch Together now works with a pasted YouTube or Vimeo link or a media file: everyone presses Load, then play, pause
  and seek are shared through the party. The server fetches nothing (migration 1700).
- The app policy now allows framing `youtube-nocookie.com` and `player.vimeo.com` and https media; the public and
  no-script pages are unchanged.
