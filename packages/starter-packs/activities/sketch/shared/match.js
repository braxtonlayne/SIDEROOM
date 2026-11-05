// @ts-check
// Guess matching (ACT-SKS-001): a guess counts when it is the prompt after normalising case, accents, punctuation,
// articles and spacing, with a small typo allowance that grows with the prompt's length. A guess one or two edits
// further away is "close" (told only to the guesser).

/** @param {string} s */
export function normalize(s) {
  return String(s ?? '')
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9 ]+/g, ' ')
    .split(/\s+/)
    .filter((w) => w !== '' && w !== 'a' && w !== 'an' && w !== 'the')
    .join(' ');
}

/** Levenshtein distance, stopping early once it is over `max`. @param {string} a @param {string} b @param {number} max */
export function distance(a, b, max = Infinity) {
  if (a === b) return 0;
  if (Math.abs(a.length - b.length) > max) return max + 1;
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const cur = [i];
    let best = i;
    for (let j = 1; j <= b.length; j++) {
      const v = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      cur.push(v);
      if (v < best) best = v;
    }
    if (best > max) return max + 1;
    prev = cur;
  }
  return prev[b.length];
}

/** Typos allowed for a normalised prompt of this length. @param {number} len */
export const allowance = (len) => (len >= 10 ? 2 : len >= 5 ? 1 : 0);

/**
 * @param {string} guess what a player typed
 * @param {string} prompt the secret prompt
 * @returns {'hit' | 'close' | 'miss'}
 */
export function judge(guess, prompt) {
  const g = normalize(guess);
  const p = normalize(prompt);
  if (g === '' || p === '') return 'miss';
  const ok = allowance(p.length);
  // The prompt without spaces matches too ("toiletpaper fort").
  const d = Math.min(distance(g, p, ok + 2), distance(g.replace(/ /g, ''), p.replace(/ /g, ''), ok + 2));
  if (d <= ok) return 'hit';
  if (d <= ok + 2 && p.length >= 4) return 'close';
  return 'miss';
}

/**
 * The hint shown to guessers: one underscore per letter or digit, spaces and punctuation kept, and the letters at
 * `shown` positions revealed.
 * @param {string} prompt @param {number[]} [shown]
 */
export function maskPrompt(prompt, shown = []) {
  const set = new Set(shown);
  return [...prompt].map((ch, i) => (/[\p{L}\p{N}]/u.test(ch) ? (set.has(i) ? ch : '_') : ch)).join('');
}

/** Positions of the prompt that can be revealed as hints (letters and digits). @param {string} prompt */
export function letterPositions(prompt) {
  /** @type {number[]} */
  const out = [];
  [...prompt].forEach((ch, i) => {
    if (/[\p{L}\p{N}]/u.test(ch)) out.push(i);
  });
  return out;
}
