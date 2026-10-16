import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { VoiceController, type ControllerDeps } from '../controller';
import type { EngineEvents, VoiceEngine } from '../engine';
import { resetSession, useVoiceSession } from '../session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../settings';
import {
  adoptFavorites,
  applySoundboardEvent,
  audioUrl,
  loadSounds,
  parseSound,
  playErrorKind,
  resetSoundboard,
  setAudioFetcher,
  soundAudio,
  soundKey,
  startFavoritesSync,
  useSoundboard,
  type Sound,
} from '../soundboard';
import { fakeAudio } from './fakes';

const DEFAULTS = [
  {
    name: 'Chime',
    sound_id: '1',
    volume: 1,
    emoji_id: null,
    emoji_name: '\u{1F514}',
    available: true,
  },
  { name: 'Ding', sound_id: '2', volume: 1, emoji_id: null, emoji_name: null, available: true },
];
const GUILD = [
  {
    name: 'Airhorn',
    sound_id: '77',
    volume: 0.5,
    emoji_id: null,
    emoji_name: null,
    guild_id: 'g1',
    available: true,
  },
];

beforeEach(() => {
  localStorage.clear();
  resetSoundboard();
  setAudioFetcher(null);
});
afterEach(() => setAudioFetcher(null));

describe('sound data', () => {
  it('reads Discord soundboard objects and drops broken ones', () => {
    expect(parseSound(GUILD[0])).toEqual({
      id: '77',
      name: 'Airhorn',
      volume: 0.5,
      emoji: null,
      guildId: 'g1',
      available: true,
    });
    expect(parseSound(DEFAULTS[0])!.emoji).toBe('\u{1F514}');
    expect(parseSound({ sound_id: '1' })).toBeNull();
    expect(parseSound(null)).toBeNull();
    expect(parseSound({ ...DEFAULTS[0], volume: 7 })!.volume).toBe(1);
    expect(soundKey({ id: '1', guildId: null })).toBe('default:1');
    expect(soundKey({ id: '77', guildId: 'g1' })).toBe('g1:77');
    expect(audioUrl({ id: '1', guildId: null })).toBe('/soundboard-default-sounds/1/audio');
    expect(audioUrl({ id: '77', guildId: 'g1' })).toBe('/guilds/g1/soundboard-sounds/77/audio');
  });

  it('loads the default sounds once and a server list each time', async () => {
    let defaultCalls = 0;
    server.use(
      http.get('*/api/v1/soundboard-default-sounds', () => {
        defaultCalls++;
        return HttpResponse.json(DEFAULTS);
      }),
      http.get('*/api/v1/guilds/g1/soundboard-sounds', () => HttpResponse.json({ items: GUILD })),
    );
    await loadSounds('g1');
    await loadSounds('g1');
    expect(defaultCalls).toBe(1);
    const s = useSoundboard.getState();
    expect(s.defaults!.map((x) => x.name)).toEqual(['Chime', 'Ding']);
    expect(s.guilds.g1!.map((x) => x.name)).toEqual(['Airhorn']);
    await loadSounds('@me');
    expect(Object.keys(useSoundboard.getState().guilds)).toEqual(['g1']);
  });

  it('follows the gateway when a server changes its sounds', () => {
    applySoundboardEvent('GUILD_SOUNDBOARD_SOUNDS_UPDATE', {
      guild_id: 'g2',
      soundboard_sounds: [{ ...GUILD[0], guild_id: undefined, sound_id: '5', name: 'Horn' }],
    });
    expect(useSoundboard.getState().guilds.g2).toEqual([
      expect.objectContaining({ id: '5', name: 'Horn', guildId: 'g2' }),
    ]);
    applySoundboardEvent('SOUNDBOARD_SOUNDS', { guild_id: 'g2', soundboard_sounds: [] });
    expect(useSoundboard.getState().guilds.g2).toEqual([]);
    applySoundboardEvent('MESSAGE_CREATE', { guild_id: 'g3' });
    applySoundboardEvent('SOUNDBOARD_SOUNDS', { soundboard_sounds: [] });
    expect(Object.keys(useSoundboard.getState().guilds)).toEqual(['g2']);
  });

  it('keeps favorites and the volume across reloads', () => {
    const s = useSoundboard.getState();
    s.toggleFavorite('default:1');
    s.toggleFavorite('g1:77');
    s.toggleFavorite('default:1');
    s.setVolume(40);
    s.setVolume(900);
    s.setVolume(40);
    expect(JSON.parse(localStorage.getItem('sr.soundboard')!)).toEqual({
      favorites: ['g1:77'],
      volume: 40,
    });
  });

  it('caches a file and hands out copies, because decoding detaches a buffer', async () => {
    const fetcher = vi.fn(async () => new Uint8Array([1, 2, 3]).buffer);
    setAudioFetcher(fetcher);
    const a = await soundAudio({ id: '1', guildId: null });
    const b = await soundAudio({ id: '1', guildId: null });
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(a).not.toBe(b);
    expect(new Uint8Array(b)).toEqual(new Uint8Array([1, 2, 3]));
  });

  it('maps statuses to what the person needs to hear', () => {
    expect([403, 409, 429, 404, 500].map(playErrorKind)).toEqual([
      'permission',
      'silenced',
      'cooldown',
      'gone',
      'other',
    ]);
  });
});

class SoundEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'connected';
  played: { data: ArrayBuffer; volume: number }[] = [];
  mixed = true;
  constructor(readonly events: EngineEvents) {}
  async connect() {}
  disconnect() {}
  setTransmitting() {}
  setDeafened() {}
  setUserVolume() {}
  setUserMuted() {}
  setMasterVolume() {}
  setInputVolume() {}
  async setInputDevice() {}
  async setOutputDevice() {}
  restartIce() {}
  async playSound(data: ArrayBuffer, volume: number) {
    this.played.push({ data, volume });
    return this.mixed;
  }
}

describe('playing into the call', () => {
  const sound: Sound = {
    id: '77',
    name: 'Airhorn',
    volume: 0.5,
    emoji: null,
    guildId: 'g1',
    available: true,
  };
  const deft: Sound = {
    id: '1',
    name: 'Chime',
    volume: 1,
    emoji: null,
    guildId: null,
    available: true,
  };

  function harness(sendSound: ControllerDeps['sendSound'], guildId = 'g1') {
    const listeners = new Set<(e: string, d: unknown) => void>();
    const engines: SoundEngine[] = [];
    const world = fakeAudio();
    const deps: ControllerDeps = {
      createEngine: (ev) => {
        const e = new SoundEngine(ev);
        engines.push(e);
        return e;
      },
      sendGateway: () => true,
      subscribe: (l) => (listeners.add(l), () => listeners.delete(l)),
      currentUserId: () => 'me',
      voiceStatesOf: () => ({}),
      audio: world.deps,
      now: () => 0,
      ...(sendSound ? { sendSound } : {}),
    };
    const c = new VoiceController(deps);
    const emit = (e: string, d: unknown) => listeners.forEach((l) => l(e, d));
    c.join({ guildId, channelId: 'c1', name: 'General' });
    emit('VOICE_STATE_UPDATE', { guild_id: guildId, channel_id: 'c1', user_id: 'me' });
    emit('VOICE_SERVER_UPDATE', {
      guild_id: guildId,
      channel_id: 'c1',
      endpoint: '/v',
      token: 't',
    });
    return { c, engines, world };
  }

  beforeEach(() => {
    resetSession();
    useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
    setAudioFetcher(async () => new Uint8Array([9, 9, 9]).buffer);
    // The test engine's connect resolves at once: the session is connected.
  });

  it('announces the play, mixes the file at the sound and soundboard volumes and plays it to you', async () => {
    useSoundboard.getState().setVolume(50);
    const send = vi.fn(async () => undefined);
    const { c, engines, world } = harness(send);
    await vi.waitFor(() => expect(useVoiceSession.getState().status).toBe('connected'));
    const r = await c.playSoundboard(sound);
    expect(r).toEqual({ ok: true });
    expect(send).toHaveBeenCalledWith('c1', { sound_id: '77' });
    expect(engines[0]!.played).toHaveLength(1);
    expect(engines[0]!.played[0]!.volume).toBeCloseTo(0.25);
    expect(new Uint8Array(engines[0]!.played[0]!.data)).toEqual(new Uint8Array([9, 9, 9]));
    // The sender hears it too, through their own output.
    await vi.waitFor(() =>
      expect(world.contexts.some((x) => x.bufferSources.length > 0)).toBe(true),
    );
  });

  it('names the source server for a sound that is not the channel own', async () => {
    const send = vi.fn(async () => undefined);
    const { c } = harness(send);
    await vi.waitFor(() => expect(useVoiceSession.getState().status).toBe('connected'));
    await c.playSoundboard({ ...sound, guildId: 'other' });
    await c.playSoundboard(deft);
    expect(send).toHaveBeenNthCalledWith(1, 'c1', { sound_id: '77', source_guild_id: 'other' });
    expect(send).toHaveBeenNthCalledWith(2, 'c1', { sound_id: '1' });
  });

  it('plays nothing and says why when the gateway refuses', async () => {
    for (const [status, text] of [
      [403, /cannot use the soundboard/],
      [409, /Unmute and undeafen/],
      [429, /Wait a moment/],
      [404, /no longer available/],
      [500, /could not be played/],
    ] as const) {
      const send = vi.fn(async () => {
        throw new ApiError(status, 'nope');
      });
      const { c, engines } = harness(send);
      await vi.waitFor(() => expect(useVoiceSession.getState().status).toBe('connected'));
      const r = await c.playSoundboard(sound);
      expect(r.ok).toBe(false);
      expect(r.ok === false && r.message).toMatch(text);
      expect(engines.at(-1)!.played).toHaveLength(0);
      c.leave();
    }
  });

  it('refuses outside a server call and when the file cannot be mixed', async () => {
    const idle = new VoiceController({
      createEngine: (ev) => new SoundEngine(ev),
      sendGateway: () => true,
      subscribe: () => () => undefined,
      currentUserId: () => 'me',
      voiceStatesOf: () => ({}),
      audio: fakeAudio().deps,
      now: () => 0,
    });
    expect(await idle.playSoundboard(sound)).toEqual({
      ok: false,
      message: 'Join a server voice channel to play a sound.',
    });
    const send = vi.fn(async () => undefined);
    const { c, engines } = harness(send);
    await vi.waitFor(() => expect(useVoiceSession.getState().status).toBe('connected'));
    engines[0]!.mixed = false;
    const r = await c.playSoundboard(sound);
    expect(r.ok).toBe(false);
  });

  it('previews a sound for you alone, in a call or not', async () => {
    const world = fakeAudio();
    const c = new VoiceController({
      createEngine: (ev) => new SoundEngine(ev),
      sendGateway: () => true,
      subscribe: () => () => undefined,
      currentUserId: () => 'me',
      voiceStatesOf: () => ({}),
      audio: world.deps,
      now: () => 0,
      sendSound: vi.fn(),
    });
    expect(await c.previewSound(deft)).toBe(true);
    expect(world.contexts[0]!.bufferSources[0]!.started).toBe(true);
    setAudioFetcher(async () => {
      throw new Error('offline');
    });
    expect(await c.previewSound({ ...deft, id: '2' })).toBe(false);
  });
});

describe('synced favorites (H-45)', () => {
  const patches: unknown[] = [];
  beforeEach(() => {
    patches.length = 0;
    useStore.setState({ userSettings: null });
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: 9, settings: body.settings });
      }),
    );
  });

  it('imports the localStorage list once when the blob has no key, and keeps the volume local', async () => {
    localStorage.setItem(
      'sr.soundboard',
      JSON.stringify({ favorites: ['default:1', 'g1:77'], volume: 40 }),
    );
    resetSoundboard();
    useSoundboard.setState({ favorites: ['default:1', 'g1:77'], volume: 40 });
    adoptFavorites({ version: 1, settings: {} });
    await vi.waitFor(() =>
      expect(patches).toEqual([{ soundboardFavorites: ['default:1', 'g1:77'] }]),
    );
    expect(JSON.parse(localStorage.getItem('sr.soundboard')!)).toEqual({ volume: 40 });
    // Later the blob wins over any local list.
    adoptFavorites({ version: 2, settings: { soundboardFavorites: ['default:2'] } });
    expect(useSoundboard.getState().favorites).toEqual(['default:2']);
    expect(patches).toHaveLength(1);
  });

  it('writes a toggle to the blob and caps the list at 50, newest last', async () => {
    const fifty = Array.from({ length: 50 }, (_, i) => `default:${i}`);
    adoptFavorites({ version: 1, settings: { soundboardFavorites: fifty } });
    useSoundboard.getState().toggleFavorite('g1:77');
    const next = useSoundboard.getState().favorites;
    expect(next).toHaveLength(50);
    expect(next[49]).toBe('g1:77');
    expect(next[0]).toBe('default:1');
    await vi.waitFor(() => expect(patches).toEqual([{ soundboardFavorites: next }]));
  });

  it('follows SR_USER_SETTINGS_UPDATE from another session', () => {
    const stop = startFavoritesSync();
    useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', {
      version: 3,
      settings: { soundboardFavorites: ['default:2', 5] },
    });
    expect(useSoundboard.getState().favorites).toEqual(['default:2']);
    stop();
  });
});
