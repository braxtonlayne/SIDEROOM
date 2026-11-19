UNCOMPILED: rust.yml is the first compile. No cargo ran (memory under 2000 MB at 05:19, 05:19, 05:20 and 05:26; see CARGO).

# Wave 114 handoff: verify

## Clock times
Started 05:19 (machine clock). 578/573 cases 05:27, compose cases 05:40, final commit after that.

## Done (commits)
`crates/sr-core/tests/deploy_files.rs` only: four helpers (`shell_fn_body`, `shell_mode_function`, `workflow_job`, `shell_statements`) plus `secret_file_examples`, and nine cases. Merged fix/p114-garage (8e57ee0f), fix/p114-hardened (1b913263), fix/p114-compose (bffbd1bd) with no conflicts. No existing case or helper signature changed.

## Not done
No cargo (rule). Nothing else.

## FINDERS
Only my own file changed; no other file depends on the new names.

## CASE FOR VERIFY
Read from the three handoffs; this wave's cases follow.

## Table of cases
| case | row | fails on the base at | here |
|---|---|---|---|
| backup_sh_refuses_a_data_only_object_backup_and_the_page_names_both_folders | 573 | backup.sh has no die naming SR_BACKUP_OBJECTS_META_DIR (lines 50-62 only test REPOSITORY and OBJECTS_DIR) | uncompiled; by eye matches merged tree (die before `need pg_dump`, line 33) |
| the_garage_folders_the_backup_page_names_are_the_mounted_volumes | 573 | BACKUP.md names no garage-meta / metadata path | uncompiled; prod.yml lines 74-75, toml example, BACKUP.md agree |
| the_storage_workflow_object_backup_step_calls_things_that_exist | 573 | storage.yml has no such step | uncompiled; backup.sh and dev.yml exist, dev.yml mounts both |
| the_compose_server_mounts_the_operator_secrets_folder_read_only_from_outside_deploy | 575 | prod.yml sr-server volumes (no /run/secrets mount) | uncompiled; one mount `${SR_SECRETS_DIR:-/etc/sideroom/secrets}:/run/secrets/sideroom:ro` |
| the_secret_file_examples_in_the_pages_lie_under_the_mounted_folder | 575 | OPERATOR.md line 130 / doc 12 line 55 told the owner to put the value in the file; no _FILE example | uncompiled; all 5 examples under /run/secrets/sideroom |
| the_installer_names_a_compose_minimum_that_prod_yml_needs | 576 | install.sh has no MIN_COMPOSE_* | uncompiled; 2.24, refusal at line 98 before the first secret (line 338) |
| the_smoke_reruns_the_installer_and_greps_for_what_it_prints | 577 | prod-smoke.sh has no rerun_install | uncompiled; both greps equal install.sh lines 189 and 513 |
| the_systemd_job_runs_the_server_unit_as_a_system_service_with_its_hardening_on | 578 | no system-service mode | uncompiled; install -m 644 + cmp -s present, no sed/grep -v on the four keys |
| the_user_service_mode_says_what_it_cannot_prove | 578 | comment sat inside the function, no CANNOT PROVE | uncompiled; comment above stage_user_service has all five strings |

Compose (e) (smoke exports and installer use the same variable names) is not a case; both files set SR_SECRETS_DIR and SR_SERVER_ENV_FILE, read by eye.

## The compile by eye
Helpers exist at the end of the file and are defined before use in the same file; `read`, `compose`, `substitutions`, `root` exist above. Types: `&str == String` comparisons, `strip_prefix(&format!(..))` returning slices of the file text, closures over `&&str`, arrays of `&str` reused (Copy). Every assert has a message. No unused import or variable (no new `use`). rustfmt --edition 2024 --check: clean (the crate edition is 2024). Literals checked with grep against the merged tree.

## CARGO
No cargo. `job.sh status` available memory: 1663 MB (05:19:07), 546 MB (05:19:51), 649 MB (05:20:37), 1000 MB (05:26:28). Under 2000 MB twice, more than five minutes apart.

## FAULTS
None found by reading. Observation (not a fault): prod-smoke.sh `rerun_install` uses a printf format string that spans a line break (`printf '%s` newline `'`); valid shell, prints a newline.

## FOR THE COMMANDER: reading rust.yml
Test binary: `deploy_files` (crate sr-core). Look for the nine names in the table above.
- A 573/575/576/577 failure naming a file text: most likely the tree (a builder reworded a sentence or variable); the message names the file and string.
- `secret_file_examples` failure on a page: a path not under /run/secrets/sideroom is the page's fault; an empty or odd path (for example a `<path>` placeholder) is the case's token scan being too coarse (case's fault).
- `the_installer_names_a_compose_minimum...`: "never refuses" means the die text no longer contains "is too old" (case/wording).
- Systemd case failing on a `sed` statement: look at which key; the tree if it really edits the unit, the case if it is a read-only sed.
- Compile errors or clippy: the case's fault (first compile).

## PROCESSES
strays.sh p114-verify: shows only the shells of the harness; no cargo, rustc or loop.
