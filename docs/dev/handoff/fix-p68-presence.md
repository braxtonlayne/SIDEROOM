# Wave 68 presence (SEC-AUD-206, 221, 222; D-702, D-703, D-704)

Branch `fix/p68-presence` (base `integration/phase68`). All three items done. No migration, no route, no OpenAPI change (the gateway is not in OpenAPI).

## Done (commits)
- `SEC-AUD-221` (5677df99): `DomainEvent::ReactionAdded.message_author: Option<UserId>` (`#[serde(default)]`); the gateway adds `message_author_id` (string) to `MESSAGE_REACTION_ADD` when known; the reaction route passes `t.message.author_id`; `reactionDecision` and the `MESSAGE_REACTION_ADD` branch of `useDesktopNotify` (`lounge/shell/desktopNotify.ts`).
- `SEC-AUD-222` (3651a7c7): digest switch removed from Settings > Notifications; the reactions switch has its description; the push card's note no longer names the digest.
- `SEC-AUD-206` (4cab2223 and the final commit): IDENTIFY `properties` from `clientProperties`, `client_status` in the store, the phone glyph, a gateway fix (below), docs, e2e spec, this handoff.

## Not done
Nothing dropped. Two small notes: the notification body is empty when the reacted-to message is not loaded in the store (the plan allows it; the e2e opens no channel so it asserts empty); the phone-size member-list shot is taken after a swipe to the members pane (the popout shot shows it behind the card).

## A server bug found and fixed (`crates/sr-gateway/src/conn.rs`)
`create_session` announces the user's presence before IDENTIFY's `properties` are read, so a new session counted as `web` until its first op 3. The web app sends no presence at IDENTIFY, so a phone would have been shown as `web` (no glyph) for as long as it sent nothing. `conn.rs` now calls `refresh_presence(user, false)` after `set_session_platform` when IDENTIFY carried no presence. Others see two updates in a row (web, then mobile); the existing tests passed an explicit op 3, which hid it. RESUME was checked by reading: it keeps the session id and its `session_presence` entry, and does not call `set_session_platform`, so the platform survives.

## How the glyph is drawn (option c, a generic prop)
`packages/ui` `Avatar` gained `statusIcon?: ReactNode` and `statusLabel?: string` (no wording, no section or theme name; the dot span keeps its place, ring of the page colour, status colour for the glyph, 12 px minimum instead of 8, name from `statusLabel`). Options a and b did not fit: `Avatar` had no way to decorate the dot, and an overlay would have meant wrapping the avatar in eight places and hiding the dot in each. Lounge code (`lounge/phoneStatus.tsx`, `statusProps(presence)`) spreads one set of props into every Avatar that draws a person's status: member list, profile avatar (popout and panel), friends, DM list, DM header (`ChannelHeader` got an optional `phone` prop), group DM member rows, the create-DM picker, and the viewer's own avatar (`StatusMenu`, the user panel). Icon: the existing `phone` icon of `@sideroom/ui` (a handset; the set has no smartphone). At 12 px it reads as a handset in all three themes (shots `glyph-closeup-*`, `friends-dmlist-*`); it is small, and a smartphone outline would read better, but the brief says to draw none. Rule: `lounge/phoneOnly.ts` (`phoneOnly`), used by `toLoungePresence` which sets `Presence.phone`. Where a status is written as text: only the member list row's accessible name ("..., online on a phone"); the profile popout and friends list write no status text.

## Every place a presence enters the store
All go through `putPresences` (reducer.ts): READY `presences`, GUILD_CREATE `presences` (`reduceGuildPeople`), the deferred people pass, `GUILD_MEMBERS_CHUNK` `presences`, `PRESENCE_UPDATE`. `SR_ACTIVITY_UPDATE` spreads the previous presence, so `client_status` stays. `offline(...)` in `hooks.ts` has none (right). `useMembers`/`usePresence` read the store object whole. The Lounge adapter `toLoungePresence` (`liveMap.ts`) was the one place that stripped fields: it now carries `phone`. `toClientStatus` keeps the three known keys with a known status and drops the rest (an empty result is omitted).

## First paint
Before (wave 67): `app-*.js` 214,946 bytes gzip, room 41,054. After: `app-D-JcQBzE.js` 215,491 bytes gzip, room 40,509 (+545 bytes; no import added to any first-paint file). `check-bundle-budget`, `check-entrance-isolation` (every public surface clean) and `check-staff-delivery` all pass after a full `pnpm build`. `packages/ui` is on the public pages: the Avatar change adds a prop and three CSS lines, nothing named.

## Constructor sites changed outside my table (one line each, `message_author: ...`)
`sr-server/src/bridge_inbound.rs` (`Some(msg.author_id)`), `sr-server/src/userfeat.rs` (`Some(msg.author_id)`), tests: `sr-server/src/feature_actions_pg_tests.rs`, `scrapbook_pg_tests.rs`, `sr-features/src/modules/{reaction_roles,scrapbook,starboard}.rs` (`None`; one pattern in `reaction_roles.rs` got `..`), `sr-events/src/event.rs` (a test), `sr-gateway/src/tests.rs`. Outside the web table: `lounge/liveMap.ts` (one line: `phone`; previews' file), `lounge/ChannelHeader.tsx` (the `phone` prop), `lounge/shell/StatusMenu.tsx`.

## What the author lookup costs
Nothing: the reaction route already loads the message (`target(...)` returns `t.message`); the bridge and the vote-seeding feature already hold the message too. No query added.

## Tests (and which I watched fail)
All new; I did not run the base, so none was watched failing against it, except as noted. Run: Rust `sr-gateway`: `a_phone_session_is_reported_as_mobile_to_others` (it failed first on the bug above: the first announcement said `web`; fixed in `conn.rs`, then passed), `an_invisible_phone_session_reports_no_client_status`, `a_reaction_add_names_the_messages_author`, `a_reaction_from_the_bridge_without_an_author_has_no_field`; clippy `-D warnings` clean on `sr-events sr-gateway sr-api sr-features sr-server` (all targets). Vitest: `store/clientStatus.test.ts` (10: `clientProperties` for Android Chrome, iPhone, iPad as Mac, desktop Firefox in a narrow window, `userAgentData.mobile` wins, no UA string; `phoneOnly`; reducer keeps `client_status` through READY, `PRESENCE_UPDATE`, a member chunk, `SR_ACTIVITY_UPDATE`, drops malformed), `lounge/shell/desktopNotify.test.ts` (`reactionDecision`, every rule on and off), `lounge/__tests__/p68ReactionNotify.test.tsx` (fake `Notification`: once for my message, not for another's, not for my own reaction, title, tag, no sound, click target), `lounge/__tests__/p68PhoneGlyph.test.tsx` (the member row draws the glyph and its name, the dot returns with a web session), `packages/ui` `display.test.tsx` (status icon). Typecheck, eslint and prettier clean on my web files. e2e `e2e/tests/p68-presence-shots.spec.ts` (local only, `SR_P68_PRESENCE=1`), both tests pass on the real stack: a phone-user-agent context (Chromium `isMobile`) makes the Owner's member list show the glyph, a desktop tab of the same account turns it back into the dot, an afk phone shows the idle dot, the viewer sees their own glyph; a reaction on the Owner's message raises exactly one `Notification` (title, tag `react-<id>`), none for someone else's message. Note: Chromium's `userAgentData.mobile` is false under a UA-string override alone, so the contexts set `isMobile: true`; that is `clientProperties` working as specified.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p68-presence`: `memberlist-`, `glyph-closeup-`, `popout-`, `friends-dmlist-`, `friends-`, `dm-header-`, `self-` and `settings-notifications-`, `settings-push-card-`, each for desktop and phone in `lounge-editorial`, `lounge-light`, `lounge-dark`. Looked at: glyph visible and legible in the member list, the popout avatar, the friends list, the DM list (phone-only green handset, desktop green dot, idle-on-phone amber dot) and the DM header, in light and dark.

## Docs updated
`docs/05-api-and-gateway.md` (IDENTIFY `properties`, `client_status`, the glyph rule; `message_author_id` on `MESSAGE_REACTION_ADD`), `docs/specs/lounge-chat.md` section 10 (reaction notifications, digest withdrawn, phone glyph). No user-settings spec line named the digest.

## For files I do not own
- CHANGELOG (ops): "The member list, profiles, friends and direct messages show a phone glyph in place of the green dot for someone who is online only on a phone; reaction notifications now work (a desktop notification while the app is open when someone reacts to your message, no sound); the email digest switch left Settings until a digest exists."
- Dead i18n keys for staff: `settings.notifications.digest` (switch removed); `settings.notifications.pushTitle`, `pushDesc` and `pushUnavailable` (replaced by `pushTitleNow`, `pushDescNow`, `pushUnavailableNow` in my fragment, because the card's title, description and note named the email digest; the settings search entry for the card is now "Push notifications", no `digest` keyword).

## Proposed inventory rows
- SEC-AUD-206: the phone glyph: `lounge/phoneStatus.tsx`, `store/connect.ts` `clientProperties`, `packages/ui` Avatar `statusIcon`; closes LNG-PRF-013. Also record the gateway fix (announce again once the platform is known).
- SEC-AUD-221: reaction notifications with `message_author_id`; with 222 closes LNG-USET-052.
- SEC-AUD-222: the digest switch left the page; `emailDigest` kept, parsed and never read (comment in `prefs.ts`).

## Proposed threat-model rows
- The client platform is something others learn about a member (phone, web or desktop), only for an `online` member, empty for an invisible one; no device model or user agent is sent.
- `message_author_id` on `MESSAGE_REACTION_ADD`, delivered to every reader of the channel: it names the author of a message they can already see; nothing new is disclosed.

## Owner questions
- The set has only a handset icon; should a smartphone icon be added to `packages/ui` for the glyph?
- The notification body is empty when the message is not loaded; should the click or a fetch fill it? (Default: no fetch.)

## Clean-up
Scratch DBs `sr_p68_presence_ui` dropped (the unit tests used none); `p68-presence-target` and `p68-presence-bin` deleted.
