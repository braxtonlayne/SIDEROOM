# Wave 102 srv handoff (SEC-AUD-453, D-958): 5 raw selects in Server settings -> `NativeSelect`

Code builder of pod pages. STACK-ROUND: none (code builder). Nothing shot or measured by me; pageshots does that.

## REACH table (all five moved; the old class `ss-select` dropped on each, nothing else changed)
Area theme: Server settings is a Lounge page (main theme: Lounge Light / Lounge Dark / Editorial). Route: `/settings/server/<guildId>/<section id>` (members, integrations, community, bridge; as `p101-chan-shots` does). Sheet rule that styled them: `.ss-select` (`server-settings.css:222`, `min-height: 40px`, 44 px at <= 720 px `:733`, width 100 %). Label: `.ss-field-label` is `display:block` (`:237`), so none of the five is in a flex or inline-flex label.

| file | select | where / how to open | wrapper context |
|---|---|---|---|
| `sections/PruneDialog.tsx:85` | `#ss-prune-days` ("Inactive for") | Members section > "Prune" button (opens the dialog; `Members.tsx:360`). Earlier spec on Members: `p101-chan-shots`, `members-hit-test`; unit: `people.test.tsx` | `div` in `.ss-form` |
| `sections/Integrations.tsx:351` | `#ss-webhook-channel` ("Channel") | Integrations > Webhooks tab > create (or edit) a webhook: `WebhookDialog` modal. Needs >= 1 channel (the world has) | `div` in modal form |
| `sections/CommunityWizard.tsx:263` | component `ChannelPick` select (id passed in) ("Choose a channel") | Community section: switch "Enable Community" on while it is off (`Community.tsx:216`) opens `CommunityWizard`; selects appear on the steps that pick channels. Needs Community off in the world (do not finish the wizard: Close) | `div` |
| `sections/Community.tsx:91` (local `ChannelSelect`) | `#ss-rules-channel`, `#ss-updates-channel`, `#ss-safety-channel` (render sites `:228, :235, :242`, shown when Community is on) and `#ss-welcome-ch-<i>` (`:605`, welcome screen card, shown when a welcome channel row exists) | Community section. Rules/updates/safety render at `Community.tsx:228-248` in the overview form; welcome ones in the Welcome tab (add a channel row). Earlier unit: `p14communityMore.test.tsx` | `div` |
| `bridge/ConflictLog.tsx:109` | filter "Kind" (no id; inside `<label class=ss-field-label>`) | Bridge section, only when bridge mode is `whole_guild`: `BridgeSection.tsx:87`. Earlier spec `p101-mod-shots` fakes `/bridge/status` with `page.route`; a real one is not available on a `memory:` stack. Label is in `.ss-toolbar` (flex, `align-items:flex-end`) beside a `Switch` | label is a flex item, block label |

Also: the `Community.tsx` channel selects of the Community section need Community enabled (guild setting) to show the overview ones; if the world has it off, the spec fakes `GET .../community` or enables it through the API under its own data.

Who finds them by class or id: `.ss-select` is named in `e2e/tests/p84-guild-shots.spec.ts:234` (one entry in a list of region selectors for a clip) and `p85-guild-shots.spec.ts:384` (`:not(.ss-select)` on inputs, harmless). No `__tests__` file finds these five by class (the unit tests passed). Ids are kept. The `ss-select` rule stays in the sheet (Admin and other files use it).

## Chunks (git grep -n of importers)
All five are in the lazy `ServerSettings` chunk (`routes/settingsServer.tsx` imports `settings/server/ServerSettings` lazily; `ServerSettings.tsx` imports Integrations and Community; `Community.tsx` imports CommunityWizard; `Members.tsx` imports PruneDialog; `BridgeSection.tsx` imports ConflictLog). None is loaded by first paint or a public page. Nothing left raw, nothing put back.

## Checks (own tree, after the last edit; last edit 09:32)
- prettier --write then eslint on the five files: exit 0 / exit 0.
- vitest single files: `p14communityMore` (25 passed), `people`, `content`, `appDirectory`, `bridge/__tests__/sync` all passed (these render the five sections). Not run on base for failure comparison (no behaviour change expected).
- TYPECHECK: see below.

## Commits
See `git log fix/p102-srv`: one commit per file, subject `SEC-AUD-453 <file> (READY)`.

## SEC-AUD-453 text for the commander
Server settings: PruneDialog, Integrations, CommunityWizard, Community (ChannelSelect), ConflictLog moved to `NativeSelect`; no raw `<select>` remains in these five files.

## Changelog sentence
Server settings drop-downs (prune days, webhook channel, Community channels, bridge conflict filter) use the kit's select look with a full-size touch target on a phone.

## Owner questions
None. Note for shots: the kit select is 100 % wide inside its wrapper like `.ss-select` was; ConflictLog's select sits in a shrink-to-fit flex label, so its width may differ from the base: measure it.
