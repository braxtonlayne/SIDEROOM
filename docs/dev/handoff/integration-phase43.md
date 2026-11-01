# Integration of wave 43 (ARC-GST-001, ARC-GST-002): guest links

Branch `integration/phase43`, base `origin/integration/phase42` (defe9413; PR #73 still in CI, no new commits on it
when this integration ran, and it had not merged into `main`). Then `feat/p43-guest-relay` (eaf4dc1c, ARC-GST-001) and
`feat/p43-guest-runner` (1ef5b152, ARC-GST-002), both `--no-ff`. Not pushed; no PR opened.

Handoffs: [feat-p43-guest-relay](feat-p43-guest-relay.md), [feat-p43-guest-runner](feat-p43-guest-runner.md).
Decisions: D-412 to D-419 (integrator defaults; the owner may change any). Threat model: T41 (relay), T42 (runner).

## Merge conflicts and how they were resolved

- `docs/15-threat-model.md`: 43-A's T41 kept, 43-B's T41 stub dropped, 43-B's T42 kept (the doc test needs the rows in
  order; `scripts/threat-model.test.mjs` passes).
- `docs/inventory/21-feature-phase.md`: one "Wave 43 — Guest links (roadmap P28)" header, ARC-GST-001 then ARC-GST-002.
- Nothing else conflicted: 43-B added no route, no OpenAPI schema and no Rust outside `web.rs`; OpenAPI, api-client and
  MSW handlers were regenerated once on the merged tree and did not change (1173 operations, `mocks:check` green).

## What the integration changed

1. **Contract reconciled** (`apps/web/src/public/guest-play/contract.ts`, `session.ts`, `Runner.tsx`). The runner was
   built before 43-A added `GuestJoined.capabilities` and fell back to an assumed `["guest_play", "datachannel"]`. Now
   the field is required: a join answer without it is refused (`refused`), never assumed, so the `ingame_voice`
   refusal fires against the real relay. Everything else already matched the relay exactly: numeric `seat`/`from` +
   `role` (the runner builds `g<seat>`/`m<seat>`), close codes 4400/4401/4403/4409/4410/4413, member seat by
   `hello {lobby_token}` with the session cookie and the app's own Origin (`allowed_origins` = `server.public_url`),
   standard base64 `data`, `ready`/`peer`/`closed` frames.
2. **Bundle serving for a guest (D-385, D-418).** The runner frames `<application-id>.activities.<host>/`, the existing
   activity proxy origin. It needs no cookie (the proxy never reads one), serves only the reviewed bundle of a released
   application, and has always answered anyone who knows the application id; it carries none of our code. So no
   token-scoped route was built (no new route, no matrix row, no ceiling). What the guest's browser may reach is pinned
   by the real-stack proof instead: the guest-play shell, `POST /api/v1/play/<token>/join`, `/gateway/guest` and this
   one game's activity origin; no other activity host, no other API route, no cookie set.
3. **The real-stack gate proof ran** (`e2e/tests/guest-play.spec.ts`, part 2, Chromium,
   `SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1`) and needed these fixes, all in the spec:
   - the link field locator (`getByLabel('Guest link')` also matched the QR code's label);
   - the guest's callsign is random, read from the runner's "Playing as" bar (the spec expected "Amber");
   - the 403 probes now use real routes of the call's server (made-up paths answered 404/405, proving nothing), as
     bearer and as `sr_session` cookie: `users/@me`, `users/@me/guilds`, the text and voice channel's messages, the
     guild's members, `users/@me/guilds/{id}/member`, `users/1/profile`, `shelves/visible`, `activities`;
   - the forged subscription is now real: a guest token from the join route, `hello`, `ready`, then a `subscribe` to
     another lobby's topic is closed **4403**; a made-up token is closed 4401;
   - the `ingame_voice` test was rewritten: it only uploaded a bundle (and got 403, no CSRF header). Now a Tier 1 game
     declaring `ingame_voice` runs in a call, Alice makes a real link, the relay's join answer lists `ingame_voice`,
     and the guest's runner shows the refusal with no game frame and no guest socket. The same test checks the Share
     link dialog at 375 px (dialog, link, QR code and Done inside the screen, no sideways scroll).
4. **CI** (`.github/workflows/e2e.yml`, live job): a "Guest links (Chromium)" step runs `guest-play.spec.ts` in its own
   `e2e-run.sh` server run (it registers 4 accounts; the registration cap is per run) with `SR_E2E_GUEST=1`, and
   uploads `server-guest-play.log`. Parsed with the repo's js-yaml.
5. **Entrance isolation after the OpenAPI merge.** The API denylist now lists `/play`; two allow entries with reasons:
   the guest-play route path `/play/$token` and `sw.js`'s `"/play/*"` surface pattern. The `/api/v1/play/` entry is no
   longer stale. Every surface clean (guest-play 114.0/120 KB gzip, 2 files).
6. **Phone and en-XA.** `i18n-pseudo.spec.ts` gained two signed-out rows, `/play/<token>#key=...` (the "Join the game"
   card) and `/play/<token>` without a key (the incomplete-link message), at desktop and phone. The phone rows found
   the card 9 px wider than the screen; `guest.css` now sizes the card and `main` with `border-box` and wraps long
   words (`overflow-wrap: anywhere`). The Share dialog cannot be a row of that spec (it needs the voice and Activities
   stack); its phone check is in `guest-play.spec.ts` (English only, see Not done).

## Gates on the merged tree

| Gate | Result |
|---|---|
GATES_TABLE

## Not done

- **Share dialog in en-XA.** `?lang=en-XA` lasts one page load and the dialog needs a running instance in a call, so
  the pseudo-locale pass covers `/play` only; the dialog is checked at phone width in English.
- **Firefox/WebKit** for `guest-play.spec.ts` (Chromium only, as the other Activity specs: Chromium resolves
  `*.localhost` activity hosts by itself).
- Everything the two handoffs list as open stays open: lobby-approved nickname (D-412), shared bridge (D-413),
  text-channel links (D-414), members-only lobby (D-419), rekey on revoke (D-416), multi-node lobby routing (D-417),
  the media leg of `ingame_voice` (design note only).

## Owner items

In `docs/dev/owner-checklist.md` section 4b, "Guest links (wave 43)": guest names (nickname approval), who bridges the
guests, text-channel links, a members-only lobby, rekey on revoke, the limits and the single-node relay.

## Resume here

Nothing is half-done. If PR #73 (wave 42) gets more spec fixes, merge `origin/integration/phase42` again; if it merges,
merge `origin/main`. The next CI run's live job is the first CI run of the guest-links step.
