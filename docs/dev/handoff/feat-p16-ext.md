# feat/p16-ext handoff

Branch `feat/p16-ext` (based on `feat/p16-lounge`). Migration `1060_guild_appearance.sql`. Local commits only.

## Done
- **LNG-SSET-050 + EXT-PKG-006** (ticked). Server Settings > Extensions & Themes:
  - Server-wide theme: `PUT /guilds/{id}/packages/{package}` with section `all` or `lounge` (other sections now 400 for a guild theme); `DELETE` stops it. Needs `MANAGE_GUILD` as before.
  - Server accent colour: `GET|PATCH /guilds/{id}/appearance`. `GET` is for any member, `PATCH` needs `MANAGE_GUILD` (and the MFA gate). Table `guild_appearance`.
  - Add a pack to the server: `POST /guilds/{id}/packages/{package}/import` (emoji, sticker, sound packs). Turns the pack on and copies its items into the server's own expressions through the existing `SettingsService` create methods, so slot limits, name rules, size rules and the audit log apply (also needs `MANAGE_GUILD_EXPRESSIONS`). Items that do not fit (name already used, no slot, wrong file type) are returned under `skipped` with the reason. Table `guild_pack_items` remembers which items came from which pack; `DELETE /guilds/{id}/packages/{package}` removes exactly those and the pack. Expression update events are published as for the normal routes.
  - Gateway: new dispatch `SR_GUILD_APPEARANCE_UPDATE` (`{guild_id}`; `DomainEvent::GuildAppearanceUpdated`, `SettingsEvent::AppearanceChanged`), documented in doc 05 and the generated gateway reference.
  - Web: "Server look" card (theme in use, accent colour picker with save and clear), Themes kind in the browser (Use for this server / Stop using), "Add to server" / "Remove from server" on packs with a report of added and skipped items. Members' clients: `packages/serverAppearance.ts` (`useServerAppearance`) reads the appearance when a server opens and again on the event; `LoungeShell` lays the tokens (re-checked with `safeTokens`) over the viewer's theme and passes the accent to `ThemeProvider`. Nothing applies when the viewer turned off "Allow server themes" (existing Appearance setting) or is on Home.
- **VOX-UI-003 remainder** (ticked). No protocol change was needed: the SFU already labels each forwarded microphone line with the speaker's id (msid stream id), and the browser's `RTCRtpReceiver.getStats()` gives that line's inbound-rtp (loss, jitter, bytes). `voice/quality.ts` (`inboundSample`, `remoteQuality`), `engine.ts` (`pollRemote` on the existing 2 s timer, `remoteQuality` event), session store `remoteQuality`, `RemoteQualityBars` on remote tiles (loss, jitter, kbit/s in the label). A paused sender (Opus DTX, muted) keeps the last verdict.
- i18n fragment `p16-ext-1.ts`. Route matrix rows reviewed (no `?`), OpenAPI and api-client regenerated.

## Still open
- **LNG-CHN-017** (per-channel module switches): not started. Still needs a channel-scope module schema (no module uses `Scope::Channel`), an owner/design decision on which modules get channel scope; too large to do well here.

## Tests run
- `sr-packages` Postgres suite `tests/db.rs` in a scratch DB (`--include-ignored --test-threads=1`): 10/10, new `a_server_gets_a_theme_an_accent_colour_and_pack_items_it_can_take_back`.
- `sr-gateway` lib test `guild_appearance_update_is_dispatched_to_members`; `sr-api` `settings::events` mapping test case; `route_matrix`, `openapi_names`.
- Web vitest (`--maxWorkers=2`): extensions, `p16extServerLook`, `packages/serverAppearance`, `voice/p16Remote`, `voice/engine` (47 pass); `tsc --noEmit`; prettier.
- No Playwright, no servers. The import handler itself (HTTP layer over SettingsService) has no HTTP-level test: the sr-api test harness has no Postgres-backed package service; the storage-side pieces are covered by the service test and the UI by MSW.

## Decisions / owner questions

Answered 2026-10-19 as D-280 to D-283 (`docs/01-decisions-log.md`, "Phase 16 extensions questions" in `docs/open-question-answers.md`); code follow-ups L-40 to L-43 in `docs/dev/answers-followups-p16.md`. In short: 1 confirmed (D-280); 2 confirmed, `all` stored as `lounge` (D-281); 3 changed, `MANAGE_GUILD_EXPRESSIONS` alone for packs (D-282); 4 and 5 confirmed, the pack validator refuses WebP stickers at upload (D-283).
1. Precedence: the server theme is laid over the viewer's own Theme Store theme while they are in that server (their "Allow server themes" switch is the opt-out); the Theme Store preview still wins. Accent colour set by a manager wins over an accent token in the theme. Confirm.
2. A server theme applies in the Lounge only (sections `all` or `lounge`); Cinema/Library/Entrance are not recoloured by a server.
3. Adding a pack needs both `MANAGE_GUILD` (the existing extension rule) and `MANAGE_GUILD_EXPRESSIONS` (the item rule). Removing a pack deletes its items without asking again beyond the button.
4. Re-adding a pack skips names the server already has (so a repeat does not duplicate).
5. Sticker packs accept PNG/APNG/GIF only (WebP stickers are not a server sticker format today); those items are reported as skipped.

## Follow-ups (L-40..L-43, D-280..D-283)

Done (all four ticked in `docs/dev/answers-followups-p16.md`):

* **L-40.** Precedence documented (doc 10, developer-platform spec, `serverAppearance.ts`); vitest for accent over the theme's accent token and the switch off. Finding: `LoungeShell` laid the server style after the member's style, so it also beat a Theme Store preview. Fixed: while a preview is open the server theme and accent step aside (`p16ServerLook.test.tsx`).
* **L-41.** `PackageService::enable` stores a guild theme's `all` as `lounge` (and replaces an older `all` row); other sections are refused; OpenAPI description of `section`; docs 10 and spec ("Not built" trimmed, import and appearance routes listed). No migration: 1060 is new on this branch, and `appearance()` still reads an `all` row.
* **L-42.** `require_manage_or_expressions` (MFA gate for either permission) for import, removal of a pack and the list (an expressions-only viewer gets packs only); removal keeps `MANAGE_GUILD` for themes, recipes and templates, and answers 404 to either holder when nothing is on. `Enablement.item_count` (packs in a server) added; OpenAPI, api-client types and MSW mocks regenerated. Web: the Extensions section opens for either permission, expressions-only viewers get the three pack kinds, no Server look card and no non-pack rows; the remove button and its aria-label name the count (`p16-ext-2.ts`; old keys removed from `p16-ext-1.ts`). Route matrix rows stay `membership` (no `?`). Docs: doc 07 paragraph, spec routes line.
* **L-43.** Validator refuses WebP under `stickers/`; `image_dims` reads `VP8 `/`VP8L`/`VP8X` so the pixel cap covers WebP emoji; unit tests; import guard comment names D-283.

Tests run: sr-packages unit (20) and Postgres suite (10, scratch DB); sr-api lib (613 + the new Postgres route test `settings/tests/packages_pg.rs`: expressions-only may import and remove a pack, not enable a theme or apply a recipe; `MANAGE_GUILD` alone may still import; guest refused), `route_matrix`, `openapi_names`; fmt and clippy `-D warnings` (sr-packages, sr-api, all targets); web tsc, eslint (touched dirs), prettier, full vitest (254 files, 2343 tests, 2 workers), vite build and the bundle budget.

Still open / notes:

* A direct `PUT /guilds/{id}/packages/{pkg}` for a pack still needs `MANAGE_GUILD` (the page uses `import`, which turns the pack on itself).
* `MANAGE_GUILD` without `MANAGE_GUILD_EXPRESSIONS` can still import and remove a pack as before, but the item-level permission skips or leaves the files (existing behaviour, not changed).
* The wasm package had to be built in this worktree first (the five formerly unloadable test files now run and pass).
