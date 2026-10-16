# Handoff: feat/p14-lounge-rest (Lounge server settings and servers/channels, remaining screens)

Branch `feat/p14-lounge-rest`, based on `integration/phase14` (= main). Local commits only. Inventories: [06-lounge-server-settings.md](../../inventory/06-lounge-server-settings.md) and [04-lounge-servers-channels.md](../../inventory/04-lounge-servers-channels.md). No migration, no new route and no OpenAPI change: every backend this needed already existed from phases 10 to 13, so this is all web. The route matrix, OpenAPI file, generated client and mocks are unchanged on purpose.

## Audit result

Most unticked boxes were screens over finished backends. Stale boxes ticked with a note naming the file: LNG-SSET-009 (card counts), LNG-SSET-048 (the Apps tab is the App Directory), LNG-SSET-049 (Features), LNG-SSET-072 (delete server with typed name and `withReauth`), LNG-GLD-026 (`GuildProfileDialog.tsx`), LNG-GLD-028 (`DirectoryDialog.tsx`). Two notes were wrong and are corrected: the API can change a text channel to an announcement channel (`type` 0 and 5), and the rail never drew server icons because `toLoungeGuild` set `iconUrl: null`.

## Done

Server Settings (all in `apps/web/src/settings/server`):
- **Community tabs** (`sections/CommunityMore.tsx`, `apiMore.ts`) replace the "More / not available" tab: **Server Guide** editor (LNG-SSET-066), **Rules Screening** editor (041), **Access** with how-people-get-in summary, application form editor and approve/reject queue with reason (039, 042), **Insights** with totals, per-day bars, top channels (068), **Directory** listing request with status and staff note (069).
- **Soundboard tab** in Expressions (`sections/SoundboardTab.tsx`, LNG-SSET-025): play, rename, volume, upload, delete.
- **Integrations**: Overview (counts, bots with Remove) and Followed Channels tab with Unfollow (`sections/IntegrationsOverview.tsx`, 044, 046). Followers no longer appear in the Webhooks list.
- **Features search** (049), **member audit history** in the details dialog (030, uses `audit-logs?target_id=`), **icon crop modal** (005, reuses the user-settings `ImageCropDialog`), server card separator bug ("00b7" printed literally).
- **Channel editor** (LNG-CHN-017 partial): announcement toggle, voice user limit and bitrate, followed channels list in the Integrations tab.

Lounge (all in `apps/web/src/lounge`):
- **Join pipeline** (`shell/JoinGate.tsx`, `shell/joinPipeline.ts`): rules dialog for a pending member (GLD-030), onboarding questions (GLD-031), Browse Channels (GLD-032, `BrowseChannelsDialog.tsx`).
- **Server Guide** dialog (GLD-033), **Security Actions** dialog with pause invites and pause DMs (GLD-034), **Create Server icon step with crop** (GLD-017), **server banner** in the channel list header and rail icons (GLD-019), **Help menu** with the shortcut sheet and support page (CHN-018), **forum card reaction count** (FRM-003).

## Still open, with reasons

| ID | Reason |
|---|---|
| LNG-SSET-007 traits | no backend field; owner decision on what traits are |
| LNG-SSET-032 | two-colour role gradient not backed by the role API |
| LNG-SSET-036 View Server As Role | needs the channel visibility computation for a chosen role |
| LNG-SSET-047 | command permission routes are application-owner only; a server manager has no route |
| LNG-SSET-050, 051 | no extension/theme packages, no per-server watch party or library settings in the backend |
| LNG-SSET-053 | alert threshold and Pause invites exist; auto-lockdown needs a backend job |
| LNG-SSET-062 | the engine emits an event only; no alert message is posted into the alert channel, so there is nothing to render |
| LNG-GLD-025 | no per-server privacy API |
| LNG-GLD-029 lurk mode | a non-member cannot read channels; needs a preview API |
| LNG-CHN-010 | voice status text and stream preview need backend support |
| LNG-CHN-017 | video quality, region, per-channel feature-module switches |

## Decisions and conservative defaults

- **"Pause DMs" in Security Actions is the server DM filter** (messages from members go to Message Requests, D-224), not a hard block.
- **Onboarding is offered once**, to a member who joined in the last 3 days (or right after the rules). "Answered" is remembered in `localStorage` per user and server because the API keeps no flag. A member on a new browser who joined recently sees it again.
- **Bots list** reads the first 1000 members and keeps `user.bot`; there is no installed-apps endpoint for a guild.
- **Create Server icon** is set with a `PATCH /guilds/{id}` right after creation because `POST /guilds` ignores `icon`.
- **Soundboard upload** refuses a file over 5 s with its length rather than trimming (server stores files as uploaded, D-201).

## Open owner questions

1. Should "no DMs from this server" (LNG-GLD-025) block, or send to Message Requests like the server filter?
2. Should auto-lockdown (LNG-SSET-053) pause invites by itself when the join-rate alert fires?
3. Should AutoMod alerts (LNG-SSET-062) become a real message in the alert channel, with Delete / Timeout / Kick / Ban buttons?
4. Add an API flag for "member finished onboarding" so the dialog is not browser-bound?

## Tests run

`apps/web`: `tsc --noEmit`, `eslint`, `prettier`, vitest for `src/settings/server`, `src/lounge`, `src/i18n` (see the final report for the exact counts), new files `settings/server/__tests__/p14communityMore.test.tsx` (guide, rules, access/applications, insights, directory, audit history, features search, integrations overview, followers, soundboard, channel editor) and `lounge/__tests__/p14JoinPipeline.test.tsx` (screening, onboarding, guide, security actions, create-server icon, browse channels, help menu, banner). Existing tests updated: `content.test.tsx` (Integrations now opens on Overview), `overview.test.tsx` (crop step), `p10LoungeFe.test.tsx` (reaction count). i18n fragments `p14-lounge-rest-1.ts` and `-2.ts`.
