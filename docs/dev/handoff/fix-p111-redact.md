# Handoff: fix/p111-redact (D-1033, SEC-AUD-557, 559)

Status: code written, nothing compiled yet (waiting for the commander's "go" for the cargo window).

Done: `Command` in `crates/sr-cli/src/main.rs` has a hand-written redacting `Debug` (`Bootstrap.owner_password` and `Seed.password` print as `<redacted>`; all other variants print as the derive did) and test `command_debug_hides_passwords`. Tests added for `CreatedToken` (sr-media `service.rs`), `Ready` (sr-voice-proto `wire.rs`), `SfuConfig` (sr-voice `config.rs`), all with the made-up value `TEST-SECRET-NOT-REAL`. The sr-bridge `fake_guild.rs` fake is untouched (ruling).
