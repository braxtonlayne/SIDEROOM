# Handoff: feat/p39-ops-immutable (OPS-IMM-003)

Base: origin/integration/phase37 (a7d10bf0). Migrations: none. No Playwright, no server ports, no downloads.

## Done

- `crates/sr-server/tests/immutable_rules.rs`
  - **Name test over the full catalog.** `all_flag_names_of(registry)` walks the registry (feature ids, schema keys), the settings catalog, the config fields and, new, the names of the phase's new crates: storage kinds (incl. the emulator's `games`, exhaustive match so a new kind will not compile until listed), storage grants (`view_games`), activity bundle capabilities (`sr-activities`), bridge module ids (`sr-bridge` ids via `sr-features`). Same deny pattern (it is pinned and unchanged).
  - **Deliberate violation proof.** `TestOnlyE2eeSwitch` (`voice.e2ee_test`) is registered on top of the real registry in the test build only; `a_test_only_voice_e2ee_test_feature_is_caught_by_the_name_test` is `#[should_panic(expected = ...)]` on the name test's own body (`assert_names_clean`), and `the_deliberate_violation_is_the_only_hit` shows it is the single hit.
  - **Registry catalog test** `the_registry_catalog_is_well_formed_and_every_feature_can_be_killed`: every registered feature has name, description, scope, a platform scope (or a `gated_by` parent with one) so the operator can kill it, owner-only implies guild scope, `gated_by` targets exist. Walks the registry: wave-39 additions are covered at merge.
  - **Third state.** `Flip::KillAll` (every guild module off in both seeded servers via `GET/PATCH /guilds/{id}/sr/features`, then every feature killed) added to `refusals_and_logs_hold_in_every_flag_state` (now 4 states). The harness reads the state back (nothing still enabled).
  - **Nightly test** `the_full_matrix_refuses_the_same_with_the_flags_killed` (ignored, and returns at once unless `SR_SECURITY_NIGHTLY=1`, so `rust.yml`'s ignored suite does not pay for it): full matrix (refusals, reads, writes, all six groups) under `Normal`, then under `kill_all` (and `kill_features` via `SR_IMMUTABLE_STATES=normal,kill_features,kill_all`); every Deny call keyed `group|METHOD path|actor|pass` must be refused in both (`compare_refusals`, unit-tested incl. a deliberate admitted-in-one case). Admission failures of killed features are dropped under the kill states (a killed feature is not admitted); refusal-side failures are kept.
- `authz_matrix.rs` harness: `Flip::Normal`, `Flip::KillAll`, `run_full`, `Report.denials`, `group_count`; the kill logic split into `kill_features` / `guild_modules_off` (retries the per-account 429 of the toggle).
- `.github/workflows/security-nightly.yml`: weekly (Mon 03:23 UTC) + dispatch (inputs `states`, `groups`), Postgres service, standard runner, names job, full matrix compare, the refusals suite, step summary, log artifacts. Parsed with js-yaml (ok).
- Docs: threat-model T28 row extended to name the job and the new proofs; one AGENTS.md line in section 4.

## Results (own scratch DB sr_p39_immutable)

- Name/catalog tests (no DB): 14 passed.
- `refusals_and_logs_hold_in_every_flag_state`: green 4 times (4 states x 3 groups, ~4.5 min).
- Full matrix, normal vs kill_features vs kill_all, all six groups: green; 15872 refusals compared, 0 disagreements, 106/108 moved status but remain refusals (403 -> 404 etc.).
- fmt --check and clippy -D warnings on sr-server: clean. authz_matrix (7 passed, ignored included) and web_gate: green.

## Flake investigation

The wave-36 integrator's "2 failures in the admin group under KillFeatures" did NOT reproduce: 4 full runs of `refusals_and_logs_hold_in_every_flag_state` and 1 full-matrix run, all green. Likely load sensitivity (the machine was running several builds; the toggles and feature kills hit per-account rate limits: I did see 429s on the guild module toggle and fixed that in my own new code with a retry). Nothing changed for the original; if it recurs, capture the `failures` list printed by the test.

## Hard-coded ids (everything else comes from the registry)

`known_at_base()` in immutable_rules.rs: `platform.byos`, `platform.public_profiles`, `platform.bridge`, `bridge.enabled`, `platform.compliance` (constants) and `lounge.scrapbook` (string). Plus `storage kind games`, `storage grant view_games`, `activity capability datachannel` in `the_names_of_the_new_crates_are_scanned`, and `retention.large_file_days` (existing probe in the matrix harness). No id of ops-modules, gsv-page or ops-invites is named: their features are covered through the registry/lists when merged.

## Still open / for the integrator

- "The nightly's first run on CI": dispatch `security-nightly.yml` on the integration branch (cannot be done from here).
- After merging ops-modules / gsv-page / ops-invites, rerun `immutable_rules` (name test over their ids, killed-state matrix).
- No owner questions.
