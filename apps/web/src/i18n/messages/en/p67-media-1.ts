// Wave 67, media (SEC-AUD-199, D-686): the Live TV channel logo row of the channel editor. Admin strings sit in this
// catalog until `scripts/staff-keys.mjs --write` moves what only staff code names. Two keys it stopped using were removed in wave 68 (SEC-AUD-217).
export default {
  // SEC-AUD-202 (TV-UI-010): the phone guide's swipe between channels.
  'cinema.agenda.selected': 'Showing channel {n}, {name}.',
} satisfies Record<string, string>;
