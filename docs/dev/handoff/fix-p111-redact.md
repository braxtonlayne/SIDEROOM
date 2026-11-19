# Handoff: fix/p111-redact (D-1033, SEC-AUD-557, 559)

Clock: written 00:44-00:46; cargo window 01:06-01:18 (2026-11-19).

## Done
- `crates/sr-cli/src/main.rs`: `Command` has a hand-written redacting `Debug` (`Bootstrap.owner_password`, `Seed.password` print `Some("<redacted>")`; all else as the derive). Test `tests::command_debug_hides_passwords`.
- Tests (made-up value `TEST-SECRET-NOT-REAL`): `created_token_debug_hides_its_secret` (sr-media service.rs), `ready_debug_hides_its_secrets` (sr-voice-proto wire.rs), `sfu_config_debug_hides_its_secret` (sr-voice config.rs).
- sr-bridge `fake_guild.rs` fake untouched (ruling). No dependency, Cargo.lock, argument or behaviour change.
- Not done: nothing. vitest/eslint etc: not applicable (Rust only).

## CARGO (-j 2, --locked --offline, CARGO_PROFILE_DEV_DEBUG=0, target p110-mounts-target; available memory at acquire 2809 MB)
1. clippy -p sr-media -p sr-voice-proto -p sr-voice -p sr-cli --all-targets -D warnings: 01:06:22-01:09:08, rc 0.
2. test sr-media created_token_debug: 01:09:13-01:09:49, rc 0, 1 passed.
3. test sr-voice-proto ready_debug: rc 0, 1 passed.
4. test sr-voice sfu_config_debug: rc 0, 1 passed.
5. test sr-cli --bin sr-cli command_debug_hides: ended 01:18:17, rc 0, 1 passed.
(Memory was read only at acquire for 2-5 apart from the status line; no command saw trouble.) opusic-sys/cmake did not stop sr-voice or sr-cli here: all four ran.

## Row texts
- 557: closed. Ruling 1: sr-cli `Command` redacting Debug + test. Ruling 2: sr-bridge test fake stays (test support, made-up token).
- 559: closed. Tests for CreatedToken, Ready, SfuConfig added; all pass.
- 558: the sr-sdk, sr-bridge and sr-cli tests of wave 110 compile under clippy; CI is the proof of those runs. (sr-cli's runs here as of this wave.)

## FINDERS
none (no web files). Changelog: sr-cli's Command no longer prints the owner or seed password in Debug output.
JOBS: none. Owner questions: none. PROCESSES: strays.sh output below at commit time.
