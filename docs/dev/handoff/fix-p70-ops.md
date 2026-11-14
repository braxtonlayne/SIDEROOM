# Wave 70 handoff: ops (SEC-AUD-250..252; D-724)

Branch `fix/p70-ops`, base `966e5a42`. No page or `apps/web/src` file changed, so no shots.

## FIRST FINDING (first item of wave 71): music is lost on a full rejoin

Two members, `voice-live.spec.ts` on my stack: after the player's voice socket fails with 4006 the controller rejoins (new peer connection, new voice socket, "Reconnecting" then "Voice Connected"). The listener B gets A's **microphone** back (a new stream id) but **no music stream appears in 30 s**. Reproduced on three runs.

Cause, from the code: `controller.ts:952` calls `engine.publishMusic(this.music, ...)` *before* `engine.connect(...)`. `connect()` starts with `this.teardown()` (`engine.ts:331`), which does `this.outgoing.clear()` (`engine.ts:1130`). `republish()` (`controller.ts:519`) re-sends camera, screen and hints but not the music. So the doc comment at `controller.ts:367` ("Kept across a rejoin") is wrong today. A fix is one call that publishes `this.music` in the `.then(...)` after connect, next to `this.republish()` (`controller.ts:970`). Not changed (the engine and controller are read-only this wave).

The step stays in the spec as written and is a `test.fixme(true, ...)` placed after the proofs that pass (the rejoin happened; the microphone grows) and before the music check. Delete that one line with the fix. Because `test.fixme` skips the test from that point, CI shows `voice-live.spec.ts` as **skipped**, not passed; the ICE restart step and the rejoin and microphone checks ran before it.

Reword MUS-JB-001: music survives an ICE restart; it does not survive a full rejoin.

## Done (commits)

1. `2915bde8` **SEC-AUD-251**: `musicVerdict(before, after, tail): string | null` in `e2e/support/voice.ts`. `voice-soak.spec.ts` calls it (`expect(verdict === null, "... : <sentence>").toBe(true)`). `e2e/tests/voice-music-verdict.spec.ts` has 8 cases and no page.
   - Conditions the wave 69 soak asserted, now: (a) the receiver still holds at least COUNT audio streams (kept in the spec); (b) a stream not present before the music is `audio/opus`, 2 channels, and its bytes grew over the closing window.
   - The function says: "no new audio stream", "the new stream is not Opus (...)", "the new stream is mono", "the new stream has N channels, not 2", "the stereo stream stopped: 0 bytes in the closing window".
   - Cases: no new stream; new mono; not Opus; stereo without growth; stereo that grows (good); a stream missing from the tail window (counts from 0); two new streams, one of them music (good); an old stereo stream is not new.
   - **Where it runs in CI:** as a Playwright spec with no page, in the e2e jobs (the `tests/[g-z]` job of `e2e.yml`). The global setup still runs before it, so it needs the stack the job has. I ran it on my stack: 8 passed. I did not run it against a base (there was no function to fail), so "watched fail without the change" does not apply.
2. `1c10d72a` **SEC-AUD-250**: two steps in `voice-live.spec.ts` after the music step.
   - Setup change: the tone is now published through the controller, `playTone(a, { through: 'controller' })` (a dynamic import of `/src/voice/controller.ts` from the Vite dev server, which `e2e.yml` uses for this job). The engine-direct path (what the soak uses) never tells the controller, so a rejoin would lose the music for a reason that is not the product's. `playTone` defaults to the old engine path, so the soak is unchanged.
   - Stereo is not told from the microphone by channels: Chromium reports `channels: 2` for every Opus line. The music line is the one whose negotiated fmtp has `stereo=1` (`isMusicStream`; `InboundAudioStream` gained an optional `fmtp`). The existing step's assertion (`some(opus && channels === 2)`) was true from the microphone alone; it now asserts `isMusicStream`.
   - **ICE restart:** `window.dispatchEvent(new Event('online'))` in A's page (the engine's `deps.onOnline`, `engine.ts:369`). Proof it happened: the `a=ice-ufrag` set of A's newest open peer connection changes, the peer connection count does not grow, and the call bar returns to "Voice Connected". B: the music stream (`fmtp ...stereo=1`) grew over a 5 s window (175,890 bytes total at the first look; the stream id changes per run). **Passes.**
   - **Full rejoin:** a real client `close(4006)` is not usable. The browser reports the code the server answers with, so the engine saw a non-rejoin close and sent RESUME (op 8, same peer connection). The helper `failVoiceSockets` instead dispatches a `close` event with code 4006 on each tracked voice socket, so the engine's own `onSocketClose` runs (4006 is in `REJOIN_VOICE_CLOSES`; the engine closes the real socket itself).
   - Proof it was a rejoin: the call bar shows "Reconnecting" (watched with a raf-polling `waitForFunction` started before the failure) and then "Voice Connected"; A's peer connection count grows (1 to 2) and its voice socket count grows. B: the microphone line returns and grows; the music line does not (see the first finding).
   - Added time: about 30 s; no fixed wait over 5 s, the rest polls.
3. `580ab680` **SEC-AUD-252**: docs (below).

## Answers

- Does music survive an ICE restart? **Yes** (proved, two members, Chromium).
- Does music survive a full rejoin? **No**: fault above.

## voice-live.spec.ts before and after

Before my change (unchanged spec, on the base binaries): **passed** (17.7 s), so the tone move into `support/voice.ts` in wave 69 broke nothing. After: the ICE restart passes; the rejoin step passes up to the `test.fixme` and the test is reported skipped.

## The short soak

`SR_E2E_SOAK=1 SOAK_COUNT=2 SOAK_MINUTES=0.5 SOAK_CAMERA=0 SOAK_MUSIC=1`, two members: **passed** (38.7 s), `failures: []`, both `secure`, drop ratios 0, `droppedInTail` 0. Receiver (member 1): 2 audio streams, the new one `audio/opus`, 2 channels, `bytesInTail` 250,420. The same members pass as before the move. Without `SR_E2E_SOAK=1` the spec skips (the wave 69 handoff did not name that variable). Not run: music off (unchanged code path), more members.

## Docs

- `docs/dev/owner-checklist.md`: the soak dispatch row now carries D-724's instruction (one run, `music` on, ten members; green closes RDY-VOX-002, SEC-ENT-004, MUS-JB-001; red: once more with `music` off, send both) and says rejoin and ICE restart are in the live spec.
- `docs/specs/lounge-voice-video.md`: the soak paragraph (D-724, `musicVerdict`, no rejoin step) and the MUS-JB-001 sentence (what was proved, and the rejoin fault).
- `CHANGELOG.md`, Phase 70 section: **from the plan only**. No other builder's handoff had arrived (no `fix-p70-*.md` in the other worktrees at my last look). The commander must check each clause against the real handoffs (mentions: badge and reply ping; plain: reindex and list text; lines: blocked accounts; spoilers).
- `deploy/UPGRADE.md`, a wave 70 section:
  - no migration;
  - replies ping by default for API clients that send no `allowed_mentions` (`replied_user: false` to avoid it; **from the plan, check against the mentions handoff**; I also wrote that a reply with the ping off no longer pushes);
  - optional `sr-cli reindex`, verified in code: it only queues `search.reindex`; a running server clears the index and rebuilds it from every visible message in pages of 1,000, committing every 50,000, with no DB locks (about 30 s per 200,000 messages on a debug build); search is partial meanwhile;
  - the voice tests.
- `deploy/OPERATOR.md`: no change (no reindex text there). `docs/15`: nothing placed. Developer docs on `allowed_mentions`: nothing under `docs/05` states it; I did not check `apps/web/src/developers`. `.github/workflows/voice-soak.yml`: not changed.
- `node scripts/check-doc-links.mjs`: ok.
- A comment outside my files that is now wrong: `apps/web/src/voice/controller.ts:367` ("Kept across a rejoin"). Replace it with "Lost on a rejoin until it is published again", or fix the code.

## Proposed inventory rows

- **SEC-AUD-250** [TEST] `voice-live.spec.ts`: music across an ICE restart (the `online` event; new ufrag, same peer connection, B's music line keeps growing) passes. Across a full rejoin (4006) the microphone returns and the music does not: `test.fixme`, fault `controller.ts:952` (publishes before `engine.connect()` clears the outgoing map). The tone is published through the controller; stereo is told from the microphone by the `stereo=1` fmtp, not by channels. First item of wave 71.
- **SEC-AUD-251** [TEST] The soak's music check is `musicVerdict`, with eight cases that run in the e2e job without a page.
- **SEC-AUD-252** [DOC] D-724 in the owner checklist and the voice spec; the wave 70 changelog and upgrade notes.
- **MUS-JB-001 "waits on" (D-724):** still the one soak dispatch with `music` on, **and** a wave 71 fix: music is not republished after a full rejoin. Do not add a sentence that it survives a rejoin.

## Proposed threat-model rows

None.

## Changelog sentence

In "For operators": "The voice tests cover an ICE restart and a full rejoin with music: music survives the restart; after a rejoin it is not yet sent again (known, next wave)."

## Owner questions (to the Fable agent)

1. Fix the rejoin as the first item of wave 71 (one call after `engine.connect()` in the controller) and remove the `test.fixme`? (Default taken: yes.)
2. The dispatched soak has no rejoin step (D-724), so a green soak does not cover this fault. Keep MUS-JB-001 open until the fix lands, even after a green dispatch? (Default taken: yes.)

## Base binaries and clean-up

`C:/Users/braxt/Desktop/SIDEROOM_wt/p70-base-bin/` (`sr-server.exe`, `sr-cli.exe`, `BUILT_FROM.txt`) was built from `966e5a42`, crates unchanged. **It is still there until the commander says lines is done.** The cold build `cargo build -j 4 -p sr-server -p sr-cli` took 10 min 18 s.

Dropped `sr_p70_ops` (no `_ui` database), stopped my sr-server and Vite by the listener PIDs of my ports, released the lock, deleted `p70-ops-target`, `p70-ops-bin` and my scratch folder.

## Tests

Run: e2e typecheck (clean), prettier on the e2e files (clean), `voice-music-verdict.spec.ts` (8 passed), `voice-live.spec.ts` (before: passed; after: skipped at the `test.fixme`, everything before it passed), the short soak (passed). Not run: anything else (CI).
