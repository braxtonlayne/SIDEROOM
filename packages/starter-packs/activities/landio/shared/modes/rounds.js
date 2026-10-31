// @ts-check
// Timed Rounds: five minutes, most land at the end wins (gold counts three times). Respawn as often as you like;
// the round's results show for a few seconds, then a fresh round starts on a fresh map.

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'rounds',
  name: 'Timed Rounds',
  blurb: 'Five minutes. Most land at the end wins.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
};
