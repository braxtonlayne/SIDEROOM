# feat/p34-prf-profiles: PRF-PRO-001 profiles at `/~handle`, new blocks, per-block and per-audience privacy (D-382)

Base `integration/phase34`. Migrations 1680 and 1681. Ports 8081/5174 were not used (no server was started; the
Playwright specs are written or amended, not run).

## Resume here
Nothing blocks. For CI or the integrator: `e2e/tests/noindex.spec.ts` (two samples, runs in prod-smoke against Caddy)
and the web Playwright pass; the Postgres suites below were run locally against a scratch database (dropped).

## Done

**The public surface ships (not deferred).** The extraction was small enough: `packages/profile-blocks` is a new
workspace package (React peer only; no new dependency, `pnpm install --offline --frozen-lockfile` passes with the
lockfile entry added by hand) holding the whole block renderer (`ProfileRenderer`, every pure block, `pageVars`,
the `pp-*` stylesheet). It imports no app module; translation and image addresses come in through `BlocksEnv`, and
the stateful blocks (guestbook form, presence-driven Now watching/playing) are slots the host fills. The app's
`profilepage/ProfileRenderer.tsx` is now a thin wrapper over it (`appSlots` in `profilepage/blocks.tsx`).
- `apps/web/profile.html`, `src/public/profile/{main,router,PublicProfile,messages,cdn}.ts(x)` and
  `src/public/profile/ALLOWED.ts` (the landing list + `@sideroom/profile-blocks` + this folder + its one string
  fragment). The row in `public-surfaces.json` is no longer `reserved` (sample `/~sample`); `vite.public.config.ts`
  passes the list to the guard; `Caddyfile.prod` got its `@profile path /~*` matcher from `render-caddyfile.mjs`.
- `sr-server/src/web.rs`: with `serve_public`, `/~handle` is the profile shell for a visitor without a session and the
  app shell for a signed-in member (the app router has `/~{$username}` as an alias of `/u/$username`). Behind Caddy a
  signed-in member who opens `/~x` gets the public view (Caddy cannot read the session); the public 404 page offers
  "Member access" with `next=/u/<handle>`.
- **Proof of no leak**: `node scripts/build-web.mjs` then `node scripts/check-entrance-isolation.mjs` reports
  `ok landing`, `ok auth`, `ok invite`, `ok profile 113.1/150 KB gzip, 2 files`, legal, error, static. The profile
  surface's own strings (`p34-prf-profiles-core`) are registered by that surface alone (`public/profile/messages.ts`),
  never by the shared first-paint catalog, so the other three surfaces carry none of them. Five reasoned entries were
  added to `scripts/entrance-allow.json` (all `profile` or `static sw.js`: `profile.` keys, `/public/profiles`,
  `Now playing`, `/cdn/`, and the worker naming `profile.html`); none for the three section names.
  `check-bundle-budget.mjs` ok; `node --test scripts/entrance-*.test.mjs` 24 pass; Vitest `public/allowed.test.ts`
  covers the profile surface (imports only its list, never an app directory).
- `platform.public_profiles` (`sr-features/src/public_profiles.rs`, registered in `sr-server/src/app.rs`): platform
  scope, **default off**, not client-visible. `profiles.access` (wave 39) does not exist yet and is not read.

**Backend.**
- Migration 1680 `profile_privacy` (preset, `page_tier`, seven block tiers, `guestbook_accepts_public`); a missing row is
  the Private preset. Migration 1681 adds `status` (`approved|pending`), `anonymous` and `ip_hash` to
  `profile_guestbook` and lets `author_id` be null for a visitor's note.
- `sr-packages/src/profile_privacy.rs` (pure policy: tiers, presets, clamp, `effective(public_possible)`, `visible`,
  `standing`) and `service/privacy.rs` (the service: policy row, rank from the database, `page`, `public_page`,
  `preview_as`, guestbook list/sign/sign_public/approve). A page is built from the blocks the rank admits: hidden
  blocks are dropped from the document first, and the friends, featured servers, pictures and guestbook data are loaded
  only for blocks that remain. Blocked either way = 404 whatever the tiers say; below the page tier = 404.
- New block kinds `linktree` (up to 12 https links), `featured_guilds` (up to 6; shown to a viewer only if they are in
  the server or it is in the directory, and only while the owner is still in it) and `now_playing` (`auto` through
  presence for signed-in viewers, `manual` typed lines for everyone the tier admits). Tier mapping: header, custom,
  favorites, song follow the page; about = bio; top friends and featured servers = Top 8; now watching/playing = now
  playing; gallery = media; guestbook; linktree = links.
- Routes (all in `routes/packages/profiles.rs`, with `handlers.rs` passing the operator switch to the existing
  ones): `GET|PUT /users/@me/profile-privacy`, `GET /users/@me/profile-page/preview?as=public|member|shares_server|friend`,
  `POST /users/@me/guestbook/{entry_id}/approve`, public `GET /public/profiles/{handle}` and
  `POST /public/profiles/{handle}/guestbook` (204, 400 for a link or empty text, 404 for every other refusal with the
  same body, 429 over five per address per hour). Public answers are `Cache-Control: no-store`; the API layer already
  adds `X-Robots-Tag: noindex, nofollow` to every response. Route matrix rows added (class `none` for the two public
  routes, `self` for the others), OpenAPI regenerated, schema and MSW mocks regenerated.
- The existing signed-in routes now go through the filter: `GET /profile-pages/{username}` (also accepts `~name`),
  `GET /users/{id}/profile-page`, `GET|POST /users/{id}/guestbook`. Signing is for the owner or a signed-in viewer who
  can see the guestbook; the per-minute limit and the 500 cap stand.
- DSR: `sr-db/src/data_export.rs` lists `profile_privacy` as its own entry and `guestbook_received` now includes the
  held notes (`status`, `anonymous`); `account_deletion.rs` deletes the policy row and (already) every note on the
  page. (The Split names `sr-admin/src/data_requests.rs`; the export and erasure code actually lives in `sr-db`.)

**Web.**
- `settings/user/profile/{api,PrivacyPanel}.ts(x)` mounted in User Settings > Profile as "Who can see my profile": the
  four presets, an advanced panel (page audience, one select per part, never looser than the page, public options
  disabled while the operator's switch is off, the public-guestbook switch enabled only for a public page and guestbook),
  the sentence "Search engines never index SIDE ROOM profiles.", and the "view as" strip (Public web, Member, Shares a
  server, Friend) rendering `GET .../preview` through the same server filter.
- Editor: three new block kinds with inspectors; the guestbook shows held notes to the owner with an Approve button;
  a note without an author reads "A visitor".
- Strings: `i18n/messages/en/p34-prf-profiles-1.ts` (panel, editor, handbook sentence) and `p34-prf-profiles-core.ts`
  (renderer and public page; registered by the profile surface, and by `en.rest` for the app).
- Handbook privacy section gains one paragraph (`help.handbook.privacy.profiles`).

## Tests run (all passing)
- Also green: `cargo clippy -D warnings --all-targets` on sr-packages, sr-features, sr-db, sr-api, sr-server; `cargo fmt --all --check`; `cargo test -p sr-features -p sr-api` (lib 743, `openapi_names`, `route_matrix`, `api`); `-p sr-db --test secret_columns`; `-p sr-server --test immutable_rules` (13).
- `cargo test -p sr-packages -- --include-ignored --test-threads=1`: 30 unit + 11 db + 3 new in
  `tests/profile_privacy.rs`. The property test walks operator switch on/off x 5 page tiers x 5 block tiers x 5
  viewer ranks (145 visible combinations asserted, the rest asserted as 404): the block set equals the table and every
  seeded string (bio, link URL, track, favourite, custom text, song, gallery caption, a friend's name) is present
  exactly when its block is visible; guestbook entries are 404 or present by the same rule. Also: blocked parties
  see nothing in every preset; the public guestbook (opt-in only, held, invisible to a second fetch, visible after
  approval, no address hash left, five per hour).
- `cargo test -p sr-db --test data_export --test account_deletion -- --include-ignored`: policy and held notes exported,
  erased.
- `cargo test -p sr-api --test route_matrix`; `cargo test -p sr-server --test web_gate` (19);
  `cargo test -p sr-server --test authz_matrix -- --include-ignored --test-threads=1` (7 pass). New test
  `profile_privacy_holds_over_http` runs the actors anonymous, crawler (Googlebot user agent: same body, noindex
  header, 404 with the switch off), member_stranger, member_mutual_guild, friend, blocked_by_owner, blocker_of_owner and
  the owner against both page routes for the four presets plus three custom rows (public page + only_me guestbook;
  friends page + public links = clamped; members page with the switch off stays 404 publicly). The big sweep's seed now
  gives the victim a public page and guestbook with one held note (so `POST /users/{id}/guestbook`, the public
  guestbook and the approve route are proven rightful, and `WRITE_OBSERVED_LEN` fell from 41 to 40: the victim's
  guestbook delete is now proven).
- Web: typecheck (`pnpm -r --if-present typecheck`, includes e2e), eslint on the touched dirs, prettier, Vitest
  (the whole web suite with `--maxWorkers=2`: 363 files; the one failing file, `settings/user/__tests__/profile.test.tsx`, only lacked a handler for the new card and its label "About me" clashed with the panel's, both fixed and the file re-run green), `packages/profile-blocks` 6 tests, `PublicProfile.test.tsx` (4, with axe),
  `PrivacyPanel.test.tsx` (4), `prf-pro.test.tsx` (the new blocks and the `/~bob` alias), build + bundle budget +
  entrance isolation as above.

## Still open
- `media_showcase` (items from the member's own shelf through `scopes::resolve`) is not built: it needs the storage
  scope facts in `sr-packages` and a shelf picker. The `media_tier` governs the Gallery block for now.
- `ring_tier` is stored, clamped and editable, but nothing reads it: prf-rings has not landed the audience function
  (stubbed as "no ring" per the conflict watch).
- The report kind for a pending guestbook note was not added (it lives in `sr-moderation`, which prf-rings is also
  editing); the owner can delete a held note.
- `abuse.public_guestbook_per_hour` is a constant (5) kept in the service and counted from a salted address hash on the
  held rows (reset when a note is approved). Moving it to the Admin > Limits catalog touches `rate_limit.rs` and
  `sr-admin` settings, outside this row.
- `/public/profiles/*` routes live in `routes/packages/profiles.rs`, not a new `routes/public/profiles.rs`, to avoid
  editing `routes/mod.rs`.
- No Playwright run (ports belong to others this wave); the axe check is the Vitest one on the public page and on the
  panel's controls.

## Owner questions
1. **Existing profiles go private.** By D-382 a missing `profile_privacy` row is Private, so after this migration every
   existing customised profile is visible only to its owner until the member picks a preset (the signed-in page and the
   mini popout 404 for others). I did not backfill (any backfill loosens a privacy default). Do you want a one-time
   backfill (for example "Members only" for members who had a customised page), or a notice in the editor?
2. Behind Caddy (no `serve_public`), a signed-in member who opens a `/~handle` link sees the public view or "Nothing
   here" with a Member access link, because only `sr-server` can read the session. Acceptable, or should Caddy proxy
   `/~*` to `sr-server`?
3. Presence-driven "Now playing" is shown only to signed-in viewers; on the public page only the typed (`manual`) line
   can appear. Fine?

## CHANGELOG lines (for the integrator)
- Profiles at `/~handle`: new Links, Featured servers and Now playing blocks, and "Who can see my profile": a preset or
  an audience per part (anyone on the web, members, people who share a server, friends, only me), with a "view as"
  strip. Profiles start private; public pages exist only if the operator turns on `platform.public_profiles`, are never
  indexed, and load only the code the page needs.
- Public guestbook: visitors without an account can leave a plain-text note when the owner opts in; notes wait for the
  owner's approval.
- Operator: new platform feature `platform.public_profiles` (off by default).
