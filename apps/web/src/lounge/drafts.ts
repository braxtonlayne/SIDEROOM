/**
 * Composer drafts, one per channel, kept in localStorage so they survive a reload (Q171; Discord keeps drafts
 * on the device). Storage can be unavailable (private mode, quota): drafts then live for the session only.
 */
const KEY = 'sr.lounge.drafts';

let cache: Record<string, string> | null = null;

function load(): Record<string, string> {
  if (cache) return cache;
  try {
    const raw = localStorage.getItem(KEY);
    const v: unknown = raw ? JSON.parse(raw) : {};
    cache = v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, string>) : {};
  } catch {
    cache = {};
  }
  return cache;
}

function save(all: Record<string, string>) {
  cache = all;
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* not persisted; the in-memory copy still serves this session */
  }
}

export const drafts = {
  get(channelId: string): string | undefined {
    return load()[channelId];
  },
  set(channelId: string, value: string) {
    const all = { ...load() };
    if (value) all[channelId] = value;
    else delete all[channelId];
    save(all);
  },
  delete(channelId: string) {
    const all = { ...load() };
    delete all[channelId];
    save(all);
  },
  clear() {
    cache = null;
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
  },
};
