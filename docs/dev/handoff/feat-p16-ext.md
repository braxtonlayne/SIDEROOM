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
1. Precedence: the server theme is laid over the viewer's own Theme Store theme while they are in that server (their "Allow server themes" switch is the opt-out); the Theme Store preview still wins. Accent colour set by a manager wins over an accent token in the theme. Confirm.
2. A server theme applies in the Lounge only (sections `all` or `lounge`); Cinema/Library/Entrance are not recoloured by a server.
3. Adding a pack needs both `MANAGE_GUILD` (the existing extension rule) and `MANAGE_GUILD_EXPRESSIONS` (the item rule). Removing a pack deletes its items without asking again beyond the button.
4. Re-adding a pack skips names the server already has (so a repeat does not duplicate).
5. Sticker packs accept PNG/APNG/GIF only (WebP stickers are not a server sticker format today); those items are reported as skipped.
