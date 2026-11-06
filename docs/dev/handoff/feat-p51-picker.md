# Wave 51 handoff: picker (ACT-SDK-001 shelf search, watch-party host rejoin)

Branch `feat/p51-picker`, worktree `SIDEROOM_wt/p51-picker`. Decisions D-545 (search) and D-546 (rejoin). No migration.

## Done
1. **Shelf picker search.** `GET /shelves/{shelf_id}/objects?q=` (`crates/sr-api/.../shelf_access.rs`, store `catalog_page(.., name, ..)` in `sr-storage`/`sr-db`). `q` is trimmed, blank = plain listing, over 100 characters = 400 `SEARCH_TOO_LONG`. It is one extra `AND` (path or owner title `ILIKE`, wildcards escaped) in the same SQL as the plain listing, so listed / not delisted / no upheld takedown and the per-object kind and delisting checks are unchanged; same order, same `object_hash` cursor, same route bucket. No index needed (already cut to one shelf by `shelf_objects_listing_idx`). `ShelfPicker.tsx`: the box queries the server 300 ms after typing stops (rows still filter instantly meanwhile), the shelf change resets the search, a search that runs past 15 pages has its own note, the 3,000-file note stays for the unfiltered list. Strings in `p51-picker-1.ts`.
2. **Host rejoin.** The 404 was the empty *private* party: the lapsed host was not a member, a private party is invisible to non-members, and rejoin wanted the invite code. `sr-watchparty/src/service.rs`: the recorded host (`host_id`; it only stays on a user who is not a member when the party emptied) may view and join a private party without the code. Ban check stays first; seat is taken only by the first person into an empty party (existing rule); a former host whose seat moved on needs the code again. Web: the "removed" screen of the party page has a "Rejoin the party" button (`WatchPartyPage.tsx`).
3. Docs: D-545, D-546, inventory ACT-SDK-001 text.

## Threat model
No T56 row: same route, same query, same visibility filter; `q` only removes rows, is length-capped and bound as a parameter (wildcards escaped). The rejoin adds no surface beyond the host regaining their own party.

## Not verified yet (honest status)
The heavy-work lock was held by `sound` for the whole session; the Rust build, clippy, the Postgres tests, OpenAPI regeneration and the visual check have NOT run. `SIDEROOM_wt/p51-picker-round1.sh` is queued (acquires the lock, runs clippy + the three Rust test targets + openapi into `SIDEROOM_wt/p51-picker-openapi.json`, releases). Web typecheck and the 4 new vitest cases pass.

## Tests
- Rust: `sr-db --test shelf_access` (new `the_catalog_search_narrows_the_same_listing`: case-insensitive, wildcards literal, delisted/unlisted never match, subset of plain listing, pagination); `sr-watchparty` db test `a_lapsed_host_rejoins_their_empty_party_...` (lapse, rejoin, seat only when free, stranger refused); `authz_matrix shelf_access_holds_over_http` extended (search by name/title/miss, too long 400, stranger 404, delisted never matches). The route matrix rows are unchanged (path/auth same).
- vitest: `shelfPicker.test.tsx` (+2), `controller.test.ts` (+2).

## Owner questions
- D-545: should the search also match the owner's title (done, because the picker shows it) or file names only?
- D-546: a former host whose seat moved on while they were away still needs the invite code to a private party. Should every past member be able to return? That needs a stored member history (a migration).
- Should the party page rejoin automatically after a lapse instead of showing the button? (Not done: it would fight a deliberate leave from another device.)
