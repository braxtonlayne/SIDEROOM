# Wave 55 chan handoff (D-575)

Branch `fix/p55-chan`, based on `integration/phase54`.

## Done

### SEC-AUD-043 (D-575): moves into a category and `lock_permissions`
- `crates/sr-chat/src/admin_service.rs`:
  - New `check_category_moves`: a channel whose parent changes to a different category needs `MANAGE_CHANNELS` in
    that category, evaluated with the category's overwrites (`access`): a guild-level holder passes unless the
    category denies it, a member allow on the category lets a member in. A position change under the same parent,
    or a move out of a category (`parent_id: null`), needs only what the route already checked. Each target category
    is checked once per call.
  - `edit_channel` (`PATCH /channels/{id}`) runs it on the moved channel; `reorder_channels`
    (`PATCH /guilds/{id}/channels`, the bulk route, which can move across categories) runs it on every changed
    channel, comparing against the stored parent. A refusal is `50013` and nothing is written.
  - `lock_permissions` on the single-channel edit is honoured only when the actor has `MANAGE_ROLES` in the channel
    (overwrites applied); otherwise the move lands and the channel's overwrites stay, as the row says.
  - The bulk route's `lock_permissions` is unchanged (guild-level `MANAGE_CHANNELS`, Discord's route; see owner
    question 2).
- Tests (`crates/sr-chat/src/admin_tests.rs`, both fail without the change):
  - `moving_into_a_category_needs_manage_channels_there`: a category deny stops a guild-level holder on both routes
    and nothing moves; same-parent position changes and a move out of a category pass; an allow on the channel alone
    does not carry a member into a category, an allow on the category too does.
  - `lock_permissions_on_a_channel_edit_needs_manage_roles_in_the_channel`: without `MANAGE_ROLES` the move lands and
    the overwrites stay; a guild-level `MANAGE_ROLES` holder denied it in the channel gets no sync; with it the
    channel takes the category's overwrites.
- No route, DTO, migration or API contract change (no openapi regeneration needed). No UI change: the web "Sync with
  category" switch already needs `MANAGE_ROLES` (`Channels.tsx`), so no visual check applies.

## Not done / left
- No inventory row `SEC-AUD-043` exists yet in `docs/inventory/21-feature-phase.md` (the list ends at 040); I did not
  add one to avoid six builders appending at the same line. Commander: add it ticked, pointing at this handoff.
- Edge case in the web UI: a member with guild-level `MANAGE_ROLES` but a channel deny of it can flip the sync
  switch; the server now ignores the sync and the list refetch shows it unsynced (no error shown). Small follow-up if
  wanted: compute the switch's disabled state from channel-level permissions.

## Tests run
RESULTS_PLACEHOLDER

## Owner questions (for Fable)
1. Moving a channel out of its category to the top level (`parent_id: null`) needs only the channel's
   `MANAGE_CHANNELS` (there is no target category). Should it instead need the guild-level bit? Chosen: channel only.
2. The bulk route (`PATCH /guilds/{id}/channels`) still honours `lock_permissions` with guild-level
   `MANAGE_CHANNELS` alone (Discord's behaviour, as D-575's reasoning notes). Should it also need `MANAGE_ROLES` in
   the channel, like the single edit now does? Chosen: unchanged, per the row.
3. The source category is not checked (only the target). Discord checks neither; D-575 names only the target.

## Threat-model rows
None proposed: this narrows an existing authorization surface (no new surface).
