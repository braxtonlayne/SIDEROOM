# Handoff: feat/p26-pwa-bots (wave 26: service worker proof, TypeScript voice rejoin, Opus encoder)

Branch `feat/p26-pwa-bots`, worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p26-pwa-bots`, target dir `p26-pwa-bots-target`, scratch database `sr_p26_pwa_bots` (dropped). Ports 8081 (sr-server, stopped by PID) and 5174 (`vite preview` of the spec). No migrations. Nothing pushed, nothing downloaded beyond what the commander left in b8461b9f (the lockfiles are as that commit left them).

## Resume here

1. Push the branch and let the `playwright` job run `e2e/tests/pwa-update.spec.ts` (Chromium; Firefox and WebKit skip it). It has run locally only: three times in a row with `--retries=0`, and once against the old worker, where it fails. On the runner it builds `apps/web` itself (about a minute) because that job serves the dev server; it serves on port 4175 (`SR_PWA_PORT`).
2. CI needs `cmake` for `opusic-sys`. `ubuntu-latest` has it. On this Windows machine it is NOT on `PATH`; the Visual Studio 2019 Build Tools carry one, and the `cmake` crate takes it from the `CMAKE` environment variable (see Decisions). Nothing was installed.
3. The `live` job of `e2e.yml` and `sdk-compat.yml` need no edit: they run `voice-bot-live.mjs`, which now makes the TypeScript bot throw its gateway session away halfway (the fresh-session case) and checks that each bot decoded the other's tone.
4. iOS stays untested here (see "Not tested"). It is the owner's device pass.

## Done

### RDY-PWA-001 (service worker update and offline shell)

`e2e/tests/pwa-update.spec.ts`, Chromium only, about 5 seconds of test plus the build. Build A is the real `apps/web/dist`; build B is a copy with a new embedded version ("Phase 99"), new hashed file names and a new worker build id (one `vite build` instead of two; the copy is rewritten by the spec). One `vite preview` serves A, is stopped, and one serves B on the same origin. The test:

- installs A (worker active, controlling, cache `sr-static-<build>`), puts a chat upload row (with a `Blob`) into the `sideroom-uploads` IndexedDB and a "seen" version into localStorage;
- goes offline: the Lounge route is served by the cached shell, `/login` renders, and with the shell removed from the cache a navigation shows `offline.html`; online again the shell is stored again; opening `/offline.html` online must not replace the shell (this is the bug below);
- starts B, triggers the update check the app runs, waits on the worker's `statechange` to `installed`, sees "A new version is ready", checks A's cache is still the only one, clicks Reload, waits for the load and for the new worker to control the page, and sees only `sr-static-<B>` left;
- opens `/help/whats-new` under B (heading shows "Phase 99", the stored "seen" version moves to B) and reads the upload row back, file contents included;
- goes offline again with B.

Bugs found and fixed, each with a unit test in `apps/web/src/pwa/swFetch.test.ts` (new; runs `public/sw.js` against an in-memory cache) or `apps/web/src/lib/whatsNew.test.tsx` (new):

1. `sw.js`: a navigation to `/offline.html` was classified as an app navigation, so online it replaced the stored shell with the offline page, and offline it was answered with the shell. After that every offline navigation showed "You're offline" for the life of the worker. Fixed in `classify` (the offline page is a static file). The spec fails against the old worker at the offline `/login` step and passes with the fix.
2. `sw.js`: any `200 text/html` navigation was stored as the shell, so a captive portal or proxy page could become the offline shell. It is now stored only if it contains the app's mount element (`id="root"`), and the store runs under `event.waitUntil`.
3. `whatsNew.ts`: the help-button badge did not follow a read in another tab (no `storage` listener); after an accepted update every open tab is on the new version and only one is read. `useNewsBadge` now subscribes to the `storage` event while mounted.

Findings not fixed (no owner decision needed, listed for the record):

- Lazy route chunks are cached only after first use (documented in `sw.js`). A browser that never opened the Lounge online gets the shell but not the Lounge code offline (the app shows its error screen). The precache list holds only the entry assets. Precaching the Lounge route chunks would cost bundle bytes on every first visit; left as is.
- The badge itself sits in the Lounge header, which needs a signed-in session; the spec has no API, so it proves the stored "seen" version moves (what drives the badge), and the badge render is covered by `helpButton.test.tsx`.

### DEV-FU-001 (TypeScript voice rejoin, U-06 (a))

`Shard.send` (`packages/bot/src/gateway.ts`) remembers the last op 4 body that joined a channel, clears it on a null `channel_id`, and `handle` sends it right after the READY dispatch (READY only answers an IDENTIFY, so a resume never re-sends). New public `Shard.reidentify()` (forget the session, connect with IDENTIFY) for callers with a stale session and for the live test. Unit test in `sdk.test.ts` (resume does not re-send; fresh READY does; a leave clears it), same shape as `gateway_resume.rs`. `voice.live.test.ts` gained the fresh-session case (`SR_FRESH_SESSION_AFTER_SECONDS`): thrown away at 30 s, a second READY and a second VOICE_SERVER_UPDATE arrive, the bot rejoins the SFU, redoes the MLS handshake and keeps talking. The harness `e2e/sdk-compat/voice-bot-live.mjs` sets it in TypeScript mode.

### DEV-FU-002 (Opus encoder in both SDKs, U-06 (b))

- Rust: feature `voice-opus` (default on) = `dep:opus`; `crates/sr-sdk/src/audio.rs` with `OpusEncoder` (48 kHz, `encode_frame` takes exactly 960 samples per channel, anything else is `AudioError::FrameSize`, `set_bitrate`, `set_inband_fec`), `OpusDecoder::decode_frame(packet, fec)`, `audio_level_dbov`; `lib.rs` module line; `examples/voice_bot.rs` sends an encoded 440 Hz tone with its real level and decodes what it hears (prints `<user> spoke <n> bytes (<level> dBov)`); `voice_transport.rs` gained `encoded_frames_decode_to_sound_on_the_other_side` (tone through the real SFU on loopback, decoded on the other side, level above -20 dBov); `rust.yml` builds, clippies and tests `--no-default-features --features voice-opus`; README documents the feature and the cmake need.
- TypeScript: `packages/bot/src/audio.ts` over opusscript (`{ wasm: true }` always) with `OpusEncoder`, `OpusDecoder`, `audioLevelDbov`, `close()` calling `delete()` (idempotent; a closed coder refuses work), exported from `./voice`. `VoiceSession.adopt(codec)` hands a coder to the session; `leave()` closes it, and a `VoiceLink` rejoin moves it to the new session (so a reconnect does not free a coder in use). opusscript 0.1.1 has no FEC decode or loss concealment, so `decodeFrame` has no `fec` flag (a difference from the Rust name; documented in the file).
- `THIRD_PARTY_NOTICES.md` has the libopus (BSD-3-Clause, text from `opusic-sys-0.7.5/opus/COPYING`) and opusscript (MIT, its `LICENSE`) sections; the web copy is regenerated (`check-notices-copy.mjs` identical).
- Live: Rust bot alone, 40 s: `connected=true resumed=true encrypted_epochs=1 frames=1297`. TypeScript bot with the Rust bot as partner, 60 s: `connected resumed encrypted_epochs=1 frames=2999 heard=2281 loudest=-15dBov readies=2 rejoins=1`, and the partner printed 2000+ lines `spoke 128 bytes (-15 dBov)` for the TypeScript bot's tone. `undecryptable=2053` is frames of the partner that arrived before the bot held the new epoch key after each MLS commit and after the fresh session; it does not fail the run (same counter as before this change).
- D-201 stands: `cargo tree -p sr-server -i opusic-sys` and `-p sr-voice -i opus` find nothing. D-201's text is untouched.
- U-06 ticked in `docs/dev/answers-followups-p16.md`.

## Measurements for DEV-FU-002

- `cmake --version` on this machine: `cmake` is not on `PATH`. The one on disk is `cmake version 3.20.21032501-MSVC_2` from the VS 2019 Build Tools (`...\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe`). A first `cargo build -p sr-sdk --features voice-opus` without it stops in `opusic-sys` with "failed to execute command: program not found, is `cmake` not installed?" (4.8 s). With `CMAKE=<that path>` in the environment of the build it succeeds.
- Offline proof: the whole sr-sdk build and test ran with `CARGO_NET_OFFLINE=true`; `opusic-sys` 0.7.5 built from its vendored tarball.
- Build-time delta: `cargo build -j 2 -p sr-sdk` from an empty target dir without the feature (every other default feature) took 5 m 02 s; adding `voice-opus` on top took 43.7 s (`opusic-sys` + `opus` + `sr-sdk`).
- `cargo deny --offline check`: advisories ok, bans ok, licenses ok, sources ok. Web licence step (`pnpm licenses list --json --prod | node scripts/check-licenses.mjs`): "Licence check ok", opusscript listed as MIT. No change to `deny.toml` or `check-licenses.mjs`.

## Tests run (all on this machine, `-j 2`, one at a time)

- `cargo fmt --check -p sr-sdk`; `cargo clippy -p sr-sdk --all-targets -- -D warnings` for default features, `--no-default-features`, and `--no-default-features --features voice-opus`: clean.
- `cargo test -p sr-sdk`: 50 lib (5 new in `audio`), gateway_resume 2, op8_real_gateway 1, voice_e2ee 5, voice_reconnect 3, voice_transport 6 (1 new): all pass. `--no-default-features`: 41 lib + 3 pass; `--no-default-features --features voice-opus --lib`: 46 pass.
- `packages/bot`: vitest 31 passed, 5 skipped (the live ones; run separately below), `tsc --noEmit`, eslint, prettier clean. Live (`SR_VOICE_LIVE=1`) through `node e2e/sdk-compat/voice-bot-live.mjs --ts --seconds 60` and the Rust mode `--seconds 40` against sr-server on 8081 with the SFU on: both "voice bot run ok". `node --test e2e/sdk-compat/test/voice-bot-live.test.mjs`: 3 pass (1 new).
- Web: `pnpm --filter @sideroom/web typecheck` and `lint` clean; vitest `src/pwa` (4 files, 35 tests incl. 8 new) and `src/lib/whatsNew.test.tsx` (4 new) and `helpButton.test.tsx` pass; `vite build` and `scripts/check-bundle-budget.mjs` ok; `e2e` `tsc --noEmit` clean; prettier clean.
- Playwright: `pwa-update.spec.ts` on Chromium, 3 of 3 with `--repeat-each=3 --retries=0`, then once more on the final build; fails on the old `sw.js`. Run through an untracked local config (`e2e/playwright.pwa-local.config.ts`, not committed) because the shared config starts the dev server on 5173, which belongs to the i18n agent, and runs the Owner bootstrap.
- YAML: `rust.yml` parses with the repo's js-yaml.

## Decisions (conservative defaults)

- Where cmake is missing, builds here use `CMAKE=<VS Build Tools cmake>`; nothing was installed and no setting changed. Documented in the sr-sdk README.
- Build B of the spec is derived from build A (rewritten version, hashed names, worker build id) rather than a second `vite build`, to keep the runner cost to one build.
- The spec lives under `e2e/tests/` so the existing `playwright` job runs it, skips itself outside Chromium, and needs no API or database (its pages are public).
- `e2e/sdk-compat/voice-bot-live.mjs` and its test were edited (outside the pwa-bots row of the Split) because the encoded tone changed what the harness must look for; the edit is limited to the "heard a frame" check and the TypeScript-mode env.
- `packages/bot/src/voiceTransport.ts` gained `adopt`/`takeCodecs` and the close-on-leave (the roadmap names "the session's leave path"); no other behaviour changed.

## Not tested / still open

- iOS and Android installed-app behaviour: Add to Home Screen, the standalone window, the update on a cold start of the installed app, Safari's script eviction after weeks without a visit. WebKit and Firefox do not run the spec (Playwright WebKit does not expose the service worker controls reliably; Firefox does not launch on this machine). The owner's device pass covers these.
- The help-button badge dot itself under a real signed-in session after an update (see findings).

## Open owner questions

- None blocking. Optional: should the Lounge route chunks be precached so a never-visited Lounge works offline (costs first-visit bytes)? Default taken: no.
