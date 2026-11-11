# Wave 58 texts: D-593, D-594

## Done
- **SEC-AUD-063 (D-593)** (`crates/sr-activities/src/arcade.rs`, `crates/sr-server/src/arcade_jobs.rs`):
  - The 24-hour shelf alert prints the delist day as `YYYY-MM-DD UTC`.
  - The aged-delisting reason quotes the last answer without its "Try again later." tail (and its stop): "(last answer: its file could not be read from its shelf)." This is the same text in the DM, `delist_reason`, audit reason and staff alert.
  - The aged lister DM ends "List it again once the shelf is answering again."; the aged staff alert ends "The lister can list it again once the shelf is answering again." Other delistings keep their text.
  - Tests: `an_aged_delisting_says_list_it_again_once_the_shelf_answers` (new unit test), `a_cartridge_deferred_for_seven_days_is_delisted_with_dm_audit_and_alert` (reason, DM, alert tail, no "Try again later") and `the_24_hour_alert_names_the_delist_day` (`... UTC`).
- **SEC-AUD-064 (D-594)**: `admin.reviewHub.tabApps` is "App queue" and `admin.reviewHub.tabApplications` is "All applications". The tabs stay side by side in that order. No other heading or empty state repeated the old labels.
  - Updated: `reviewHub.test.tsx` (now also asserts the tab order), `p10.test.tsx`, `p57apps.test.tsx`, `e2e/tests/admin-apps.spec.ts`, and the `page-sweep.spec.ts` rows that open the tab.
  - Coordinator note: `6ebdb6ae` (p10 test expecting the Applications tab) conflicted with my rename. My version of that test is the superset (`['Cinema','App queue','All applications']`), so I skipped the cherry-pick. A merge of that commit will conflict in `p10.test.tsx`; take this branch's side.
- Inventory rows SEC-AUD-063 and SEC-AUD-064 added after SEC-AUD-060.

## Not done
Nothing outstanding.

## Visual check
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p58-texts` (rows `admin-review` and `admin-review-applications`; desktop and phone, Editorial light and Lounge dark). I looked at them:
- Desktop: all six tabs fit on one line.
- Phone: the tab strip scrolls horizontally as before. Labels do not wrap, and "All applications" is partly off-screen on the Cinema view but fully shown when it is the active tab. This is the existing scroll behaviour.

## Tests run
- `cargo test -p sr-server --lib arcade_jobs -- --include-ignored` (11/11, scratch DB `sr_p58_texts`).
- vitest on `reviewHub`, `p10` and `p57apps`.
- Prettier and eslint on the changed web files.
- `rustfmt`, `cargo fmt --all -- --check` and clippy on `sr-activities` and `sr-server`.
- `i18n-audit`, `check-css-tokens`.
- `playwright test --list` loads 967 tests.

## Owner questions
None.

## Proposed threat-model rows
None (text-only changes).
