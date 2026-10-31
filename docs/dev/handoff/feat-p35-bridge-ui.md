# feat/p35-bridge-ui: BRG-UI-001 (wave 35, the Discord bridge: settings, notices, opt-out, DSR)

Branch `feat/p35-bridge-ui`, base `integration/phase35`. Agent "bridge-ui". Decision: D-381 (owner). D-385 respected: nothing here
loads before sign-in.

STATUS: see the "Tests actually run" section at the end; items that need bridge-core's server or fake Discord are marked.

## Done

Everything is built against **MSW handlers**; the wire shapes are collected in ONE file (`apps/web/src/settings/server/bridge/api.ts`) so
they can be reconciled with bridge-core's routes in one place (see "Reconcile at integration").

- **Modules card** (`settings/server/bridge/BridgeCard.tsx`, rendered by `sections/Features.tsx` for module id `bridge.enabled`): the
  group "Integrations, more" comes from the catalog's `section`, the privacy notice sits **before** the switch, the switch is greyed
  for anyone who is not the guild owner with "Only the server owner can switch this on/off.", locked when `platform_disabled`.
  Switch-on is `PATCH /guilds/{g}/sr/features/bridge.enabled {enabled:true}` through `withReauth` (the MFA step). Switch-off opens the
  teardown dialog instead of PATCHing.
- **Conditional nav entry** (`ServerSettings.tsx`, one `SectionDef` `bridge`, group APPS): `visible(v, ctx)` is
  `ctx.bridgeOn && (owner || MANAGE_GUILD)`. `bridgeOn` comes from `bridge/moduleStore.ts` (zustand): filled by the Modules screen's
  feature list, or by one `GET .../sr/features` when Server Settings opens for an owner/manager. Off, platform-killed or unknown = no
  entry. A deep link to `/settings/server/{g}/bridge` waits (Loading) for the state instead of redirecting to the first section.
- **Screen** (`bridge/BridgeSection.tsx`): the privacy notice, token paste (password field, sent once, cleared, never shown again, owner
  only; a manager sees why not), connection state, the Message Content intent warning, the bot invite link, the mapping table
  (SIDE ROOM channel, Discord channel, direction, opt-out **count** per channel, Test, Remove), the add-a-mapping form (only unmapped
  text/announcement channels; says encrypted channels are refused and shows the server's refusal), owner-only "Disconnect the bot",
  owner-only "Switch off".
- **Teardown dialog** (`bridge/TeardownDialog.tsx`): what is removed (webhooks on Discord, maps, message links, opt-outs, token), what
  stays (messages already mirrored on both sides), the Discord-side step, "starts empty when switched on again"; after the switch-off
  it shows the number of maps removed and **names every webhook id Discord would not delete** (`teardown.failed_webhooks`) with where to
  remove them by hand. The nav entry goes only when the owner closes the result.
- **Channel banner** (`lounge/bridge/Banner.tsx`, mounted in `ChannelView.tsx` above the scrapbook banner): persistent, direction-aware
  wording naming the Discord server, plus the member's opt-out button. **Menu entry** "Don't mirror my messages" / "Mirror my messages
  again" in the channel context menu (`lounge/bridge/useBridgeMenu.ts`, one spread in `useChannelMenu.ts`); none for a `from_discord`
  channel (nothing of the member's goes out).
- **Member list of bridged channels** (`lounge/bridge/store.ts`): ONE request for the account, `GET /users/@me/bridge/channels`, asked
  when a server channel is shown and again after 5 minutes; empty while no bridge is on anywhere. Opt-out is optimistic with rollback.
- **DISCORD tag**: `Message.sr_bridge` (store type) -> `LoungeUser.bridge`; the author shows the Discord name, the tag, no profile to open,
  and only a same-origin `/cdn/external/...` avatar is ever drawn (a Discord URL is never loaded directly, D-176). Grouping now also
  splits on the Discord name, because every Discord person arrives under the one bridge author.
- **Help privacy page**: the handbook's privacy section gains a paragraph (`help.handbook.privacy.bridge`) and, for a signed-in person, the
  live list of bridged channels with the member's opt-out (`help/BridgedChannels.tsx`); it empties when a server switches off. (The
  handbook is behind sign-in since D-385; the entrance isolation check is unaffected, see tests.)
- **DSR** (`crates/sr-admin/src/data_requests.rs` + the usual one-line registrations, see "Files outside the Split row"): export gains
  `bridge.json` (the member's bridged messages as ids only, and their opt-outs); erasure queues a `delete` on the outbound queue for each
  webhook message the bridge posted for the member (best effort), deletes their message-map rows and opt-outs, and the request outcome
  says how many.
- **Docs**: lawyer brief Q21 rewritten (what is built, retention of the map = as long as the bridge is on, teardown answer, DSR),
  `docs/09` row (map rows live as long as the bridge is on).
- **i18n**: `apps/web/src/i18n/messages/en/p35-bridge-ui-1.ts` (all strings, no underscores in key segments).
- **No migration**: `bridge_optouts` is in bridge-core's 1720, so 1740-1749 stays unused.

## Files outside the Split row (small, additive; flagged)

- `apps/web/src/lounge/{MessageItem.tsx, data.ts, liveMap.ts, messageRows.ts}`, `apps/web/src/store/types.ts`: the DISCORD tag needs the
  `sr_bridge` field, the mapping and the grouping rule (the row says "the DISCORD tag").
- `apps/web/src/lounge/ChannelView.tsx`, `lounge/shell/useChannelMenu.ts`: the banner and the menu entry (named in the row).
- `apps/web/src/test/server.ts`: a default MSW handler for `GET /users/@me/bridge/channels` (empty list) so no other test warns.
- `crates/sr-db/src/{bridge_dsr.rs (new), lib.rs, account_deletion.rs, admin_repo_dsr.rs, data_export.rs}`: the actual DSR work lives in
  sr-db (the Split names only `sr-admin/data_requests.rs`); same pattern as `scrapbook_legal`/`rings_dsr`. `bridge_dsr` treats a database
  without migration 1720 as having no bridge rows (`to_regclass`), so this branch alone does not break account deletion.

## Reconcile at integration (bridge-core / bridge-gateway)

bridge-core had committed the schema and the module when I read it (`123b5594`, `2372726d`); its routes were not there yet. I assumed:

| Call | Shape assumed here |
|---|---|
| `GET /guilds/{g}/bridge` | `{ state: no_token|connecting|connected|error, token_set, discord_guild_id?, discord_guild_name?, message_content_intent?, invite_url?, last_error?, maps: [{channel_id, discord_channel_id, discord_channel_name?, direction, optout_count}] }` |
| `PUT /guilds/{g}/bridge/token` `{token}` | the status above; owner only; `DELETE` same path revokes |
| `GET /guilds/{g}/bridge/discord-channels` | `[{id, name}]` (the bot's view) |
| `POST /guilds/{g}/bridge/maps` `{channel_id, discord_channel_id, direction}` / `DELETE .../maps/{channel_id}` | the map / 204 |
| `POST /guilds/{g}/bridge/test` `{channel_id}` | `{ok:true}` |
| `PATCH /guilds/{g}/sr/features/bridge.enabled` | the `GuildFeature` plus optional `owner_only` and, on switch-off, `teardown: {maps_removed, failed_webhooks: [id]}` |
| `GET /users/@me/bridge/channels` | `[{channel_id, guild_id, channel_name, guild_name, discord_guild_name, direction, opted_out}]`, empty while off |
| `PUT`/`DELETE /channels/{c}/bridge/opt-out` | 204 |
| message payload | `sr_bridge: {platform:"discord", author_name, author_avatar}` with `author_avatar` a `/cdn/external/` path |
| outbound queue row for erasure | `bridge_outbox(kind='delete', payload {discord_message_id, webhook_id, reason:'erasure'})` with id `-sr_message_id`; the worker must delete from the payload because the map row is gone |

**Gap to close: nobody in the Split owns the two member routes** (`GET /users/@me/bridge/channels`, `PUT/DELETE /channels/{c}/bridge/opt-out`)
or the `teardown` report on the switch-off answer. They need handlers, both matrices (`route_matrix.tsv`, `authz_matrix.rs`) and
OpenAPI, normally in bridge-core's `routes/guilds/bridge.rs`.

## Still open

- `e2e/tests/bridge.spec.ts` and `e2e/support/bridgeFake.ts` are written and typecheck; **not run** (needs bridge-core's fake Discord on 8090
  and the server pointed at it; the fake's control routes `__fake/state|inject|reset` in `bridgeFake.ts` are an assumption to reconcile).
  The spec skips itself when the fake is not up.
- A gateway event to refresh the banner/list when a mapping changes (today: refresh after 5 minutes or a page load; the opt-out and the
  Modules screen update at once).
- Authz matrix rows for the routes above (bridge-core), Admin > System tile (bridge-core).

## Owner questions

1. Is `GET /users/@me/bridge/channels` (one request per account per 5 minutes) acceptable on instances where no bridge exists? It is
   the cheapest way to avoid asking every server; the alternative is a flag on the guild payload.
2. Erasure queues Discord-side deletes best effort and removes the map rows regardless (lawyer brief Q21 says so). Should erasure instead
   wait for Discord's answer?

## CHANGELOG lines

- Added: Discord bridge settings (Modules card with the privacy notice, owner-only switch, "Discord bridge" screen with token, channel
  mapping, test, opt-out counts and a teardown dialog), a banner and a per-member "Don't mirror my messages" option in mirrored
  channels, the DISCORD tag, a list of mirrored channels on the privacy page, and data export/erasure of the member's bridged messages.
