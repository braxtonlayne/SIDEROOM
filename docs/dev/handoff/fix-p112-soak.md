# fix-p112-soak (wave 112, builder "soak"): D-1038, MUS-JB-001, SEC-ENT-004

Proof is by CI only. Nothing here ran in a browser. The last commit's subject ends `(UNRUN: proven by CI)`.

## What blocks a first deployment
Nothing found. This is test and CI work only.

## Clock times
Started 01:53 (brief's T = 02:00). Edits, checks and this handoff done by about 02:00. The commit is below the 02:30 limit.

## Done (commit `D-1038 MUS-JB-001: ...`)
1. **The soak's music step already existed** (the commander's reading was wrong). `e2e/tests/voice-soak.spec.ts` has had it since `7f240237` (SEC-AUD-234) and `2915bde8` (SEC-AUD-251). What reads `music` today:
   - `voice-soak.yml`: the dispatch input, `SOAK_MUSIC: ${{ inputs.music && '1' || '0' }}`, and the soak step's name (", with music").
   - The spec: `MUSIC = process.env.SOAK_MUSIC === '1'`, the title, the music block after the joins, `streamsBefore`, the per-member verdict, and the summary.
   - Nothing else. I kept that meaning.
2. **Default of the `music` input is now `true`** (`voice-soak.yml`, input description and header comment). The spec alone, with no env, stays off, so a local run with `SOAK_MUSIC` unset is as before.
3. **The spec got the missing end checks, all inside `if (MUSIC && i !== 0)`** (with music off, no line of the test changes):
   - a stream passing `isMusicStream` (stereo Opus with `stereo=1`) whose bytes grew over the closing window (`musicVerdict` stays as it was);
   - at least COUNT-1 non-music streams, which includes the player's microphone, all with bytes > 0, at least one growing;
   - log lines (below).
4. **Drop ratio:** `max_drop_ratio` is computed from the page's E2EE counters `{encrypted, decrypted, sendDropped, recvDropped}` (`apps/web/src/voice/e2ee/media.ts:27-40`, `:121-140`), one object per page, so it **already counts every stream together, music included**. It cannot be split without app code. The mic's allowance is not loosened (the same 2 %, and "no drops in the tail" stays at 0). Music frames dilute the ratio slightly in the member's favour. I documented this in the spec's header comment.
5. **Time budget:** the pre-music poll (which already existed) and the post-music poll are sequential, bounded to 60 s each per member, normally a few seconds. Starting the music adds well under a minute. The log line "receives the music stream N s after it started" gives the number.

## Not done (why): Item 2, the Firefox music step. No step was added.
Firefox cannot run `voice-live.spec.ts` as it stands:
- `e2e/tests/voice-live.spec.ts:114`: `test.skip(browserName !== 'chromium', 'Fake media flags are Chromium only.')`. On Firefox the listing shows the test, but it skips at once.
- `:34` (`test.use permissions: ['microphone']`) and `:119`, `:121` (`browser.newContext({ permissions: ['microphone'] })`): Firefox does not know these permission names (`e2e/support/voice.ts:9-11` says so and `mediaContext` omits them for Firefox).
- `:35-45`: the Chromium launch args are used. The Firefox project's prefs come from `playwright.config.ts:43-49` and `:72-74` (fake streams, permission disabled). Whether `test.use({launchOptions})` replaces or merges the project's `launchOptions` I could not settle without a run.
- The music step is one `test.step` inside the one test "two users in one voice channel receive each other's audio" (`:110`), so `--grep` on a title cannot narrow to the music step. A Firefox run would be the whole test: sign-in, server and channel creation, invite, registration, join, then the music steps, the ICE restart and the rejoin. I estimate 3 to 5 minutes (a guess; the Chromium run's time is in the CI log).
- The tone itself does not depend on fake capture: `playTone` (`e2e/support/voice.ts:132-170`) builds it in the page with an `AudioContext`, oscillators, `ChannelMerger` and `MediaStreamDestination`, all of which Firefox has. But the tone needs a running `AudioContext`. `e2e.yml:256-263` says Firefox's `AudioContext` never finished resuming on the runner without a PulseAudio null sink, so that step would need the same null-sink block as "Voice E2EE (Firefox)", and the SFU needs `SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)"` (`e2e.yml:253-254`; `playwright.config.ts:40-42`), or ICE fails for Firefox.
- Unknown: `isMusicStream` needs `codec.channels` and `codec.sdpFmtpLine` from Firefox's `getStats`. I cannot confirm from the tree that Firefox reports `sdpFmtpLine` for inbound codecs, and the `stereo=1` test depends on it. `playTone(..., {through:'controller'})` imports `/src/voice/controller.ts` from the Vite dev server, which the CI step already provides.

**Proposed row (touches `voice-live.spec.ts`, which is mine only for a one-line guard, and the e2e step):** SEC-AUD-562 (number is the commander's) "Music step on Firefox".
- Change in `voice-live.spec.ts`: skip only on WebKit (`test.skip(browserName === 'webkit', ...)`); build the contexts through `mediaContext(browser, browserName, user)` or pass `permissions` only when `browserName !== 'firefox'`; make the `test.use` launch args Chromium-only; give the music steps their own test (sign-in moved to a shared helper) so that `--grep "music"` narrows.
- Change in `e2e.yml`: a step "Voice live (Firefox, music)" after "Voice E2EE (Firefox)", copying its announce-IP and PulseAudio lines, running `./deploy/compose/e2e-run.sh --project=firefox tests/voice-live.spec.ts --grep "music"` with its own `--output`.
- D-1038 (WebKit and Safari by hand) is untouched.
Until then MUS-JB-001's "Firefox music leg" stays open as "not built, evidence above".

## FINDERS (D-1020)
`git grep -n "voice-soak\|SOAK_MUSIC"` across docs, deploy, .github, scripts, e2e:
- `docs/dev/owner-checklist.md:51`: "Run `voice-soak.yml` once: `music` on, everything else as it is". Still true; the replacement below is simpler.
- `deploy/UPGRADE.md:109`: "(`voice-soak.yml`, off by default; ...)". **Stale.**
- `docs/specs/lounge-voice-video.md:136`: "With `music` on (off by default; D-720, MUS-JB-001)". **Stale** in that half.
- `docs/inventory/20-launch-readiness.md:50`, `21-feature-phase.md:74, 95, 678`, `docs/dev/entrance-isolation.md:178`, `docs/01-decisions-log.md:998`: describe the step; D-720 says "off by default". Historical; no edit needed.
- No script, other workflow, or e2e file reads the soak's inputs.

## Sentences for other people's files
1. `deploy/UPGRADE.md:109`: replace "(`voice-soak.yml`, off by default; the spec's `SOAK_MUSIC=1`)" with "(`voice-soak.yml`, on by default since wave 112; the spec's `SOAK_MUSIC=1`)".
2. `docs/specs/lounge-voice-video.md:136`: replace "With `music` on (off by default; D-720, MUS-JB-001)" with "With `music` on (on by default in the workflow since wave 112, off in the spec unless `SOAK_MUSIC=1`; D-720, MUS-JB-001)"; and after "whose bytes grew over the closing window" add "; it also needs a stereo line passing `isMusicStream` that grew in the window and a microphone stream from each other member".
3. `docs/dev/owner-checklist.md:51`: replace "Run `voice-soak.yml` once: `music` on, everything else as it is (ten members)." with "Run `voice-soak.yml` once with every input at its default (ten members, ten minutes, camera and music both on)."

## What I checked and each verdict
- Entrance-isolation steps that SEC-ENT-004 waits on (`voice-soak.yml`): matches, unchanged and in this order: 5 "Entrance isolation check on this dist" (`node scripts/check-entrance-isolation.mjs`), 6 "Start sr-server", 7 "Entrance isolation, anonymous and gate cases (Chromium)", 8 the Soak, 9 "The SFU was not restarted".
- e2e `tsc`: `npx tsc --noEmit -p tsconfig.json` from `e2e/`, exit 0 (about 01:55, after the last edit of the spec).
- Prettier on `e2e/tests/voice-soak.spec.ts` and `.github/workflows/voice-soak.yml`: "All matched files use Prettier code style".
- Dry listing (starts no server): chromium: `voice-live.spec.ts:110 Voice live > two users in one voice channel receive each other's audio` and `voice-soak.spec.ts:332 Voice soak > 10 members, 10 min: mute, camera, one reconnect each, E2EE on` (2 tests, exit 0). Firefox: the same two are listed (voice-live skips at run). A `--grep` on a title cannot isolate the music step.
- Parse with the repo's js-yaml 4.3.2: `voice-soak.yml` input defaults `{"count":"10","minutes":"10","max_drop_ratio":"0.02","camera":true,"music":true}`; steps in order: checkout, e2e-stack, Install the browser, Build the web app, Entrance isolation check on this dist, Start sr-server, Entrance isolation anonymous and gate cases, Soak, The SFU was not restarted, Stop sr-server, upload-artifact. `e2e.yml` is unchanged.
- `git diff --stat`: only `.github/workflows/voice-soak.yml` and `e2e/tests/voice-soak.spec.ts` (plus this handoff).

### The reading, written out
Soak, `music=false` (`SOAK_MUSIC=0`): at join no music block (`audioBefore` stays empty); churn as before; at the end the verdict block skips the music branch (`if (MUSIC && i !== 0)`), the summary has no music fields. Behaviour identical to before (only comments changed outside the music branches).
Soak, `music=true`: after all joins are `secure`, each other member must hear COUNT-1 mic streams with bytes; then member 0 calls `playTone` (engine route, 256 kbps) and each other member must hold COUNT streams with bytes (60 s each at most); log lines print; churn (mute, camera, one reconnect per member, member 0 included; the engine is expected to keep its music sender across a socket resume); at the tail start the streams are snapshotted; at the end each other member: `musicVerdict`, then a growing `isMusicStream` line, then the mic streams (count >= COUNT-1, all with bytes, one growing); then the old checks. Member 0 itself is not checked as a receiver of music.
Firefox: no step added (see above).

### Assumptions CI will be the first to test
- The music line's fmtp contains `stereo=1` on the soak's engine route (voice-live passes `isMusicStream` on the controller route; the SDP is the same).
- Music at 256 kbps with ten members on the runner stays inside `max_drop_ratio` and the tail checks (one more E2EE stream per page).
- Member 0 keeps its music across its reconnect (voice-live proves ICE restart and rejoin, not a socket resume).
- No mic stream disappears after a reconnect (count >= COUNT-1).

## FOR THE COMMANDER: reading the CI runs
- **Item 2:** nothing to read on the PR; no Firefox step was added.
- **Item 1 (dispatch):** open the "Soak (10 members, 10 minutes, with music)" step log. Expect: `soak music: member 0 starts the generated stereo tone as its music track`, then `soak music: every other member receives the music stream N s after it started` (N under 60), and at the end nine lines `soak music: member i ok: 1 music line(s), 9 microphone stream(s), music bytes in the tail X`, X > 0, plus `"music": true` and `musicPlayer` entries in the printed summary JSON.
  - Failure of the start: the poll message "member i receives the music stream" times out (music did not negotiate).
  - Failure at the end: "gets stereo Opus music that is still arriving at the end: <sentence>" (from `musicVerdict`), or "has a stereo music line (isMusicStream) whose bytes grew in the closing window: [streams with mime, channels, fmtp]" (shows what Chromium negotiated), or "still has a microphone stream from each of the other 9 members" (a mic stream vanished).
  - The entrance-isolation steps must show "ok" for every surface before the soak starts.
- **Closing text for MUS-JB-001** (once the dispatch is green with music on): "Wave 112 dispatch of voice-soak.yml (run <id>): ten members, ten minutes, music on, every other member received the stereo Opus music line and it was still arriving in the closing window; microphones intact. The music survives an ICE restart and a rejoin in voice-live.spec.ts (SEC-AUD-250). Safari: by hand (D-1038). The Firefox leg is row SEC-AUD-562."
- **Closing text for SEC-ENT-004:** "voice-soak.yml run <id>: 'Entrance isolation check on this dist' and 'Entrance isolation, anonymous and gate cases (Chromium)' passed before the soak, on the dist the soak loaded."
- **Text for RDY-VOX-002** (same dispatch): "Run <id>: ten members for ten minutes on ubuntu-latest, camera <on/off>, music on, E2EE on, no SFU restart, no panic, every member heard the tone at the end." Only if green; if the runner starved ten members, record the passing count as the data point (the existing note).

## Proposed rows
- SEC-AUD-562 (number by the commander): the Firefox music leg (above), size M. Not for this wave.

## Owner questions (default chosen)
None. Default: the Firefox leg waits for a row; the soak stays Chromium-only.

## PROCESSES
`strays.sh p112-soak`: "strays: none found". I started no process.
