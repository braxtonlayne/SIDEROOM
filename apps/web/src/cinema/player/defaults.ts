// The player defaults Settings > Activity lets people change (LNG-USET-035). They live in the player's own
// localStorage record (`sr.cinema.player.v1`), so the player and the settings page read the same values; other
// fields in the record are left alone.
const KEY = 'sr.cinema.player.v1';

export interface PlayerDefaults {
  autoplayNext: boolean;
  dataSaver: boolean;
  /** Language code, or null to follow the file's default track. */
  subtitleLang: string | null;
}

export function readPlayerDefaults(): PlayerDefaults {
  let p: Partial<PlayerDefaults> = {};
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) p = JSON.parse(raw) as Partial<PlayerDefaults>;
  } catch {
    /* storage unavailable or damaged: defaults */
  }
  return {
    autoplayNext: p.autoplayNext !== false,
    dataSaver: p.dataSaver === true,
    subtitleLang: typeof p.subtitleLang === 'string' && p.subtitleLang ? p.subtitleLang : null,
  };
}

export function writePlayerDefaults(patch: Partial<PlayerDefaults>): PlayerDefaults {
  let all: Record<string, unknown> = {};
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) all = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    /* start over */
  }
  const next = { ...all, ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* best effort */
  }
  return readPlayerDefaults();
}
