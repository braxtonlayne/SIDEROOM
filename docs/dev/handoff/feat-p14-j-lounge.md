# Handoff: feat/p14-j-lounge (J-01 to J-05, J-10, J-11)

Branch `feat/p14-j-lounge`, based on `integration/phase14`. Local commits only. Items are from [answers-followups-p14.md](../answers-followups-p14.md) with D-243 to D-245.

## Done

| Item | What |
|---|---|
| J-02 incident actions and auto-pause | Migration `0960_guild_incident_actions.sql`; `PUT /guilds/{id}/incident-actions` (`MANAGE_GUILD`, `MfaGate`, bounds, `GUILD_UPDATE` audit and dispatch); `incidents_data` on the guild object and in `GUILD_UPDATE`; `raid_auto_pause_invites` on `GET\|PATCH /sr/community`; invite accept, vanity and directory join refuse while `invites_disabled_until` is in the future; `dm_access_protected` refuses with `50007` when every shared server has `dms_disabled_until` running (`SharedProtection.dms_paused_all`); the join-rate alert (new: nothing evaluated `raid_alert_joins_per_minute` before) runs after every join in `routes/settings/alerts.rs`. Web: Security Actions dialog rewritten (two 24 hour switches, the indefinite pause as its own switch, the raid-alert switch), "Invites paused until <time>" under the server name. |
| J-03 AutoMod alert messages | Message type 24 (`MessageType::AutoModerationAction`), card builders and press handling in `routes/settings/alerts.rs`, `automod_alerts` table (migration `0966_automod_alerts.sql`), `POST /guilds/{id}/alert-actions`, `MessageRepository::replace_card`. Web: type 24 renders as a normal message, alert buttons and the Timeout select go to the new route. |
| J-04 member flags | Migration `0961_guild_member_flags.sql`; `sr_core::member_flags`; `MemberObject.flags`; `post_onboarding_responses` sets `COMPLETED_ONBOARDING` and dispatches `GUILD_MEMBER_UPDATE`. Web: the `localStorage` memory is gone, the modal condition is "flag clear and (joined within 3 days or rules just accepted)", a "Channels & Roles" row opens the questions for anyone without the flag. |
| J-05 integrations | `GET /guilds/{id}/integrations`, `DELETE /guilds/{id}/integrations/{id}` (`routes/developer/integrations.rs`, `AppsService::guild_integrations` and `uninstall`, `AppsRepository::uninstall`), error `10005 UnknownIntegration`. Web: the Bots list reads the route, Remove calls `DELETE`. |
| J-10 command permission overrides | The three permission routes and the guild command list accept the application's owner, a `MANAGE_GUILD` session and the scoped bearer; writes audit action 121. Web: `BotCommandPermissions.tsx` (Commands button on each installed application). |
| J-11 `video_quality_mode` | Migration `0964_channel_video_quality.sql`; channel object `video_quality_mode` and `rtc_region` (null, non-null refused), create and `PATCH`; join token `video_quality`, SFU `Flags::video_full`, `cap_of` in `sr-voice`. Web: Video Quality switch in the channel editor. |
| J-01 per-server DM toggle | `ServerDmToggle.tsx` over `server_dm_overrides`, in the server menu's Privacy Settings and the per-server profile dialog. |

Docs: doc 05 (one paragraph for all of the above), doc 09 section 3 (the DM block), spec `lounge-voice-video.md` section 8 (video quality), inventory LNG-GLD-025, LNG-SSET-047 and LNG-SSET-062 ticked, LNG-CHN-017 note updated (still open for per-channel module switches).

## Found on the way

- **The message object never carried `components`.** `MessageObject` had no such field, so no client saw buttons or selects on any message (staff cards, bot messages). It is now serialised (omitted when empty). `Component` also gained `placeholder` and string-select `options`, which were dropped on parse before.
- `MemoryBackend` (the in-memory message store) now implements `GuildAdminRepository` so the settings endpoint tests can drive kick, ban and timeout; no production code.

## Decisions and deviations

- J-05 says audit action 81 for `INTEGRATION_DELETE`. Discord numbers it 82 (80 create, 81 update, 82 delete) and the repo's enum agrees; 82 is written.
- Embed field names are readable labels (Rule, Trigger, Channel, Member, Matched content, Blocked), not the identifiers of the brief.
- The join-rate alert does not fire again within 10 minutes of the last one (`RAID_ALERT_COOLDOWN_MS`); the brief set no cooldown.
- Security Actions no longer toggles `dm_spam_filter` (D-243: a pause blocks, the standing filter of D-224 stays in Server Settings > Protection).
- The alert id is the id of the alert message. A blocked message has no Delete message button (nothing was stored).
- The reason `AutoMod alert: <rule>` rides on the request's ambient audit reason, so the kick, ban and timeout rows carry it. A message delete writes its row through the messages deps with the same ambient reason when those deps have an audit sink (production does).
- The video quality mode is fixed when a member joins (like the tier); a change applies from the next join.
- `GET /applications/{app}/guilds/{guild}/commands` takes `include_global=true` (a SIDEROOM extension) so the permission editor can list global commands too. The OAuth route policy is unchanged (bearer only for the two PUTs).
- Skipping the onboarding questions is remembered in memory for the page visit only (D-245: no browser-side memory).

## Tests run

Filled in at the end of the work, see the final report of the agent.

## Not done / open

- Playwright flows were not run (ports are shared by the agents).
- A Postgres-backed API test of the alert buttons (kick, ban and timeout run on the in-memory stores in the endpoint tests; the Postgres repos are covered by `crates/sr-db/tests/p14_j_lounge.rs`).
- Owner questions: none blocking. The 10 minute cooldown and the labels above are the defaults to confirm.
