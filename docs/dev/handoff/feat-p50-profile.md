# Handoff: feat/p50-profile (wave 50, PRF-PRO-001)

Branch `feat/p50-profile`, based on main `7ad12261`. Builds all of section 1 of Fable's wave-50 decisions (D-502 to
D-507) in Fable's order: the ring on the page, the guestbook note report, then the showcase. Nothing was left
half-done. Inventory row PRF-PRO-001 is ticked; D-508's "later" items are named in it.

## Done (commits)

- `4e9fa046` Guestbook note report (D-506, D-507). Migration `2340_guestbook_note_reports.sql` (rebuilds the
  `target_kind` checks of `moderation_cases` and `moderation_reports`, as 1710 did; no hold scope).
  `TargetKind::GuestbookNote`; `crates/sr-moderation/src/guestbook.rs` (`visible_note`, `note_snapshot`,
  `Moderation::remove_guestbook_note`). The owner reports any note of their page, an admitted signed-in viewer an
  approved one, the author is refused, everyone else gets the missing-note 404. `private: true`, subject = the
  author. The snapshot is `{kind, id, profile_id, profile_username, author_id, author_username, anonymous, status,
  body, created_at_ms}`, never `ip_hash` (the two usernames are extra, for the case card). Staff
  `DELETE /moderation/guestbook-notes/{note_id}` (`MANAGE_CASES`, access-log `moderation.guestbook_note.delete`
  written first, 404 when gone; optional `?reason=`). sr-moderation now depends on sr-packages (path dependency,
  no new crate): the guestbook audience comes from `sr_packages::service::privacy::guestbook_visible_to`, which
  shares the rank query (`signed_in_standing`) with the page. Web: the kind in `ReportKind`, `TITLE_KEYS`,
  `TARGET_KEYS`; `GuestbookNoteEvidence` card with the Remove button on the case page.
- `a7534945` Ring (D-504, D-505). `PrivacyPolicy::visible_ring`; `effective()` clamps a stored `public` ring tier to
  `members`. `ProfilePage.ring_visible` (absent unless true; never on the public page); the page handlers AND it with
  `sr_userfeat::rings::viewer_may_see_a_ring_of` (`profiles.rs::gate_ring`). `BlockSlots.avatar`; the app wraps the
  header picture in `RingAvatar` (page and popout). The privacy panel offers no "Anyone on the web" for rings and the
  ring hint reads as D-505 (`profile.privacy.ringHint`, text edited in place in `p34-prf-profiles-1.ts`, key kept);
  the media field reads "Pictures and showcase". No 128 px ring step: the header picture is drawn at 40 px, where the
  existing 2 px ring already fits.
- `231fc153` Showcase (D-502, D-503). `BlockKind::MediaShowcase` (13 kinds; singleton; `media_tier`),
  `ShowcaseItem` validation (6 items, hash, shelf id, 80-char one-line caption, server-filled fields refused).
  Save checks every item against the saver's own active mount and its catalog (track/movie/episode), refusing with
  the field path. `service/showcase.rs` resolves items per view (title: owner title, track tag, file name; length
  from `music_tracks`), skipping gone, delisted (catalog flag and takedown register) and non-media objects; other
  viewers' copy of the block names only what can play. `PrivacyPolicy::served` drops the block for rank `public`
  whatever the tier. Mint `GET /profile-pages/{username}/showcase/{object_hash}/url` (`SessionOnly`, `no-store`,
  page 404 for every refusal, mount re-checked: owner, active, CORS) through `shelf_access::presign_object_url`,
  the presign core now shared with `shelf_object_url`. Web: host slot only (the package draws nothing without it;
  the block is found by type so the public bundle names nothing); `showcase.tsx` (Play mints on click,
  `controlsList="nodownload"`, one item at a time); `ShowcaseInspector.tsx` (picker with `ownOnly`, captions,
  up/down, remove); `ShelfPicker` gained `ownOnly`, `title`, `description`. Route matrix rows, authz matrix seeds
  and overlays, entrance denylist tokens (`media_showcase`, `showcase`, `presign`, `status_ring`, `RingAvatar`).
- `05799cb9` D-502..D-508 into the decisions log; T53 into the threat model (names verified by the test).
- `673bf890` OpenAPI, generated client and MSW handlers regenerated; fixes to the two new Postgres tests.
- Last commit: visual-check fixes and docs (below).

## Visual check

Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-profile` (`profile-u`, `profile-tilde-app`,
`profile-member`, `profile-page-edit`, `pub-profile`, `pub-profile-missing`, `settings-user-profile`; desktop and
phone; Lounge Light, Editorial; a Lounge Dark pass of `profile-u`, `profile-page-edit` and
`settings-user-profile` overwrote the `-dark` files). The sweep seed now gives the Owner a header, a guestbook, a
live text ring, a public page whose guestbook took one visitor note (held) and one member note; the showcase is
added by `decorate` (memory storage cannot be a member bucket), as the Study's shelves are.

Fixed after looking:
- Guestbook entries on a phone (two-column grid, 2 or 3 buttons): the text column was one character wide. Entry
  rows now wrap their buttons under the text (`profile-blocks.css`). Before: first `profile-u-phone-dark.png` run.
- Report and Remove were ghost buttons, nearly invisible on the profile palette; now `secondary` like Approve.
- Showcase titles were cut ("Blue Ho…") on a phone; they wrap now.
- The public page's own note form (first shown by the sweep now that the seeded page is public) had a tiny
  textarea and an unstyled "Send note"; styled in `profile-blocks.css` (`.pp-sign > label > textarea`, button).

Flows clicked (Browser pane, own stack): opened the ring from the header picture (viewer opens); reported the held
note (dialog "Report this guestbook note", sent); approved it; opened the case in Admin > Moderation (label
"Guestbook note", evidence card: visitor, waiting for approval, text, page) and pressed Remove note (the note is
gone); editor: added a Showcase block, opened "Add from my shelf" (own-shelves picker, empty state). Play was
checked in vitest and the decorated shots (the stack has no bucket).

Flags left: `profile-member` 404 (the member's page is private, unchanged from main); `pub-profile-missing` 404 is
the point of that row; `settings-user-profile` phone: a "▾" control at x=353..385 is reported off-screen (the
section dropdown; not touched by this branch). Seen but not mine: in the report dialog the reason list opens partly
under the dialog's lower edge (shared `Modal` + `Select`); keyboard choice works.

## Tests run

- `cargo clippy -j 4 -p sr-packages -p sr-moderation -p sr-api -p sr-server --all-targets -- -D warnings`: clean.
  `cargo fmt --all -- --check`: clean.
- `cargo test -p sr-packages -- --include-ignored`: lib 36, `profile_privacy` 4 (tier x rank x block incl. the
  showcase and the ring column; the save-time shelf check), others green.
- `cargo test -p sr-moderation --test guestbook_note --test rings -- --include-ignored`: 3 + 1; `--lib` 40.
- `cargo test -p sr-server --lib profile_pg_tests -- --include-ignored`: 2 (`a_showcase_mint_is_the_owners_grant_
  and_dies_with_the_object`, `the_page_ring_needs_the_profile_tier_and_the_rings_own_audience`).
- `cargo test -p sr-api --test route_matrix`: 6. `-p sr-api --lib packages`: green.
- Not run: the full authorization matrix (`crates/sr-server/tests/authz_matrix.rs`; seeds and overlays added for
  both new routes) and `immutable_rules.rs`: the integrator's run.
- Web: `tsc --noEmit` clean; vitest `src/profilepage src/settings/user/profile src/moderation
  src/admin/__tests__/moderation.test.tsx src/activities src/lounge/rings src/i18n` 31 files / 253 tests;
  `packages/profile-blocks` 21; eslint and prettier on changed files; `check-css-tokens`, `i18n-audit` (0),
  `check-doc-links`, `threat-model.test.mjs` (5/5); `pnpm --filter @sideroom/web build`,
  `check-entrance-isolation` ("every public surface is clean"), `check-bundle-budget` all ok;
  `mocks:check` up to date; `playwright test --list` loads 903 tests in 67 files.
- New e2e: `moderation-gate.spec.ts` step "PRF-PRO-001 (D-506, D-507)" (written, not run here; it needs the gate's
  full stack).

## Note for the commander: the shared cargo target can build another worktree's code

With `CARGO_TARGET_DIR=fu-target` shared by six worktrees, cargo twice used a stale `sr-packages` (errors about
items that exist in this tree). Cargo's metadata for workspace members is path-relative, so the same crate in two
worktrees shares one artifact, and freshness is judged by mtimes of whichever tree built it last. Everything above
was built and tested in a private target (`C:/Users/braxt/Desktop/SIDEROOM_wt/p50-profile-run/target`, about 10 GB;
delete when done). Results from the shared target this wave may not belong to the tree that ran them.

## Owner questions (for Fable)

1. The staff removal takes an optional `reason` (written to the access log); should it be required like the held
   ring read?
2. The showcase is not drawn in the compact popout (it shows at most five blocks and no stateful ones); fine?
3. A stored `public` ring tier is shown in the panel as "Members of this server" (what is in force); fine, or
   rewrite the row to `members` on the next save?
4. The case card's Remove stays after removal (the snapshot is the evidence); a second press says the note is gone.
   Hide it once the note is gone (needs a lookup per card)?
