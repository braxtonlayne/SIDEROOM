// ARC-GST-002: the name a guest plays under. A random callsign by default (adjective + noun + number, nothing a
// person could mistake for a real handle); a nickname the guest typed is only a request that the lobby may refuse.
const ADJECTIVES = [
  'Amber',
  'Brisk',
  'Calm',
  'Dapper',
  'Eager',
  'Fuzzy',
  'Gentle',
  'Hasty',
  'Jolly',
  'Keen',
  'Lucky',
  'Mellow',
  'Nimble',
  'Plucky',
  'Quiet',
  'Rowdy',
  'Sunny',
  'Tidy',
  'Vivid',
  'Witty',
];
const NOUNS = [
  'Otter',
  'Falcon',
  'Lantern',
  'Pebble',
  'Comet',
  'Badger',
  'Maple',
  'Nebula',
  'Puffin',
  'Quill',
  'Raccoon',
  'Sparrow',
  'Thistle',
  'Walrus',
  'Yarrow',
  'Zephyr',
  'Beacon',
  'Cricket',
  'Marble',
  'Ember',
];

const pick = <T>(list: readonly T[], r: number): T => list[r % list.length]!;

/** `Mellow Otter 42`; `rng` is a test seam (32-bit unsigned). */
export function randomCallsign(
  rng: () => number = () => crypto.getRandomValues(new Uint32Array(1))[0]!,
): string {
  return `${pick(ADJECTIVES, rng())} ${pick(NOUNS, rng())} ${10 + (rng() % 90)}`;
}

export const NICKNAME_MAX = 24;

/** A nickname to ask for, or `null` when the typed text is empty or unusable (letters, digits, space, . _ -). */
export function cleanNickname(raw: string): string | null {
  const s = raw.normalize('NFKC').replace(/\s+/g, ' ').trim();
  if (!s || [...s].length > NICKNAME_MAX) return null;
  return /^[\p{L}\p{N} ._-]+$/u.test(s) ? s : null;
}
