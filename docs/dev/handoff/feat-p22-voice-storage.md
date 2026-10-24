# feat/p22-voice-storage: handoff

Branch `feat/p22-voice-storage` (base `integration/phase22`), worktree `SIDEROOM_wt/p22-voice-storage`. Items: RDY-VOX-001, RDY-VOX-002, RDY-STO-001, RDY-STO-002. No migrations used (1260 to 1269 stay unused). No Rust changed.

## Resume here

Everything below is **written, not yet run in CI**. None of the four inventory boxes is ticked; each line in `docs/inventory/20-launch-readiness.md` carries a "written, not yet run" note. The first proof is the first PR run after the owner's push:

1. `e2e.yml` job `live`, step "Voice through TURN relay (Chromium)" (RDY-VOX-001).
2. `storage.yml` jobs `live-s3` (API half, new section 13 of `storage-live.mjs`) and `composer` (browser half) (RDY-STO-001/002).
3. Dispatch `voice-soak.yml` once on the merged tree with the defaults (10 members, 10 minutes) and attach its log here (RDY-VOX-002).

Tick the boxes only after those runs are green. The fixes expected after a first red run are listed under "Likely first-run failures".

## What was built

### RDY-VOX-001 voice through a TURN relay

- `deploy/compose/coturn/render-test-conf.sh <ip> <secret>` renders **`turnserver.prod.conf.example` as shipped** into `deploy/compose/coturn/rendered/` (git-ignored). Only three things differ from a real install, all in the script header: the `CHANGE_ME` values (external IP, REST secret), an appended `allowed-peer-ip=<ip>` (the template denies private ranges, the runner and so the SFU sit in one), and a self-signed certificate so coturn starts with the template's `cert`/`pkey` lines. The script fails if a `CHANGE_ME` is left in a non-comment line.
- `deploy/compose/dev.yml`: new service `coturn-relay` (profile `relay`, host networking, mounts the rendered conf and certificate). The existing `coturn` service and its dev conf are unchanged.
- `e2e/tests/voice-relay.spec.ts`: two Chromium contexts, every `RTCPeerConnection` built with `iceTransportPolicy: 'relay'`. Proof from `getStats`: the local candidate of each selected pair (`transport.selectedCandidatePairId`) is `relay`; every gathered local candidate is `relay`; the server handed out a `turn:` URL whose username is `<expiry>:<user id>` (the `voice.turn.secret` REST path; a relay candidate can only exist if coturn accepted those credentials); audio bytes grow. The evidence is attached to the report as annotations. Gated on `SR_E2E_VOICE=1 SR_E2E_RELAY=1`, so the plain `playwright` job skips it.
- `.github/workflows/e2e.yml`: one new step in the `live` job (this file is outside the Split row; one step and two artifact paths added, nothing else touched). It picks the runner address with `scripts/ice-announce.mjs`, renders the template, starts `coturn-relay`, runs `e2e-run.sh` with `SR_VOICE__ANNOUNCE_IPS`, `SR_VOICE__TURN__SECRET`, `SR_VOICE__TURN__URLS`, saves `server-relay.log` and `coturn-relay.log`, and removes `coturn-relay` so the later preflight step can use port 3478.

### RDY-VOX-002 voice soak

- `.github/workflows/voice-soak.yml`: `workflow_dispatch` only; inputs `count` (10), `minutes` (10), `max_drop_ratio` (0.02). Starts its own `sr-server` (pid file) so a step afterwards can assert the **same process is still running, `/readyz` answers and the log holds no panic** (no SFU restart). `timeout-minutes` is `minutes + 60`, so the phase 2 gate by hand is `minutes=120`.
- `e2e/tests/voice-soak.spec.ts`: N members in one E2EE channel; every member mutes/unmutes repeatedly, turns the camera on then off once and reconnects once (its `/voice/ws` socket is closed from the client, the engine must resume). Everything is scheduled as fractions of the duration. Per member at the end: still `secure` with N members, voice bar connected; media worker never errored and no peer connection went `failed` while a watcher sampled every 10 s; drop ratio of the E2EE frame counters under the limit and zero drops in the closing steady window; no SFU restart seen from the page (members that did not reconnect opened exactly one voice socket, a reconnecting one at most three); a tone is heard (inbound audio bytes and decrypted frames both grow over the closing window). The summary JSON is attached and printed, so the run's log can be attached here.

### RDY-STO-001 chat multipart over real S3

- `e2e/support/s3-sign.mjs` (+ `.d.mts`): SigV4 presign, `ListParts`, `ListMultipartUploads`.
- `e2e/support/storage-live.mjs` section 13 (API half, in `live-s3`): a Patron-limit user, a 140 MiB file (parts of 64, 64, 12 MiB): part 1 only, compare `GET .../parts` with S3 `ListParts`, abort (S3 forgets the upload, the slot answers 404), fresh slot, parts 3 and 1 out of order, compare, send the missing part, compare, complete, object has the original SHA-256, multipart record cleared.
- `e2e/tests/storage-composer.spec.ts` + `storage.yml` job `composer` (browser half): the same size through the real composer file input. (1) abort: part 1 reaches S3, user cancels, ledger row / S3 upload / server slot are all gone. (2) reload: part 1 reaches S3, page reloads, the composer reattaches from the IndexedDB ledger with no click, only parts 2 and 3 are sent, the object's SHA-256 matches, the message is sent. Whenever the upload is held still, the **ledger's `done` equals S3's `ListParts`** (the test holds part PUTs at the Playwright route layer to get a still point).

### RDY-STO-002 reattach over the network

- Part (3) of the same spec: reload, CDP bandwidth limit (3 MiB/s, 150 ms latency) plus a 4 s hold per part, and `SR_E2E_RESTART_CMD` (the job's `sr-restart.sh`: kill sr-server, start it again with the same secret key, wait for `/readyz`) run while the parts are in flight; the upload must still finish with the right bytes.
- **Real defect found by reading the code and fixed**: after a reload or while the API server was down, the reattach asked `GET .../parts` once and a network error turned the file into an error card; `complete` was sent once with no retry. `apps/web/src/lounge/msg/upload.ts` now has `whileServerDown`: listing the parts (reattach and same-file resume) and completing the upload retry with the existing back-off for network failures and 502/503/504 (6 tries, about 15 s), never for a real answer such as 400/404, never after the user cancelled. Two new Vitest cases in `apps/web/src/lounge/__tests__/uploadMultipart.test.ts` (restart during the parts listing; 503 then ok on complete, and 400 not retried). Note the code is in `lounge/msg`; there is no `lounge/uploads` directory.

## What was run locally, and what was not

| Check | Result |
|---|---|
| `uploadMultipart.test.ts` (15 tests, 2 new) | pass |
| web typecheck (`tsc --noEmit`), eslint on touched web files | clean |
| prettier (local copy) on web, e2e, workflows, deploy/compose, docs | clean |
| `e2e` `tsc -p .` | clean |
| YAML parse of `storage.yml`, `e2e.yml`, `voice-soak.yml`, `dev.yml` | ok |
| `render-test-conf.sh`: sed rendering and the `CHANGE_ME` guard | ok (openssl step not runnable under Git Bash path mangling; fine on the runner) |
| `node --check` of `storage-live.mjs`, `s3-sign.mjs`; presign output shape | ok |
| full web Vitest | 2509 tests pass |
| Chromium soak against a local server | joined the call, blocked by a server binary without voice E2EE (see Final local results) |
| coturn relay, Garage multipart, the composer spec, the real soak | **not run** (no Docker, coturn or Garage on this machine); CI only |

## Likely first-run failures

1. **Relay (VOX-001)**: coturn cannot reach the SFU, so the call never connects. Check `coturn-relay.log` (denied peer?) and that `allowed-peer-ip` equals the SFU's announced address. If the runner has several interfaces, `ice-announce.mjs` and the SFU must agree. The SFU is ICE-lite on a UDP socket bound per announce address; a relay candidate reaches it from coturn's relay port (49152 and up, UDP, host network).
2. **Relay**: `denied-peer-ip=0.0.0.0-0.255.255.255` or the `max-bps=2000000` cap in the template may be hit by the fake camera path; the spec uses audio only.
3. **Relay**: Chromium reports `getConfiguration().iceServers[].username` correctly, but if a future Chromium hides it the username assertion needs relaxing to the candidate evidence alone.
4. **Composer (STO)**: Garage CORS for the browser's direct PUT with the `x-amz-*`-less presigned URL; the dev Garage allows it for the Library already, but the Playwright route layer (`route.continue`) changes nothing. If the route layer swallows a request body for 140 MiB, hold-and-continue should move to the CDP `Fetch` domain.
5. **Composer**: the 140 MiB `File` is built in the page and stored as a Blob in IndexedDB; if the runner's Chromium quota refuses it, `chatUploads().add` fails and the localStorage fallback is used (no ledger row, so `stillAndEqual` times out with "no ledger row yet"). Lower `SIZE` to 70 MiB (two parts) if so, and the spec's expectations of part 2/3 shrink accordingly.
6. **Composer**: CDP throttling may not apply to requests that Playwright intercepts; the 4 s hold per part is the fallback that still opens the restart window.
7. **Composer / API half**: the Patron grant is inserted by SQL right after registration; if the entitlement cache was warmed before the insert the first slot answers 413 ("exceeds your limit of 31457280 bytes"). The grant is first in both flows, so this should not occur.
8. **Soak**: 10 contexts with cameras compete for runner CPU. The strict "peers never reopened their voice socket" check is the most likely to flag something real (an SFU restart) or something benign (a rejoin after a heavy stall). `max_drop_ratio` is an input for the drop-ratio check; the camera step is the first thing to cut if the runner cannot keep up.
9. **Soak**: Playwright's default 10 s `expect` timeout applies to the control clicks; under CPU pressure a click may need more.

## Decisions

- Browser-side proofs live in new spec files (`voice-relay.spec.ts`, `voice-soak.spec.ts`, `storage-composer.spec.ts`), each gated by an environment variable so the ordinary `playwright` jobs skip them.
- `e2e.yml` got a single step (the Split row names only `dev.yml` and `coturn/*` for this, but the acceptance says "the Chromium live job runs it").
- Scratch database `sr_p22_voice`, ports 8080/5173 and nothing else were used locally.

## Open questions for the owner

- Is the dispatch soak at ten members on a standard `ubuntu-latest` runner acceptable, or should it run on a larger runner (more CPU) to avoid false failures?
- Safari/WebKit: untested here (Playwright's Linux WebKit has no RTCRtpScriptTransform); no change.

## Final local results

- Full web Vitest (`--maxWorkers=2`, wasm built first): 283 files, 2509 tests, all pass. Web typecheck and eslint clean. No Rust touched, so no fmt/clippy needed.
- Local browser run: the soak at 2 members / 1 minute reached the join but stopped at "member 0 sees all 2 in the MLS group" (E2EE snapshot `false/null/0`). The existing `e2ee/voice-e2ee.spec.ts` fails identically on the same setup, so it is the environment, not the new spec: I could only use the stale Oct 18 `sr-server.exe` of the main checkout (no voice E2EE). A fresh `cargo build -j 2 -p sr-server -p sr-cli` crashed mid-way (rustc exit) and was abandoned. So the soak and relay specs are compile-checked (tsc) and reach the join locally, nothing more. Processes on 8080/5173 were stopped by PID and scratch DB `sr_p22_voice` dropped.
