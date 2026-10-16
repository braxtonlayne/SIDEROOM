# Handoff: feat/p14-e2ee (voice and video end-to-end encryption, VOX-E2E-001..008)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p14-e2ee, branch feat/p14-e2ee (base integration/phase14 = main). Local commits only, nothing pushed. Scope: `docs/inventory/08-lounge-voice.md` VOX-E2E-001..008. No migrations (nothing is stored server side). The owner approved `openmls` mid-pass (commit 713a241, D-249, D-250); the second half of this branch builds on it.

## State in one paragraph

The whole E2EE path is built and tested piece by piece and across pieces: MLS on `openmls` (native and WASM), the gateway as MLS delivery service and external sender, the frame transform, the browser MLS session, the media worker attached by the engine to every sender and receiver, identity keys and the shield with D-250 codes. **Both flags stay off** (`voice.e2ee` on the server, `VITE_SR_VOICE_E2EE` in the web build). D-249 says to turn them on once MLS lands, and the coordinator added "only if the full path works end to end in tests". Every part has tests except one: the encoded-frame transform has never run inside a real browser's WebRTC stack. That covers RTCRtpScriptTransform or createEncodedStreams with real Opus/VP8/H.264 packetizers, the Vite-built module worker loading the 2 MB WASM, and real DTLS media through the SFU. If that part were wrong, turning E2EE on by default would make every call silent, because the transform fails closed. So I left it off. One Playwright run (two browsers, both flags on, audio and video flowing, same call code) on CI is the remaining step; then flip both defaults (`crates/sr-core/src/config.rs` `e2ee: false`, `apps/web/src/voice/e2ee/flag.ts`).

## Done

| Item | What | Where |
|---|---|---|
| VOX-E2E-001 (ticked) | MLS per call, ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256` (D-249), PublicMessage handshakes. Per-session P-256 signature key, certified by the account identity key: the identity key signs `"sr-e2ee/v1 session key" ‖ user id ‖ session key`, and the basic credential carries `1 ‖ user id ‖ identity key (65 B) ‖ signature (64 B)`. Every member verifies every credential. Key packages; commits whose adds and removes are checked against the voice-channel roster (adds only for users in the channel, removes only for users who left, one leaf per user, a reconnect replaces the old leaf, no update proposals); welcomes with the ratchet tree; external Remove proposals; epoch tracking; the per-sender secret `MLS-Exporter("SideRoom Secure Frames v1", user id u64 LE, 16)`. Compiles for wasm32 (openmls `js` feature, getrandom `js`/`wasm_js` in sr-e2ee-wasm only). | `crates/sr-e2ee/src/mls.rs`, `crates/sr-e2ee-wasm/src/mls.rs` |
| VOX-E2E-002 (ticked) | Frame encryptor/decryptor: AES-128-GCM, counter nonce, 8-byte tag, codec-aware clear bytes (as in the first half of this pass). | `crates/sr-e2ee/src/{frame,codec,keys}.rs` |
| VOX-E2E-004 (ticked) | Op 101: `hello` gets a `call` reply (random group id, the server's external-sender key, `create` for exactly one member; a group that lost all its members restarts). Opaque relay; first commit per epoch wins; welcomes go only to their recipients; `ready` carries the member's leaf, and `execute` is sent when all are ready or after 5 s. **Server as external sender:** when a member leaves, the gateway signs a Remove proposal for their leaf, and the remaining members' MLS accepts it and commits it. The external-sender key is per gateway node, in memory, made on first use. | `crates/sr-gateway/src/voice_e2ee.rs`, tests `voice_e2ee_tests.rs` (incl. real MLS clients from creation to an external remove) |
| VOX-E2E-005 (ticked) | Identity key (ECDSA P-256, wrapped by a non-extractable key in IndexedDB); now also exposes the raw 65-byte public key and `signWithIdentity` for the MLS certificate. | `apps/web/src/voice/e2ee/identity.ts` |
| VOX-E2E-006 (ticked) | Prepare on commit, switch on `execute`, 10 s receive grace. | `media.ts`, `KeyRing`, gateway |
| VOX-E2E-007 (ticked) | D-250: pairwise verification code (both identity keys and user ids, the same on both sides, stable until a key changes) and the call code (epoch authenticator), shown in the mounted shield (`E2eeShield` in the call controls and the voice panel, only while the call is E2EE). Verify toggles are kept per identity key. | `crates/sr-e2ee/src/privacy.rs`, `lounge/voice/PrivacyCodes.tsx` |
| VOX-E2E-008 (ticked) | Shared vectors (frames, session certificate message, both codes) native and WASM; SFU plaintext assertion. | `crates/sr-e2ee/tests/vectors.*`, `packages/wasm/test/e2ee.test.ts`, `crates/sr-voice/tests/loopback.rs` |
| VOX-E2E-003 (partial, not ticked) | Wired: the controller starts the call's MLS session (`E2eeCall`) before the engine. The engine attaches the worker to the mic, camera and screen senders and every receiver (`ConnectParams.e2ee`), with `encodedInsertableStreams` for the fallback. Video codec comes from frame metadata, or else from the clear bytes (Annex B start code means H.264, otherwise VP8). No encoded-frame API, or a failed start, refuses the join (`voice.e2ee.unsupported`, `voice.e2ee.failed`); a call is never joined in the clear. Missing: a real-browser run. | `apps/web/src/voice/e2ee/{call,runtime,worker,handler,transform,attach,store}.ts`, `voice/engine.ts`, `voice/controller.ts`, `vite.config.ts` (`worker.format: 'es'`) |

**Browser MLS session (`E2eeCall`).**
- The member with the lowest leaf in the call commits at once; the others commit after 3 s if the change is still pending.
- Losing a race drops our commit; the winning commit or a welcome follows.
- If MLS refuses a commit (someone not in the call added, someone still in it removed), the client goes to `error`, drops its group copy and sends a new key package, so it gets re-added the same way as a reconnect.
- Secrets leave WASM only to be posted to the worker, which zeroes them after installing.

## Deviations and choices (D-249 accepted the first-pass ones)

1. Handshake messages are MLS PublicMessages (signed, not encrypted), so the server's external Remove proposals can be processed. They hold only public keys and HPKE-encrypted path secrets; DAVE does the same.
2. Group creation goes through the server (`hello`/`call`: one creator), and members report their leaf with `ready` so the server can name it in a Remove proposal. A member lying about its leaf can only get a wrong leaf proposed, and honest members refuse a removal of anyone still in the call.
3. When the external proposal is stale (a commit raced it), the members' own commit removes the leaver anyway. Both paths stay in the code.
4. The main thread holds the MLS client, not the worker; per-epoch sender secrets are posted to the worker.
5. The WASM is about 2 MB (730 KB gzip) and loads lazily, only when E2EE is on. It is outside the route budgets, which all still pass.

## Tests run

- Rust:
  - `cargo test`: sr-e2ee (29 + vectors, incl. 5 MLS tests), sr-gateway (all 167, incl. 2 with real MLS through the relay), sr-voice loopback.
  - `cargo clippy -D warnings` on sr-e2ee, sr-e2ee-wasm, sr-gateway, sr-server.
  - `cargo fmt --all --check`.
  - `cargo check -p sr-e2ee-wasm --target wasm32-unknown-unknown`.
  - No Postgres suites: no DB code changed.
- WASM: `pnpm --filter @sideroom/wasm build`; vitest with `SR_WASM_REQUIRED=1` (14 tests, incl. MLS run inside WASM).
- Web: typecheck, eslint, prettier, vitest `--maxWorkers=2` (2063 tests, all pass), build and bundle budget (pass).
  - New: `voice/__tests__/e2eeCall.test.ts` runs three clients end to end: MLS in WASM, a relay with the gateway's rules, real media keys, frames checked across members, the leader leaving, a refused malicious add.
  - Also new or extended: `engineE2ee.test.ts`, `controller.test.ts` (E2EE section), `privacyCodesUi.test.tsx`.
- Not run: Playwright and any real browser. The brief forbids Playwright here because of shared ports; that run is what the flags wait for.

## Still open

- **The real-browser run, then flip the defaults** (see above). Also check Safari's RTCRtpScriptTransform, and how the PWA precache handles the 2 MB WASM.
- **Bots and apps in voice** must speak MLS through the SDK (spec 5.6, DEV-BOT-007). Until they do, they cannot hear or be heard in an E2EE call once it is on by default.
- The gateway's ordering and external-sender state is per node, like voice states (D-249).

## Owner questions

None open. D-249 and D-250 answered the earlier three. One heads-up: turning E2EE on by default will lock voice bots out until the SDK speaks MLS.
