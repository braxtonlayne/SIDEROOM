// The last "Watch together" destination used in each server (D-264): a per-device memory so the picker opens on
// where the group usually watches. Not account state. Every read is wrapped and a bad value is ignored, in the
// style of cinema/tv/livePrefs.ts.
const KEY = 'sr.watchparty.lastDest';

function readAll(): Record<string, string> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return {};
    const out: Record<string, string> = {};
    for (const [g, c] of Object.entries(v as Record<string, unknown>))
      if (typeof c === 'string' && c) out[g] = c;
    return out;
  } catch {
    return {};
  }
}

/** The channel last used for a party in this server, if any. */
export function getLastDest(guildId: string): string | null {
  return readAll()[guildId] ?? null;
}

export function setLastDest(guildId: string, channelId: string) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...readAll(), [guildId]: channelId }));
  } catch {
    /* best effort */
  }
}
