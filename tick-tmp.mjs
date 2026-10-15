import fs from 'node:fs';
const p = 'docs/inventory/09-feature-modules.md';
let s = fs.readFileSync(p, 'utf8');
const notes = {
  'FTR-SCH-001': 'sr-userfeat scheduled.rs + tick.rs, routes userfeat/scheduled.rs, composer clock button (features/ScheduleDialog.tsx), list/edit/cancel in the saved-items panel; tests sr-userfeat/tests/db.rs, sr-server userfeat_pg_tests.rs',
  'FTR-REM-001': 'reminders.rs, DM delivery by the userfeat.tick job, /remind and Remind Me via POST /users/@me/reminders, reminders tab; db.rs tests',
  'FTR-BKM-001': 'bookmarks.rs, PUT /channels/{id}/messages/{id}/bookmark, bookmarks tab with notes; db.rs tests',
  'FTR-BDAY-001': 'BACKEND ONLY: birthdays.rs, /users/@me/birthday, daily announcement + 24h role; no settings UI yet',
  'FTR-LVL-001': 'BACKEND ONLY: leveling.rs, LevelingHook, rank/leaderboard/opt-out routes; no leaderboard page yet',
  'FTR-RSS-001': 'BACKEND ONLY: rss.rs (RSS/Atom parser, sr_net::SafeClient fetch, seeding, backoff), /guilds/{id}/rss-feeds; no feed manager UI yet',
  'FTR-LIBF-001': 'announcers.rs run_library_feed (cursor per server, kind/genre filters, age-restricted skipped); configured via Server Settings > Features',
  'FTR-WPA-001': 'announcers.rs announce_party + PartyAnnouncer ChatHook wired on PartyService; role ping option',
  'FTR-POLLT-001': 'BACKEND ONLY: poll_templates.rs and /guilds/{id}/poll-templates; no poll modal picker yet',
  'FTR-SUGG-001': 'BACKEND ONLY: suggestions.rs (card, seeded votes, status edits), /guilds/{id}/suggestions; no UI yet',
  'FTR-TICK-001': 'BACKEND ONLY: tickets.rs + routes (private thread via ThreadService, staff added); no UI yet, route path untested end to end',
};
for (const [id, n] of Object.entries(notes)) {
  s = s.replace(new RegExp(`- \[ \] \*\*${id}\*\* (.*)`), (_, rest) => `- [x] **${id}** ${rest} _${n}_`);
}
fs.writeFileSync(p, s);
