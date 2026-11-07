# Wave 55 webui builder: handoff (SEC-AUD-046, SEC-AUD-047)

Branch `fix/p55-webui` (based on `integration/phase54`). Nothing pushed. No migrations. One small Rust change
(the `sr-theme` token registry, see below).

## Done
- **SEC-AUD-046 (D-577)** (`74da0e45`; spec `769a06c9`; test `e0636818`; fixes from the shots `c2d1f3f5`,
  `71f08d62`):
  - A server's channel list or member list that failed to load no longer raises a toast. The list shows "The
    channels could not be loaded." / "The member list could not be loaded." with Try again where the list is
    (`lounge/ListLoadFailed.tsx`), and the open channel says "This channel could not be loaded." instead of "doesn't
    exist"; Server Settings shows "This server could not be loaded." with Try again instead
    of loading forever. A failure is remembered per server (`lounge/loadFailures.ts`), so it is not retried in a
    loop; Try again and the browser's `online` event load it again. A failure is "failed", not "No channels yet".
  - Soundboard favourites sync: a failed save keeps the change, marks it unsynced (`favoritesUnsynced`), shows one
    quiet line in the soundboard panel ("Favorites are not saved to your account yet.", no alert role), and sends
    it again on the next change and on `online`. While a change waits, an account copy arriving from elsewhere
    does not overwrite it. No toast. The three wave 54 toast strings (`loadServer`, `loadMembers`,
    `soundFavorites`) are removed.
  - `--ui-danger-text` (danger text on a surface) in every theme: Lounge Dark `#fa777c` (Onyx inherits), Lounge Ash
    `#ff8a8d`, Lounge Light `#c22a31`, Editorial and Side Room `#d97a68` (Broadcast inherits). 4.5:1 or better on
    `--ui-bg`, `--ui-bg-raised`, `--ui-bg-overlay` and the Lounge surfaces, in the normal and the high contrast
    setting (`packages/ui/src/theme/tokens-contrast.test.ts`, new case). The token is in the `sr-theme` registry
    (`registry.rs`, so `registry_covers_every_ui_kit_token` holds), the validator warns below 4.5:1 on `--ui-bg`
    (`validate.rs` pair) and `tokens.generated.ts` lists it.
  - The entrance palette gets `--sr-entrance-danger-text: #d97a68` (scoped to `.ent-root` like
    `--sr-entrance-text-muted`; 5.6:1 or better on `--sr-bg-0/1/2`), used by the refused-invite text.
  - The kit's input error text uses `var(--ui-danger-text, var(--ui-danger))`; the four `[data-theme=...]`
    colour-mix overrides are gone, so the kit CSS that public pages load names no theme any more (D-385).
    Editorial had no override before and showed its danger fill colour (3.2:1) as text; fixed by the token.
  - `.lng-failed` (every Lounge failure line, including the new rows) uses the token instead of `--danger`
    (`#da373c` was 2.8:1 on Lounge Dark).
  - docs/10: the danger colour rule (fill, ink on fill, text on surface) and the entrance token row.
- **SEC-AUD-047 (D-578)** (`e4e81fa9`):
  - `voice/e2ee/pins.ts`: a set of identity keys per viewer and person (cap 8, oldest dropped; the wave 54 single
    pin reads as a set of one). First key: pinned silently. Key in the set: silent (a phone and a laptop no longer
    flip-flop the notice). Key not in the set: reported once, joins the set, its verified mark is cleared. Marking
    the person verified pins the key again as the newest (so the cap keeps verified devices).
  - `voice/e2ee/runtime.ts`: key checks run one at a time (two rosters can no longer both read a set before either
    wrote it) and every reported person stays on the notice for the rest of the call (the key is in the set after
    the first check, so a later roster would otherwise clear the notice at once).
  - `voice/e2ee/verified.ts`: the verified mark is per person and key (a list per person, cap 8; a wave 54 single
    mark still reads). Verifying the laptop does not vouch for the phone; unmarking one key leaves the others.
  - Shield copy: "End-to-end encryption: a new privacy code", "X has a new privacy code (a new device, or a new
    sign-in).", "Compare the new code with them, then mark them verified." (`p54-webb-1.ts` replaced by
    `p55-webui-1.ts`).
  - Spec `docs/specs/lounge-voice-video.md` section 5 item 5: the set, "once", per-key verification, and that
    sign-out drops this device's identity key and pins (so contacts see one notice after the next sign-in).
  - Sweep row `lounge-voice-key-changed` clicks the shield by its new label.

## Not done / left
- Inventory: there are no SEC-AUD-046/047 rows in `docs/inventory/21-feature-phase.md` yet; I did not add them
  (six builders appending at the same line). Suggested rows for the commander:
  - `SEC-AUD-046` [FE] List-load failures are inline with Try again (retried on `online`); the favourites sync
    is quiet; `--ui-danger-text` and the entrance danger token (D-577). Handoff `fix-p55-webui.md`.
  - `SEC-AUD-047` [FE] E2EE key pins are a set per person; a new key is named as a new device or sign-in, once;
    verified marks per person and key (D-578). Handoff `fix-p55-webui.md`.
- The unsynced favourites mark lives for the session only: a reload with an unsaved change takes the account
  copy (the wave 54 prefs sync keeps a localStorage mark; D-577 asks only for retry on change and `online`).
- `packages/wasm/dist` (the in-browser theme validator, `sr-theme-wasm`) is prebuilt from the old registry until
  the next wasm build: a custom theme package that sets `--ui-danger-text` is called an unknown token there.
  Built-in themes are not affected.
- The prebuilt `sr-server` likewise validates uploaded theme packages with the old registry until rebuilt.

## Visual check
Stack: p55-bin binaries copied to `p55-webui-bin`, DB `sr_p55_webui_ui` (dropped), ports 8353/5353 (metrics
9353; stopped, port check clean). All PNGs below were looked at.
- `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p55-webui/` (32 PNGs), spec
  `e2e/tests/p55-webui-shots.spec.ts` (`SR_P55_WEBUI=1`, local only): channel list, open channel and member list
  failed (`lists-failed-*`; the page's gateway socket is routed and the server's GUILD_CREATE dropped, so the REST
  load runs and is answered 500), Server Settings failed, the kit's error text (Server Settings > Channels, emptied
  name), the soundboard unsynced line (PATCH settings answered 500), the shield's new privacy code notice; Editorial,
  Lounge Light, Lounge Dark; desktop 1280x800 and phone 375x812. The refused invite on the landing, desktop and phone
  (the entrance has one look).
- Sweep rows `lounge-voice-key-changed`, `lounge-channel`, `settings-server-channels` in Lounge Light + Editorial
  (`.../visual-sweep-shots/p55-webui-sweep-lounge-editorial/`) and Lounge Light + Lounge Dark
  (`.../p55-webui-sweep-lounge-dark/`). Flags: only the pre-existing 404 console line on `lounge-channel`.
- Fixed because of the shots (`c2d1f3f5`, `71f08d62`): with the channel list failed the open channel said "This
  channel doesn't exist" (now "This channel could not be loaded." with Try again); the member list showed only the
  people known so far (you) and hid the failure (the row now leads the list whenever the load failed); in Server
  Settings "Try again" rendered as a grey default button because the Lounge stylesheets are not loaded there (the
  row now has its own `lounge/listLoadFailed.css`). Before: first-run `lists-failed-desktop-lounge-editorial.png` and
  `server-settings-failed-desktop-lounge-editorial.png` (overwritten); after: the files of the same names now.
- Seen, not mine: a server whose GUILD_CREATE never arrived shows an empty name in the channel header and a blank
  rail icon (the READY stub has no name); that is the stub, unchanged.
- On a phone the member list is not on screen (Discord-like), so the member row was shot on desktop only.

## Tests run
- Under the lock: `cargo fmt --all -- --check`, `cargo clippy -j 4 -p sr-theme --all-targets -- -D warnings`,
  `cargo test -j 4 -p sr-theme` (all pass, including `registry_covers_every_ui_kit_token` and
  `committed_typescript_is_current`); `packages/ui` vitest 18 files / 360 tests; web vitest over lounge, voice,
  settings/server, components, i18n: 163 files / 1404 tests, all pass.
- After the two fixes from the shots (`c2d1f3f5`, `71f08d62`, Lounge-only code, no public page): web tsc, eslint,
  prettier, `check-css-tokens.mjs`, and single-file vitest runs of `p55ListLoads`, `roving` and `shell` (all pass).
- `pnpm --filter @sideroom/web build`, `check-entrance-isolation.mjs` (every public surface clean; landing
  115.8/120 KB, auth 121.4/150 KB), `check-bundle-budget.mjs` pass; `playwright test --list --project chromium`
  loads 937 tests in 75 files.
- web and `packages/ui` tsc, e2e tsc, eslint and prettier on changed files, `check-css-tokens.mjs` clean,
  `i18n-audit.mjs` 0, `check-doc-links.mjs` ok.
- New or changed tests (each fails on the code before the change): `lounge/__tests__/p55ListLoads.test.tsx` (2:
  inline rows, Try again, `online`, no toast), `voice/__tests__/soundboard.test.ts` (+1: quiet failure, kept,
  resent on `online`), `lounge/voice/__tests__/soundboardUi.test.tsx` (+1), `voice/__tests__/keyPins.test.ts` (10:
  set, once, no flip-flop, cap, verify refresh, legacy pin, per-key verified marks, the call binding keeps the
  notice), `lounge/voice/__tests__/privacyCodesUi.test.tsx` (new copy, axe),
  `components/__tests__/entranceDangerText.test.ts` (3), `packages/ui/src/theme/tokens-contrast.test.ts` (+7:
  danger text per theme, both contrast settings).

## Threat-model row (proposed, for the commander to number)
docs/15 has no row for the client key pins yet (D-564 had none either). Proposed: "A server (or the delivery
service) substitutes a person's E2EE identity key in a call. Mitigation: each viewer's device keeps the set of keys
it has seen per person (D-564, D-578); a key not in the set is flagged in the shield as a new privacy code and has
no verified mark until the viewer compares codes; the server never pins and there is no setting. Residual: the
first key of a person is trusted on first use, and a viewer who ignores the one notice keeps talking to the new
key (as with any TOFU scheme); sign-out empties the set."

## Owner questions
- "Shows the notice once" (D-578) is built as: the first call that carries a key new to the set shows the notice
  for the rest of that call, and the key joins the set at once; later calls with that key are silent even if the
  viewer never verified it (the person stays unverified, without the verified mark). This is how Signal treats a
  changed safety number. The alternative reading (the key joins the set only when the viewer verifies, so the
  notice repeats every call until then) would bring back the repetition D-578 wants to remove. Fine as built?
- A key new to the set clears only that key's verified mark; the person's other verified devices keep theirs.
  The shield's "everyone verified" state reads the key each person carries in this call. Fine?
- The failed-list rows use `role="status"` (polite) rather than `alert`: a background load did not interrupt the
  member, so a screen reader hears it when idle. Fine?
