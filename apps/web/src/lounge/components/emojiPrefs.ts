/**
 * Per-browser emoji picker state (LNG-EXP-007): how often each emoji was picked, favorites and the
 * skin tone. Kept in localStorage; every access is guarded because storage can be unavailable.
 */
const KEY = 'sr.emoji.prefs';
const MAX_FREQUENT = 16;
const MAX_COUNTED = 200;

export interface EmojiPrefs {
  /** Key (unicode text or `name:id`) -> times picked. */
  counts: Record<string, number>;
  favorites: string[];
  /** 0 = default yellow, 1..5 = Fitzpatrick types 1-2 to 6. */
  tone: number;
}

export const emptyPrefs = (): EmojiPrefs => ({ counts: {}, favorites: [], tone: 0 });

export function loadEmojiPrefs(): EmojiPrefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return emptyPrefs();
    const p = JSON.parse(raw) as Partial<EmojiPrefs>;
    return {
      counts: typeof p.counts === 'object' && p.counts ? p.counts : {},
      favorites: Array.isArray(p.favorites) ? p.favorites.filter((f) => typeof f === 'string') : [],
      tone: typeof p.tone === 'number' && p.tone >= 0 && p.tone <= 5 ? p.tone : 0,
    };
  } catch {
    return emptyPrefs();
  }
}

export function saveEmojiPrefs(p: EmojiPrefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    /* kept for this session only */
  }
}

/** Count a pick; the table is trimmed to the most used entries. */
export function recordPick(p: EmojiPrefs, key: string): EmojiPrefs {
  const counts = { ...p.counts, [key]: (p.counts[key] ?? 0) + 1 };
  const keys = Object.keys(counts);
  if (keys.length > MAX_COUNTED) {
    keys
      .sort((a, b) => (counts[a] ?? 0) - (counts[b] ?? 0))
      .slice(0, keys.length - MAX_COUNTED)
      .forEach((k) => delete counts[k]);
  }
  return { ...p, counts };
}

/** The most picked emoji, most used first (ties keep insertion order). */
export function frequent(p: EmojiPrefs): string[] {
  return Object.entries(p.counts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, MAX_FREQUENT)
    .map(([k]) => k);
}

export function toggleFavorite(p: EmojiPrefs, key: string): EmojiPrefs {
  return {
    ...p,
    favorites: p.favorites.includes(key)
      ? p.favorites.filter((f) => f !== key)
      : [...p.favorites, key],
  };
}

const TONES = ['', '\u{1F3FB}', '\u{1F3FC}', '\u{1F3FD}', '\u{1F3FE}', '\u{1F3FF}'];

/** Apply a skin tone modifier right after the base character. */
export function withTone(emoji: string, tone: number): string {
  const mod = TONES[tone];
  if (!mod) return emoji;
  const [first, ...rest] = Array.from(emoji.replace(/️/g, ''));
  return first ? first + mod + rest.join('') : emoji;
}
