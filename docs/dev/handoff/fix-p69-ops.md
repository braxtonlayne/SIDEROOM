# Wave 69 handoff: ops (SEC-AUD-234..236; D-713, D-714, D-720)

Branch `fix/p69-ops`, base `3d074009` (the integration/phase69 base). No page changed, so no shots.

## Done (commits)

1. **SEC-AUD-234, the soak's music step (D-720)** (`SEC-AUD-234: the voice soak's music step...`).
   - `e2e/tests/voice-soak.spec.ts`: input `SOAK_MUSIC` (`1` turns it on; off by default; documented in the header beside `SOAK_CAMERA`). On: after all members are secure and before the churn, every other member must first hear all microphones (`COUNT-1` audio streams with bytes), then member 0 (the server owner, so it has SPEAK; the channel's `music_allowed` defaults to true, so nothing is set) plays the tone, and every other member must show `COUNT` streams with bytes. At the end every other member must hold at least one audio stream that was not there before the music, `audio/opus` with 2 channels, whose bytes grew over the closing window (the spec's existing tail window). The summary gains `music` (and per member `musicPlayer`, `music`). Thresholds unchanged. With the input off nothing new runs (the summary only says `"music": false`).
   - The tone and the stream listing moved to `e2e/support/voice.ts` (`playTone`, `inboundAudioStreams`, which also returns the stats id); `voice-live.spec.ts` changed only by that move (it imports both). I did not run `voice-live.spec.ts` (it needs its own stack); it typechecks.
   - `.github/workflows/voice-soak.yml`: boolean dispatch input `music` (default false), `SOAK_MUSIC: inputs.music && '1' || '0'` like `camera`; the step name says ", with music" when on. Parsed with js-yaml (inputs and env as intended).
   - Docs: `docs/specs/lounge-voice-video.md` (the soak line), `docs/dev/owner-checklist.md` (see below).
2. **SEC-AUD-235** (`SEC-AUD-235: i18n-audit --max <n>...`).
   - `scripts/i18n-audit.mjs`: `--max <n>` is taken out of the arguments before the root is; a missing or bad value exits 2 with the usage; over the limit exits 1 with `N hard-coded strings, over the limit of n: move the text to a message key: t('...') (the files are listed above).`; without the flag the exit is 0 as before; `--help` / `-h` prints the usage line (it used to be read as a directory). `scripts/i18n-audit.test.mjs` (6 tests, child process on a temp fixture, because the script walks the disk when loaded): no flag, over, at or under (and the flag's place), clean folder with `--max 0`, four bad values, `--help`. All 6 pass.
   - CI: step "Hard-coded strings (i18n audit)", `node scripts/i18n-audit.mjs --max 0`, in `repo-checks.yml` (it needs no build or install). **It fails on this branch's base: 2 hits, `admin/cinema/SubtitlesPanel.tsx`, until the media builder's item lands. Merge media first, or run the gate after both.** (I did not touch those strings.)
   - `AGENTS.md` section 5 (definition of done): one line for the audit (no list of checks exists in `AGENTS.md`, `CONTRIBUTING.md` or `docs/dev`).
   - `crates/sr-voice/src/process_cpu.rs`: one test under `#[cfg(target_os = "linux")]`, `the_real_proc_stat_gives_a_value_that_does_not_go_back` (text below). **It is not compiled on this machine** (rustfmt was run on the file and parses it); CI compiles and runs it first.
   - Docs: `docs/12-deployment-and-operations.md`, where the metric is described, got D-713's sentence ("It stays a counter of whole seconds (D-713): read it with `rate()` over a minute or more, where a whole second is noise.").
3. **SEC-AUD-236, documents** (three commits).
   - `deploy/OPERATOR.md`: both host rows say `sr-cli preflight` shows the dropped-host warning in its `web` row (the `SR_PEERTUBE_HOSTS` row already said it; D-714 added, no new row id); `docs/dev/server.md` (the preflight ids paragraph) says the same where the `web` row is described.
   - `CHANGELOG.md`: a Phase 69 section (Members from the notify and threads handoffs; Staff and operators from the plan). `deploy/UPGRADE.md`: a wave 69 section (migration 2970, push in plain text, the `music` input, the audit step for forks).
   - `docs/dev/owner-checklist.md`: the `voice-soak.yml` dispatch row now says "with `music` on once" and that it serves SEC-ENT-004, RDY-VOX-002 and MUS-JB-001; **added** the RDY-DOC-003 row (walk the RUNBOOK playbooks once on a running stack); RDY-DEV-003 named in the discord.py/twilight row. SEC-ENT-004 and DEP-FU-001 (tantivy, on or after 2026-11-17; two rows) were there and right.
   - `node scripts/check-doc-links.mjs` passes. `prettier --check` does not apply to `*.md`, `docs` or `.github` in this repo (`.prettierignore`); the e2e files and the two scripts are prettier-clean; the `@sideroom/e2e` typecheck passes; the e2e package has no eslint.

## Not done / for the commander to check

- **The media builder's handoff had not arrived** (its worktree had none). The UPGRADE line on migration 2970 is written from the migration file in `p69-media` and the base code: it says a link carrying `subtitle_language=eng` finds nothing afterwards because the filter (`cinema_repo.rs:437`, `= ANY(t.subtitle_languages)`) compares the value as written. **Check against media's handoff: if media made the filter read the long form, delete that sentence.** The Phase 69 "Staff" line is from the plan only.
- Notify's and threads' handoffs were read; their sentences are in the changelog. Their proposed inventory and threat-model rows are theirs; I placed nothing in `docs/15`, the inventory or the decisions log.

## Base binaries (for the threads builder)

`C:/Users/braxt/Desktop/SIDEROOM_wt/p69-base-bin/` holds `sr-server.exe` and `sr-cli.exe`, built from `3d074009` (crates unchanged; only a `cfg(linux)` test in `sr-voice` was added after the build started, which changes no binary), with `BUILT_FROM.txt`. **Still there; delete it when threads is done.** Cold build `cargo build -j 4 -p sr-server -p sr-cli`: 8 min 45 s.

## The soak runs

Inside the lock, my own sr-server binary, fresh `sr_p69_ops`, E2EE on; dev stack: sr-server on 8474 with `SR_VOICE__ENABLED=true`, Vite dev on 5474 with `VITE_SR_E2E=1`, Chromium fake devices (the spec's own flags). `SOAK_COUNT=2 SOAK_MINUTES=0.5 SOAK_CAMERA=0`, `playwright test --project=chromium tests/voice-soak.spec.ts --retries=0 --workers=1`:

- **`SOAK_MUSIC=1`: passed** (run twice: 46.6 s and 39.4 s). Both members `secure`, 2 members each, `recvDropRatio` 0, `sendDropRatio` 0, `droppedInTail` 0, `failures: []`. The receiver (member 1) had 2 inbound audio streams at the end; the new one `audio/opus`, 2 channels, `bytesInTail` 249759 (the other run's receiver: 301651 audio bytes in the tail in all). Member 0, the music player, had `reconnected: true` and `voiceSockets: 2`.
- **`SOAK_MUSIC` unset: passed** (39.2 s), `"music": false` in the summary and no music field per member: the same as before.
- Not run: a negative control (the music assertion failing when no music is played), and a run with more than two members, camera on, or the two-hour length. The first attempt through `pnpm --filter @sideroom/e2e exec playwright ...` printed nothing for 9 minutes and never reached the global setup (no account existed; pnpm buffers filtered output); I killed that chain by PID and ran `npx playwright` from `e2e/` directly, which ran at once. Nothing else differed.

## Does music survive a reconnect? Yes, as far as the soak shows

The engine keeps `outgoing` (including the `music` sender) and its peer connection across a voice-socket resume (`engine.ts` `openSocket(true)`; only `teardown` clears `outgoing`, and the controller re-publishes `this.music` on a rejoin, `controller.ts:952`). The soak's reconnect closes the socket with code 3999 (a resume, not a rejoin), and member 0 (the player) took part in the churn (`reconnected: true`); the receiver's music stream was still growing in the closing window. So the music member is **not** left out of the reconnect churn. Not tested: a full rejoin or an ICE restart with music on (the soak does neither).

## The `/proc` test's text

```rust
#[cfg(target_os = "linux")]
#[test]
fn the_real_proc_stat_gives_a_value_that_does_not_go_back() {
    let read = || {
        let stat = std::fs::read_to_string("/proc/self/stat").expect("/proc/self/stat");
        parse_cpu_ticks(&stat).expect("utime and stime in /proc/self/stat")
    };
    let first = read();
    let start = std::time::Instant::now();
    let mut x = 0u64;
    while start.elapsed() < std::time::Duration::from_millis(50) {
        x = std::hint::black_box(x.wrapping_add(1));
    }
    assert!(
        read() >= first,
        "CPU ticks went back after a busy loop ({x})"
    );
}
```

Uncompiled here; CI compiles it first. **Tests I watched fail without the change: none** (I did not run the base for any of them; `i18n-audit.test.mjs` cannot pass without the flag existing, by construction; the soak's music assertion was not run on a failing case).

## Proposed inventory rows (after SEC-AUD-224 and the other builders' 225..233)

- **SEC-AUD-234** [TEST] The voice soak has a music step: `SOAK_MUSIC=1` / the workflow's `music` input; one member plays the generated stereo tone, every other member must receive a new stereo Opus stream whose bytes grow in the closing window (D-720). Tried here with two members, music on and off: both passed. Narrows MUS-JB-001.
- **SEC-AUD-235** [CI] `node scripts/i18n-audit.mjs --max 0` runs in Repo checks (exit 1 over the limit; usage error 2); a Linux-only test reads the real `/proc/self/stat` through the CPU module's parser (D-713's sentence in doc 12).
- **SEC-AUD-236** [DOC] The two ops answers (D-713, D-714), the wave 69 changelog and upgrade notes, the owner checklist rows (soak with `music`, the RUNBOOK walk, the SDK pair named).
- **MUS-JB-001 "waits on" text (D-720):** "waits only for the owner's `voice-soak.yml` dispatch with `music` on (with RDY-VOX-002 and SEC-ENT-004; one dispatch serves all three); the step is built (SEC-AUD-234) and passed with two members. The Firefox and WebKit live runs are TV-HLS-005's download and no longer wait here."
- Closes: none outright. RDY-VOX-002, SEC-ENT-004, RDY-DOC-003, RDY-DEV-003 and DEP-FU-001 stay open as the plan says.

## Proposed threat-model rows

None.

## Changelog sentence

In the Phase 69 section's "For operators" line: "The voice soak can play music (a `music` input, off by default); the web app is checked for hard-coded text in CI."

## Owner questions (to the Fable agent)

1. The soak with music has only run with two members here. Should the owner's first dispatch use `music` on with the default ten members, or one run with and one without? (Default taken: one dispatch with music on, `docs/dev/owner-checklist.md`.)
2. Should the soak also exercise a full rejoin (not only a socket resume) with music on? The controller's rejoin path re-publishes the music stream, but nothing proves it yet.

## Clean-up

Dropped `sr_p69_ops` (the soak used no `_ui` database); stopped my sr-server and Vite by PID (the listeners of 8474, 5474 and 9174; I also killed my own wrapper scripts and the hung Playwright chain by PID); released the lock; deleted `p69-ops-target`, `p69-ops-bin` and my scratch folder. `p69-base-bin` is **left in place** until the commander says threads is done.
