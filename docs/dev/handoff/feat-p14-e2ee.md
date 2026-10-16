# Handoff: feat/p14-e2ee (voice and video end-to-end encryption, VOX-E2E-001..008)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p14-e2ee, branch feat/p14-e2ee (base integration/phase14 = main). Local commits only, nothing pushed. Scope: `docs/inventory/08-lounge-voice.md` VOX-E2E-001..008. No migrations (nothing is stored server side).

## The blocker: no MLS (VOX-E2E-001)

The spec's key agreement is MLS through `openmls` (docs/03, approved). The `openmls` crates (and their `hpke-rs`/`tls_codec`/`openmls_rust_crypto` dependencies) are **not in this machine's offline cargo cache**, and the working agreement is to ask the owner before downloading anything. A subagent cannot ask, so MLS was not built. Everything else was built against a narrow seam, so MLS can be dropped in without touching the frame layer:

- Each sender's per-epoch base secret = `MLS-Exporter("SideRoom Secure Frames v1", sender user id as u64 little-endian, 16)` (`sr_e2ee::keys::SENDER_SECRET_LABEL`).
- The client feeds it to the worker: its own secret with `prepareSend(epoch, secret)`, everyone's with `setReceive(epoch, userIds, secrets)`, and calls `execute(epoch)` on the server's `execute`.

Because nothing can supply keys yet, **E2EE is off** on both sides: server config `voice.e2ee` (default false; op 101 is ignored) and the client build flag `VITE_SR_VOICE_E2EE` (default off; `voice/e2ee/flag.ts`). With both off, behaviour is exactly as before this branch.

## Done

| Item | What | Where |
|---|---|---|
| VOX-E2E-002 (ticked) | Frame encryptor/decryptor. AES-128-GCM (RustCrypto `aes-gcm`), nonce = 8 zero bytes + u32 frame counter, tag truncated to 8 bytes. Codec-aware clear bytes: Opus none (as DAVE), VP8 3 bytes (interframe) or 10 (keyframe), H.264 start codes + NAL header bytes + whole SPS/PPS/AUD + slice header through `pic_parameter_set_id` (the receiving packet buffer parses those before the receive transform runs). The ranges depend only on clear bytes, so the receiver derives the same ones; H.264 output with start-code emulation is re-encrypted with the next counter. Fails closed: unknown codec, missing/bad trailer, bad tag are errors, never passthrough. | `crates/sr-e2ee/src/{frame,codec,keys}.rs` |
| VOX-E2E-006 (ticked) | Epoch transitions. Receivers: `KeyRing` keeps the previous epoch for 10 s. Senders: the new epoch's encryptor is prepared on the commit and switched to only on the server's `execute` (sent once every member reported `ready` or after 5 s), so receivers hold the key before the first frame under it. | `sr_e2ee::{keys::KeyRing, sequencer}`, `apps/web/src/voice/e2ee/media.ts`, gateway `voice_e2ee.rs` |
| VOX-E2E-005 (ticked) | Identity key: ECDSA P-256 per account per device; the private key is stored only AES-GCM-wrapped by a non-extractable wrapping key, both in IndexedDB (`sr-e2ee` database); loaded as a non-extractable signing key; PKCS#8 bytes only via `unwrapForExport` (for the future MLS client in WASM); a lost wrapping key means a new identity. | `apps/web/src/voice/e2ee/identity.ts` |
| VOX-E2E-008 (ticked) | Interop vectors: `crates/sr-e2ee/tests/vectors.json` written/checked natively, reproduced byte for byte by the WASM build. SFU plaintext assertion: real str0m clients through the SFU core; E2EE frames arrive unchanged, contain no plaintext, fail without the key, decrypt with it. | `crates/sr-e2ee/tests/vectors.rs`, `packages/wasm/test/e2ee.test.ts`, `crates/sr-voice/tests/loopback.rs` |
| VOX-E2E-004 (partial, not ticked) | Op 101 relay: key packages and proposals go to the other members, commits to everyone with the new epoch (first commit per epoch wins, others get `commit_rejected` with the current epoch), welcomes only to their recipients in the channel, `ready`/`execute` for transitions. Only members of the channel, on the session that joined, can send; payloads are opaque base64, at most 96 KiB. State is dropped when the channel empties or the guild goes. Missing: the server as MLS external sender (signed remove proposals) needs openmls. | `crates/sr-gateway/src/voice_e2ee.rs`, tests `voice_e2ee_tests.rs` |
| VOX-E2E-003 (partial, not ticked) | Media worker for `RTCRtpScriptTransform` (`rtctransform` event) and the `createEncodedStreams` fallback (streams posted to the worker), `attachTransform`, `peerConfigFor` (`encodedInsertableStreams`), support detection (`none` means the client must show `voice.e2ee.unsupported`). Missing: the hook in the voice engine (it would drop every frame without MLS keys) and any real-browser run. | `apps/web/src/voice/e2ee/{worker,handler,transform,attach,support}.ts` |
| VOX-E2E-007 (partial, not ticked) | Privacy code (`sr_e2ee::privacy`, also WASM) and the shield popover with per-participant codes and verify toggles kept per identity key in localStorage. Missing: mounting it in the call UI (needs the MLS epoch authenticator). | `lounge/voice/PrivacyCodes.tsx`, `voice/e2ee/verified.ts` |
| WASM | `crates/sr-e2ee-wasm` -> `packages/wasm/dist/e2ee`, facade `loadE2ee()` (`createEncryptor`, `createKeyRing`, `privacyCode`, `isSupportedCodec`). Secrets go in as bytes and stay in WASM memory; the worker zeroes the copies it was posted. | `crates/sr-e2ee-wasm`, `packages/wasm/src/index.{js,d.ts}` |

The SFU needed no change: it forwards frames opaquely and reads only header extensions and the clear codec bytes. `sr-voice` links `sr-e2ee` only as a dev-dependency for the test.

## Frame wire format (v1)

`body (input length, clear ranges unchanged, the rest ciphertext) | tag 8 | instance id 4 | counter ULEB128 1..5 | version 1 | trailer length 1 | magic 0x53 0x52`. AAD = `"sr-e2ee/v1"`, codec id, body length, then each clear range's offset, length and bytes. Key = HKDF-SHA256(salt `"sr-e2ee/v1"`, ikm = sender base secret, info = `"sr-e2ee/v1 media key"` + instance id). Overhead 17 to 21 bytes per frame.

## Deviations and choices (please review)

1. **Truncated tag check.** RustCrypto GCM only accepts 12..16-byte tags. The receiver decrypts with GCM's own CTR keystream (`ctr::Ctr32BE`, from counter block 2), recomputes the full tag by re-encrypting with `aes-gcm`, and compares the first 8 bytes in constant time before releasing plaintext. Stock primitives only; a test pins the keystream to `aes-gcm`'s. Costs a second AES pass on receive.
2. **Encryptor instance id (4 bytes per frame, not in the spec).** A counter that restarts (a recreated worker in the same epoch) would repeat nonces under one key. Each encryptor draws a random non-zero 4-byte id and derives its own key from it, so restarts cannot collide (short of a 2^-32 id collision within one sender's epoch).
3. **One encryptor per sender, shared by all its tracks** (mic, camera, screen) in the one worker per call, so the counter is never duplicated across tracks.
4. **Server-to-client relay is the `E2EE_MLS` dispatch (op 0)**, not raw op 101 frames, so MLS messages are sequenced and replayed on RESUME. Client-to-server is op 101 as documented in docs/05.
5. **ready/execute transition** (modeled on DAVE's prepare/execute) added to the relay; the spec only says senders re-key.
6. **Privacy codes** follow the spec literally (epoch authenticator + identity key + user id), so a participant's code changes every epoch. DAVE instead shows stable per-user codes plus a call code. Owner question below.
7. **Identity key curve P-256 (ECDSA)**, matching DAVE's MLS ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256`. The spec names no ciphersuite.
8. **Leaves:** without an external sender, the remaining members must commit the removal (the relay already stops waiting for a leaver's `ready`).
9. **Per node:** like voice states, the relay's ordering state is in memory on the gateway node.

## Still open (with reasons)

- **VOX-E2E-001** (MLS group, key packages, add/remove, commits, welcome, epoch tracking): needs the `openmls` download approved. Then: an `mls` module in sr-e2ee (one group per call, ciphersuite per the owner's answer, credentials = user id + identity public key signed per session), its WASM exports, the client state machine over op 101, exporter secrets into the worker, and MLS interop vectors.
- **VOX-E2E-003**: engine hook (attach on `addTransceiver`/`ontrack`, `peerConfigFor` in `createPeer`, refuse to join E2EE calls when `detectTransformMode()` is `none`) and a real-browser run.
- **VOX-E2E-004**: external-sender remove proposals (openmls).
- **VOX-E2E-007**: mount the shield in tiles/call controls once codes exist.
- DM calls: the relay works by channel, so DM calls are covered the same way once clients exist.

## Tests run

- `cargo test -p sr-e2ee` (24 incl. vectors), `-p sr-gateway` (165, 5 new for op 101), `-p sr-voice` (all, incl. the new SFU assertion); `cargo clippy -D warnings` on sr-e2ee, sr-e2ee-wasm, sr-gateway, sr-voice, sr-core, sr-server; `cargo fmt --check` on the same. No Postgres suites run (no DB code changed; `voice_pg_tests.rs` only got the new config field, compiled by clippy, not executed).
- `pnpm --filter @sideroom/wasm build` and its vitest with `SR_WASM_REQUIRED=1` (13 tests, 4 new interop tests).
- Web: typecheck, eslint, prettier, vitest `--maxWorkers=2` (2054 tests; one unrelated timeout in `admin/__tests__/p13entrance.test.tsx` under load, passes alone), build and bundle budget (pass; the worker is not referenced by the app yet, so bundles are unchanged).
- Not run: Playwright (brief), any real browser, any MLS.

## Owner questions

1. May I (or the next agent) download `openmls` (and its RustCrypto provider) from crates.io? It is approved in docs/03 but not cached.
2. MLS ciphersuite: `MLS_128_DHKEMP256_AES128GCM_SHA256_P256` as DAVE (assumed here), or X25519/Ed25519 (`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`)?
3. Privacy codes: keep the spec's per-epoch participant code, or DAVE's stable per-user codes plus a per-call code?
