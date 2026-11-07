# Wave 51 handoff: picker (ACT-SDK-001 shelf search, watch-party host rejoin)

Branch `feat/p51-picker`. Decisions D-545, D-546 (builder) and D-548..D-550 (Fable's answers). No migration, no threat-model row.

## Done
1. **Shelf picker search (D-545).** `GET /shelves/{shelf_id}/objects?q=` (`sr-api` `shelf_access.rs`; `catalog_page(.., name, ..)` in `sr-storage`/`sr-db`). `q` is trimmed; blank = plain listing; over 100 characters = 400 `SEARCH_TOO_LONG`. One extra `AND` (path or owner title `ILIKE`, wildcards escaped, bound parameter) in the same SQL as the plain listing, so the visibility filter, order, `object_hash` cursor, auth and rate bucket are unchanged. No index needed (`shelf_objects_listing_idx` already cuts to one shelf). `ShelfPicker.tsx`: server query 300 ms after typing stops (rows filter instantly meanwhile), a shelf change resets the search, a search past 15 pages has its own note, the 3,000-file note stays for the unfiltered list. OpenAPI, generated client and MSW handlers regenerated (`mocks:check` clean).
2. **Host rejoin (D-546, D-549).** The 404 was the empty *private* party: the lapsed host is not a member and rejoin wanted the invite code. `sr-watchparty/src/service.rs`: the recorded host of a private party may view and join it without the code (ban check first; the seat is taken only by the first person into an empty party). Web: the "removed" screen of the party page has a "Rejoin the party" button; `controller.open` falls back to the invite code the tab was shown (D-549), so a former host whose seat moved on comes back too; a tab that never saw the code still gets the 404.
3. Docs: decisions D-545, D-546, D-548..D-550; inventory ACT-SDK-001 text.

## Not done
- The visual check of the picker **inside an Activity (Sketch)**: it needs the guest relay and voice stack (`sketch-gate.spec.ts`). It is the same `ShelfPicker` component with the Activity title text; the profile showcase editor shots show the search and the dialog in all themes. Not looked at in an Activity.

## Threat model
No T56: same route, same query, same filter; `q` only removes rows, is length-capped and parameter-bound. The rejoin gives the host back their own party.

## Tests run
- Rust (own target dir, scratch DB `sr_p51_picker`, since dropped): `cargo clippy -p sr-storage -p sr-db -p sr-api -p sr-watchparty --all-targets -D warnings` clean; `sr-db --test shelf_access` 5 passed (new `the_catalog_search_narrows_the_same_listing`); `sr-watchparty` 32 passed (new `a_lapsed_host_rejoins_their_empty_party_and_retakes_the_seat_only_when_free`); `sr-server --test authz_matrix shelf_access_holds_over_http` passed (extended: search by name/title/miss, too-long 400, stranger 404, delisted never matches); `cargo fmt` applied.
- vitest: `shelfPicker.test.tsx` (9), `controller.test.ts` (38; +4 for rejoin and D-549 incl. the 404 for a tab that never saw the code); web `tsc` clean; eslint and prettier clean on the changed files; `check-css-tokens` and `i18n-audit` clean.
- Playwright `--list` loads all 924 tests (70 files).

## Visual check
Shots in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p51-picker/` (all looked at in a sample across themes and sizes; the party page's frame is the same dark theatre in all three themes by design): `{lounge-editorial,lounge-light,lounge-dark}-{desktop,phone}-{1-picker,2-search,3-nomatch}.png` (profile showcase editor, search against a mocked shelf), and `party-<theme>-<desktop|phone>-{1-in,2-lapsed,3-rejoined}.png` (party page after a lapse, the Rejoin button, back in). The spec is `e2e/tests/p51-picker-shots.spec.ts` (off unless `SR_P51_SHOTS=<folder>`); the shelf and party are played by routes in the page. The first look showed the two buttons on the "removed" screen touching; fixed with `.wp-msg-actions` (flex gap) and re-shot.

## Owner questions: answered
1. Search matches path or owner title: D-548 (no change).
2. Former members of a private party: D-549 (the page remembers the code; no member history, no migration).
3. Auto-rejoin after a lapse: D-550 (keep the button).
