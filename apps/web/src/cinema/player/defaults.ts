// The player defaults Settings > Activity lets people change (LNG-USET-035). They live in the player's own
// localStorage record (`sr.cinema.player.v1`), so the player and the settings page read the same values; other
// fields in the record are left alone.
const KEY = 'sr.cinema.player.v1';

export interface PlayerDefaults {
  autoplayNext: boolean;
  dataSaver: boolean;
  /** Language code, or null to follow the file's default track. */
  subtitleLang: string | null;
  /** Language code of the audio track to start with, or null for the file's default. */
  audioLang: string | null;
  /** Highest picture height to start with (480, 720 or 1080), or null for no cap. */
  qualityCap: number | null;
}

export const QUALITY_CAPS = [480, 720, 1080] as const;
export const capOf = (v: unknown): number | null =>
  typeof v === 'number' && (QUALITY_CAPS as readonly number[]).includes(v) ? v : null;

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
    audioLang: typeof p.audioLang === 'string' && p.audioLang ? p.audioLang : null,
    qualityCap: capOf(p.qualityCap),
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
