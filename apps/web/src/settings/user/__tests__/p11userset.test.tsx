import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { CuePlayer } from '../../../voice/sounds';
import { browserAudioDeps } from '../../../voice/deps';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import { RemoteAudio } from '../../../voice/playback';
import { setLocale } from '../../../i18n';
import { http, HttpResponse, server } from '../../../test/server';
import { DEFAULT_PREFS, applyPrefs, sanitizePrefs, streamerState, usePrefsStore } from '../prefs';
import { searchSettings } from '../searchIndex';
import { openSettings, resetPrefs, signedIn } from './helpers';

const saved = () => usePrefsStore.getState().prefs;
const voice = () => useVoiceSettings.getState().settings;

beforeEach(() => {
  resetPrefs();
  signedIn();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  vi.spyOn(browserAudioDeps, 'enumerateDevices').mockResolvedValue([
    { kind: 'videoinput', deviceId: 'cam-1', label: 'USB Camera', groupId: 'g' } as MediaDeviceInfo,
  ]);
  vi.spyOn(browserAudioDeps, 'queryMicPermission').mockResolvedValue('granted');
  vi.spyOn(browserAudioDeps, 'onDeviceChange').mockReturnValue(() => undefined);
});
afterEach(async () => {
  vi.restoreAllMocks();
  await setLocale('en');
});

describe('Streamer Mode (LNG-USET-055)', () => {
  it('only applies once enabled and saved', () => {
    expect(streamerState(DEFAULT_PREFS)).toEqual({
      personal: false,
      invites: false,
      silence: false,
    });
    const on = sanitizePrefs({ ...DEFAULT_PREFS, streamerMode: true, streamerHideInvites: false });
    expect(streamerState(on)).toEqual({ personal: true, invites: false, silence: true });
    expect(applyPrefs(on, false).data['data-streamer-personal']).toBe('true');
  });

  it('is a section with a master switch that greys out the options, and is searchable', async () => {
    const user = userEvent.setup();
    openSettings('streamer');
    const master = await screen.findByRole('switch', { name: /Enable Streamer Mode/ });
    const hide = screen.getByRole('switch', { name: /Hide invite links/ });
    expect(hide).toBeDisabled();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(master);
    expect(hide).toBeEnabled();
    await user.click(hide);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({ streamerMode: true, streamerHideInvites: false });
    expect(searchSettings('streamer')[0]?.section).toBe('streamer');
  });

  it('keeps the email masked and takes Reveal away on the account page', async () => {
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, streamerMode: true } });
    openSettings('account');
    expect(await screen.findByText('Two-factor authentication')).toBeInTheDocument();
    expect(screen.getByText('a******@example.com')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reveal' })).toBeNull();
  });

  it('silences the voice cues', () => {
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, streamerMode: true } });
    expect(streamerState().silence).toBe(true);
    // The controller consults streamerState() before every cue; a CuePlayer is otherwise unaffected.
    expect(new CuePlayer(() => null).play('ring', 1)).toBe(false);
  });
});

describe('Text & Images: inline media size (LNG-USET-051)', () => {
  it('saves a size and exposes it as a data attribute', async () => {
    const user = userEvent.setup();
    openSettings('text');
    await user.click(await screen.findByRole('radio', { name: 'Large' }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().inlineMediaSize).toBe('large');
    expect(applyPrefs(saved(), false).data['data-media-size']).toBe('large');
    expect(applyPrefs(DEFAULT_PREFS, false).data['data-media-size']).toBeUndefined();
  });
});

describe('Language and Advanced (LNG-USET-054, 056)', () => {
  it('has an interface language picker next to the region', async () => {
    openSettings('language');
    expect(await screen.findByRole('combobox', { name: 'Language' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Region and date format' })).toBeInTheDocument();
  });

  it('shows diagnostics only in app test mode', async () => {
    const user = userEvent.setup();
    openSettings('advanced');
    await user.click(await screen.findByRole('switch', { name: 'App test mode' }));
    expect(screen.getByText('Interface language code')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().appTestMode).toBe(true);
  });
});

describe('Voice & Video additions (LNG-USET-046..049)', () => {
  it('sets attenuation, the camera, screen share defaults and the ring sound', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    fireEvent.change(await screen.findByRole('slider', { name: 'Lower others by' }), {
      target: { value: '40' },
    });
    expect(voice().attenuation).toBe(40);
    await user.click(screen.getByRole('switch', { name: /Always preview video/ }));
    expect(voice().alwaysPreviewVideo).toBe(true);
    await user.click(screen.getByRole('switch', { name: 'Share audio by default' }));
    expect(voice().shareAudio).toBe(false);
    await user.click(screen.getByRole('switch', { name: 'Incoming call ring' }));
    expect(voice().soundRing).toBe(false);
    await user.click(screen.getByRole('radio', { name: 'Detail' }));
    expect(voice().shareHint).toBe('detail');
    await user.click(screen.getByRole('combobox', { name: 'Quality' }));
    await user.click(await screen.findByRole('option', { name: '1080p 60 fps' }));
    expect(voice().sharePresetId).toBe('1080p60');
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('lists cameras by name and previews the chosen one, releasing it afterwards', async () => {
    const user = userEvent.setup();
    const track = { stop: vi.fn() };
    const stream = { getTracks: () => [track], getVideoTracks: () => [track] };
    // The preview uses the browser video deps; stub mediaDevices for it.
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getUserMedia: vi.fn().mockResolvedValue(stream),
        enumerateDevices: vi.fn().mockResolvedValue([]),
      },
    });
    openSettings('voice');
    await user.click(await screen.findByRole('combobox', { name: 'Camera' }));
    await user.click(await screen.findByRole('option', { name: 'USB Camera' }));
    expect(voice().cameraDeviceId).toBe('cam-1');
    await user.click(screen.getByRole('button', { name: 'Test video' }));
    await waitFor(() =>
      expect(
        (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mock.calls.length,
      ).toBe(1),
    );
    await user.click(screen.getByRole('button', { name: 'Stop test' }));
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
  });

  it('attenuation lowers the voices and clamps its input', () => {
    const gains: { gain: { value: number } }[] = [];
    const ctx = {
      createGain: () => {
        const g = { gain: { value: 1 }, connect: () => undefined, disconnect: () => undefined };
        gains.push(g);
        return g;
      },
      createMediaStreamSource: () => ({ connect: () => undefined, disconnect: () => undefined }),
      destination: {},
      resume: () => undefined,
      close: () => Promise.resolve(),
    };
    const audio = new RemoteAudio({
      createAudioContext: () => ctx as unknown as AudioContext,
      createAudioElement: () => ({ play: () => undefined, pause: () => undefined }) as never,
    } as never);
    audio.add('u1', {} as MediaStream, 100);
    const voiceGain = gains[1]!.gain;
    expect(voiceGain.value).toBe(1);
    audio.setAttenuation(40);
    expect(voiceGain.value).toBeCloseTo(0.6);
    audio.setAttenuation(250);
    expect(voiceGain.value).toBe(0);
    audio.setAttenuation(0);
    expect(voiceGain.value).toBe(1);
  });
});

describe('Activity: history and defaults (LNG-USET-035)', () => {
  const cinemaApi = (items: unknown[] = []) => {
    let cleared = false;
    server.use(
      http.get('*/api/v1/cinema/me/history', () =>
        HttpResponse.json({ items: cleared ? [] : items }),
      ),
      http.get('*/api/v1/cinema/me/history/settings', () => HttpResponse.json({ paused: false })),
      http.put(
        '*/api/v1/cinema/me/history/settings',
        () => new HttpResponse(null, { status: 204 }),
      ),
      http.delete('*/api/v1/cinema/me/history', () => {
        cleared = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
  };

  it('lists the watch history and clears it after a confirmation', async () => {
    const user = userEvent.setup();
    cinemaApi([
      {
        title: { id: 't1', kind: 'series', title: 'Space Show' },
        episode: { id: 'e1', title_id: 't1', season: 1, number: 3, title: 'Landing' },
        progress: null,
      },
    ]);
    openSettings('activity');
    expect(await screen.findByText('Space Show')).toBeInTheDocument();
    expect(screen.getByText('Episode 3: Landing')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear all' }));
    const dialog = await screen.findByRole('dialog', { name: 'Clear your watch history?' });
    await user.click(within(dialog).getByRole('button', { name: 'Clear all' }));
    expect(await screen.findByText('Nothing in your watch history.')).toBeInTheDocument();
  });

  it('pauses the server-side watch history', async () => {
    const user = userEvent.setup();
    let body: unknown;
    cinemaApi();
    server.use(
      http.put('*/api/v1/cinema/me/history/settings', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('activity');
    await user.click(await screen.findByRole('switch', { name: /Pause watch history/ }));
    await waitFor(() => expect(body).toEqual({ paused: true }));
  });

  it('writes the Cinema defaults to the player record and the Library defaults to the reader', async () => {
    const user = userEvent.setup();
    cinemaApi();
    localStorage.setItem(
      'sr.cinema.player.v1',
      JSON.stringify({ volume: 0.4, autoplayNext: true }),
    );
    openSettings('activity');
    await user.click(
      await screen.findByRole('switch', { name: 'Play the next episode automatically' }),
    );
    await user.click(screen.getByRole('switch', { name: /Data saver/ }));
    const rec = JSON.parse(localStorage.getItem('sr.cinema.player.v1')!);
    expect(rec).toMatchObject({ volume: 0.4, autoplayNext: false, dataSaver: true });
    await user.click(screen.getByRole('radio', { name: 'Long strip' }));
    await user.click(screen.getByRole('radio', { name: 'Left to right' }));
    const reader = JSON.parse(localStorage.getItem('sr.library.reader.v1')!);
    expect(reader.global).toMatchObject({ mode: 'strip', direction: 'ltr' });
  });
});
