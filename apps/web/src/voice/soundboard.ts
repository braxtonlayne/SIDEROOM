/**
 * Soundboard data and playback plumbing (VOX-SB-002 / VOX-SB-003), over the REST shapes in
 * crates/sr-api/src/routes/settings/soundboard.rs (Discord's soundboard objects):
 *
 *   GET /soundboard-default-sounds                the eight default sounds (`sound_id` "1".."8")
 *   GET /guilds/{id}/soundboard-sounds            `{ items: [...] }`
 *   GET .../audio                                 the file as uploaded (MP3, Ogg or WAV, at most 5 s)
 *   POST /channels/{id}/send-soundboard-sound     announce a play (permission, voice state, cooldown)
 *   GUILD_SOUNDBOARD_SOUNDS_UPDATE / SOUNDBOARD_SOUNDS   `{ guild_id, soundboard_sounds: [...] }`
 *
 * The server never decodes a sound: the client mixes it into its own microphone stream (mic.ts), so everyone
 * hears it at their volume setting for that person, and plays it to itself too.
 */
import { create } from 'zustand';
import { apiFetch } from '../api/client';
import { rest, useStore } from '../store';
import type { UserSettingsBlob } from '../store';

export interface Sound {
  id: string;
  name: string;
  /** 0 to 1, the volume the sound was uploaded with. */
  volume: number;
  /** A unicode emoji, when the sound has one. */
  emoji: string | null;
  /** The server it belongs to; null for the default sounds. */
  guildId: string | null;
  available: boolean;
}

/** `default:1` or `<guild>:<id>`, unique across servers; used for favorites and the audio cache. */
export const soundKey = (s: Pick<Sound, 'id' | 'guildId'>) => `${s.guildId ?? 'default'}:${s.id}`;

export function parseSound(raw: unknown): Sound | null {
  const o = (raw ?? {}) as Record<string, unknown>;
  const id =
    typeof o.sound_id === 'string' || typeof o.sound_id === 'number' ? String(o.sound_id) : '';
  const name = typeof o.name === 'string' ? o.name : '';
  if (!id || !name) return null;
  return {
    id,
    name,
    volume: typeof o.volume === 'number' ? Math.min(1, Math.max(0, o.volume)) : 1,
    emoji: typeof o.emoji_name === 'string' && o.emoji_name ? o.emoji_name : null,
    guildId: typeof o.guild_id === 'string' ? o.guild_id : null,
    available: o.available !== false,
  };
}

const parseList = (raw: unknown): Sound[] =>
  (Array.isArray(raw) ? raw : []).map(parseSound).filter((s): s is Sound => !!s);

const STORAGE_KEY = 'sr.soundboard';

/** The synced settings key holding the favorites (H-45): sound keys, newest last, at most FAVORITES_MAX. */
export const FAVORITES_KEY = 'soundboardFavorites';
export const FAVORITES_MAX = 50;
/** A well-formed favorite: `default:<id>` or `<server id>:<id>` (P14-WEB-2). */
const FAVORITE_SHAPE = /^(default|[0-9]{1,20}):[0-9]{1,20}$/;

/**
 * Entries read from the settings blob or the old local list are kept only when they match
 * {@link FAVORITE_SHAPE}; the list is cut to the newest {@link FAVORITES_MAX}; and a server entry whose
 * server is not in `servers` (the person left it) is dropped. `servers` is null while the server list is
 * not known yet, which keeps every well-formed entry.
 */
export function cleanFavorites(raw: unknown, servers: ReadonlySet<string> | null): string[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((f): f is string => typeof f === 'string' && FAVORITE_SHAPE.test(f))
    .filter((f) => {
      const server = f.slice(0, f.indexOf(':'));
      return server === 'default' || !servers || servers.has(server);
    })
    .slice(-FAVORITES_MAX);
}

/** The servers the person is in, or null before the first READY (nothing is dropped then). */
function knownServers(): ReadonlySet<string> | null {
  const order = useStore.getState().guildOrder;
  return order.length ? new Set(order) : null;
}

/** True once the settings blob has been read; until then the localStorage list is the first paint. */
let favoritesAdopted = false;

interface Saved {
  favorites: string[];
  volume: number;
}

function load(): Saved {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<Saved>;
    return {
      favorites: cleanFavorites(raw.favorites, knownServers()),
      volume: typeof raw.volume === 'number' ? Math.min(100, Math.max(0, raw.volume)) : 100,
    };
  } catch {
    return { favorites: [], volume: 100 };
  }
}

/** Writes the per-device volume, plus the legacy favorites list only while the synced one is not known. */
function save(s: Saved): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(favoritesAdopted ? { volume: s.volume } : s));
  } catch {
    /* private mode: the choice lasts until the page closes */
  }
}

interface SoundboardState extends Saved {
  /** null until loaded. */
  defaults: Sound[] | null;
  guilds: Record<string, Sound[]>;
  toggleFavorite(key: string): void;
  setVolume(percent: number): void;
}

export const useSoundboard = create<SoundboardState>()((set, get) => ({
  ...load(),
  defaults: null,
  guilds: {},
  toggleFavorite: (key) => {
    const cur = get().favorites;
    const favorites = (cur.includes(key) ? cur.filter((k) => k !== key) : [...cur, key]).slice(
      -FAVORITES_MAX,
    );
    set({ favorites });
    save({ favorites, volume: get().volume });
    if (favoritesAdopted) writeFavorites(favorites);
  },
  setVolume: (percent) => {
    const volume = Math.min(100, Math.max(0, Math.round(percent)));
    set({ volume });
    save({ favorites: get().favorites, volume });
  },
}));

export const resetSoundboard = () => {
  favoritesAdopted = false;
  useSoundboard.setState({ defaults: null, guilds: {}, favorites: [], volume: 100 });
};

/** Saves the favorites into the settings blob and records the new version; a failure keeps the local state. */
function writeFavorites(list: string[]): void {
  void rest
    .patchUserSettings({ [FAVORITES_KEY]: list })
    .then((saved) => useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved))
    .catch(() => undefined);
}

/**
 * Takes the favorites from the settings blob. A blob without the key imports this device's old localStorage
 * list once (written up, then the local copy is dropped); a blob with it wins and the local list is ignored.
 */
export function adoptFavorites(blob: UserSettingsBlob | null): void {
  if (!blob) return;
  const raw = blob.settings[FAVORITES_KEY];
  if (Array.isArray(raw)) {
    const favorites = cleanFavorites(raw, knownServers());
    const cur = useSoundboard.getState().favorites;
    favoritesAdopted = true;
    if (favorites.length !== cur.length || favorites.some((f, i) => f !== cur[i])) {
      useSoundboard.setState({ favorites });
    }
    save({ favorites, volume: useSoundboard.getState().volume });
    return;
  }
  const legacy = load().favorites;
  favoritesAdopted = true;
  if (legacy.length) {
    useSoundboard.setState({ favorites: legacy });
    writeFavorites(legacy);
  }
  const { favorites, volume } = useSoundboard.getState();
  save({ favorites, volume });
}

/** Follows the settings blob (loading it when this session has not): call on mount, call the result on unmount. */
export function startFavoritesSync(): () => void {
  let last = useStore.getState().userSettings;
  let lastOrder = useStore.getState().guildOrder;
  adoptFavorites(last);
  if (!last) {
    rest
      .fetchUserSettings()
      .then((blob) => useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob))
      .catch(() => undefined);
  }
  return useStore.subscribe((st) => {
    // A change of the server list re-reads the blob, so favorites of a server just left drop out.
    if (st.userSettings === last && st.guildOrder === lastOrder) return;
    last = st.userSettings;
    lastOrder = st.guildOrder;
    adoptFavorites(last);
  });
}

/** Load the default sounds once and a server's own sounds (again each time, so a new upload shows). */
export async function loadSounds(guildId: string | null): Promise<void> {
  const jobs: Promise<void>[] = [];
  if (!useSoundboard.getState().defaults) {
    jobs.push(
      apiFetch<unknown[]>('/soundboard-default-sounds').then((raw) => {
        useSoundboard.setState({ defaults: parseList(raw) });
      }),
    );
  }
  if (guildId && guildId !== '@me') {
    jobs.push(
      apiFetch<{ items?: unknown[] }>(`/guilds/${guildId}/soundboard-sounds`).then((raw) => {
        useSoundboard.setState((s) => ({
          guilds: { ...s.guilds, [guildId]: parseList(raw.items).map((x) => ({ ...x, guildId })) },
        }));
      }),
    );
  }
  await Promise.all(jobs);
}

/** Gateway dispatches that carry a server's whole list. */
export function applySoundboardEvent(event: string, data: unknown): void {
  if (event !== 'GUILD_SOUNDBOARD_SOUNDS_UPDATE' && event !== 'SOUNDBOARD_SOUNDS') return;
  const d = (data ?? {}) as Record<string, unknown>;
  const guildId = typeof d.guild_id === 'string' ? d.guild_id : null;
  if (!guildId) return;
  const list = parseList(d.soundboard_sounds).map((s) => ({ ...s, guildId }));
  useSoundboard.setState((s) => ({ guilds: { ...s.guilds, [guildId]: list } }));
}

// ---- audio ----

export type AudioFetcher = (url: string) => Promise<ArrayBuffer>;

const browserFetcher: AudioFetcher = async (url) => {
  const res = await fetch(`/api/v1${url}`, { credentials: 'include' });
  if (!res.ok) throw new Error(`sound audio ${res.status}`);
  return res.arrayBuffer();
};

let fetcher: AudioFetcher = browserFetcher;
/** Tests: replace how audio files are fetched. */
export function setAudioFetcher(f: AudioFetcher | null): void {
  fetcher = f ?? browserFetcher;
  cache.clear();
}

const cache = new Map<string, ArrayBuffer>();
const CACHE_MAX = 32;

export const audioUrl = (s: Pick<Sound, 'id' | 'guildId'>) =>
  s.guildId
    ? `/guilds/${s.guildId}/soundboard-sounds/${s.id}/audio`
    : `/soundboard-default-sounds/${s.id}/audio`;

/** The sound's file, kept for the next play (a few dozen at most). A copy: decoding detaches the buffer. */
export async function soundAudio(s: Pick<Sound, 'id' | 'guildId'>): Promise<ArrayBuffer> {
  const key = soundKey(s);
  let buf = cache.get(key);
  if (!buf) {
    buf = await fetcher(audioUrl(s));
    if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value as string);
    cache.set(key, buf);
  }
  return buf.slice(0);
}

/** What a refused play means to the person, by status. */
export function playErrorKind(
  status: number,
): 'permission' | 'silenced' | 'cooldown' | 'gone' | 'other' {
  switch (status) {
    case 403:
      return 'permission';
    case 409:
      return 'silenced';
    case 429:
      return 'cooldown';
    case 404:
      return 'gone';
    default:
      return 'other';
  }
}

export interface SendSoundBody {
  sound_id: string;
  source_guild_id?: string;
}

/** `POST /channels/{id}/send-soundboard-sound`: the gateway checks permission, voice state and cooldown. */
export const sendSoundboardSound = (channelId: string, body: SendSoundBody) =>
  apiFetch<void>(`/channels/${channelId}/send-soundboard-sound`, {
    method: 'POST',
    json: body,
    retries: 0,
  });
