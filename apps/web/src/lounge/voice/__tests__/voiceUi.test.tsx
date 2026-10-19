import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { afterAll, beforeAll, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { createAppRouter } from '../../../router';
import { resetComposerDrafts } from '../../Composer';
import { useLoungeUi } from '../../store';
import { resetStore, useStore } from '../../../store';
import { setVoiceController, VoiceController } from '../../../voice/controller';
import type { ConnectParams, EngineEvents, VoiceEngine } from '../../../voice/engine';
import { patchSession, resetSession, useVoiceSession } from '../../../voice/session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import { setPreviewFetcher } from '../useStreamPreview';
import { warmLoungeRoute } from '../../../test/warmRoutes';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

class StubEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'idle';
  params: ConnectParams | null = null;
  calls: string[] = [];
  constructor(readonly events: EngineEvents) {}
  connect(p: ConnectParams) {
    this.params = p;
    return new Promise<void>(() => undefined);
  }
  disconnect() {
    this.calls.push('disconnect');
  }
  setTransmitting(t: boolean) {
    this.calls.push(`transmit:${t}`);
  }
  setDeafened(d: boolean) {
    this.calls.push(`deafen:${d}`);
  }
  setUserVolume(id: string, v: number) {
    this.calls.push(`volume:${id}:${v}`);
  }
  setUserMuted() {}
  setMasterVolume() {}
  setInputVolume() {}
  async setInputDevice() {}
  async setOutputDevice() {}
  restartIce() {}
}

let sent: { op: number; d: Record<string, unknown> }[];
let engines: StubEngine[];
let emit: (e: string, d: unknown) => void;

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  resetComposerDrafts();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  resetSession();
  sent = [];
  engines = [];
  const listeners = new Set<(e: string, d: unknown) => void>();
  emit = (e, d) => listeners.forEach((l) => l(e, d));
  setVoiceController(
    new VoiceController({
      createEngine: (events) => {
        const e = new StubEngine(events);
        engines.push(e);
        return e;
      },
      sendGateway: (op, d) => (sent.push({ op, d: d as Record<string, unknown> }), true),
      subscribe: (l) => (listeners.add(l), () => listeners.delete(l)),
      currentUserId: () => '1',
      voiceStatesOf: () => ({}),
      audio: {
        getUserMedia: () => Promise.reject(new Error('no media in tests')),
        createAudioContext: () => null,
        createAudioElement: () => document.createElement('audio'),
        enumerateDevices: async () => [],
        onDeviceChange: () => () => undefined,
        queryMicPermission: async () => 'prompt',
      },
      now: () => Date.now(),
    }),
  );
});
afterEach(() => setVoiceController(null));

// After the env stubs above, so the warmed modules see the same env as the tests.
warmLoungeRoute();

describe('voice channel list', () => {
  it('shows participants under the voice channel with speaking ring and mute icon', async () => {
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const list = within(sidebar).getByRole('list', { name: 'Connected to General' });
    const mara = within(list).getByText('Mara').closest('[data-speaking]');
    expect(mara).not.toBeNull();
    const muted = within(list).getByText('Doge').closest('.lng-voice-member')!;
    expect(within(muted as HTMLElement).getByRole('img', { name: 'Muted' })).toBeInTheDocument();
  });

  it('shows the stream thumbnail while hovering a streaming row (LNG-CHN-010)', async () => {
    const user = userEvent.setup();
    const seen: string[] = [];
    const create = vi.fn(() => 'blob:row-thumb');
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: create, revokeObjectURL: vi.fn() }));
    setPreviewFetcher(async (key) => {
      seen.push(key);
      return new Response(new Blob(['jpeg']), { status: 200, headers: { etag: '"1"' } });
    });
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const row = (await within(sidebar).findByText('Kestrel')).closest('.lng-voice-member')!;
    expect(within(sidebar).queryByRole('img', { name: /Preview of the stream/ })).toBeNull();
    await user.hover(row);
    const img = await within(sidebar).findByRole('img', { name: /Preview of the stream/ });
    expect(img).toHaveAttribute('src', 'blob:row-thumb');
    expect(seen[0]).toMatch(/^guild:1001:2012:5$/);
    await user.unhover(row);
    await waitFor(() =>
      expect(within(sidebar).queryByRole('img', { name: /Preview of the stream/ })).toBeNull(),
    );
    setPreviewFetcher(null);
    vi.unstubAllGlobals();
  });

  it('clicking a voice channel joins it, sends the gateway op and opens the voice view', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await user.click(within(sidebar).getByRole('button', { name: /^Movie night/ }));
    expect(sent.at(-1)).toMatchObject({
      op: 4,
      d: { guild_id: '1001', channel_id: '2012', self_mute: false, self_deaf: false },
    });
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Movie night' }),
    ).toBeInTheDocument();
    expect(useVoiceSession.getState()).toMatchObject({ channelId: '2012', status: 'joining' });
  });
});

describe('voice channel view', () => {
  it('shows who is inside and a Join Voice button before joining', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2011');
    const grid = await screen.findByRole('list', { name: 'In this channel' });
    expect(within(grid).getByText('Mara')).toBeInTheDocument();
    expect(screen.queryByRole('toolbar', { name: 'Call controls' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Join Voice' }));
    expect(sent.at(-1)!.d.channel_id).toBe('2011');
    expect(await screen.findByRole('toolbar', { name: 'Call controls' })).toBeInTheDocument();
  });

  it('shows tiles with speaking ring and mute state, and has no a11y violations', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2011');
    await user.click(await screen.findByRole('button', { name: 'Join Voice' }));
    const grid = await screen.findByRole('list', { name: 'In General' });
    expect(within(grid).getByRole('group', { name: 'Mara, speaking' })).toHaveAttribute(
      'data-speaking',
    );
    expect(within(grid).getByRole('group', { name: 'Doge, muted' })).toBeInTheDocument();
    expect(await axeViolations(screen.getByRole('region', { name: 'General' }))).toEqual([]);
  });

  it('call controls mute, deafen and disconnect', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2011');
    await user.click(await screen.findByRole('button', { name: 'Join Voice' }));
    const bar = await screen.findByRole('toolbar', { name: 'Call controls' });
    await user.click(within(bar).getByRole('button', { name: 'Mute' }));
    expect(useVoiceSession.getState().selfMute).toBe(true);
    expect(sent.at(-1)!.d).toMatchObject({ self_mute: true, self_deaf: false });
    expect(within(bar).getByRole('button', { name: 'Unmute' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(within(bar).getByRole('button', { name: 'Deafen' }));
    expect(useVoiceSession.getState()).toMatchObject({ selfDeaf: true, selfMute: true });
    await user.click(within(bar).getByRole('button', { name: 'Disconnect' }));
    expect(sent.at(-1)!.d.channel_id).toBeNull();
    expect(screen.queryByRole('toolbar', { name: 'Call controls' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Join Voice' })).toBeInTheDocument();
  });

  it('opens a participant menu with User Volume and local Mute', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2011');
    await user.click(await screen.findByRole('button', { name: 'Join Voice' }));
    const tile = await screen.findByRole('group', { name: 'Mara, speaking' });
    await user.pointer({ keys: '[MouseRight]', target: tile });
    const menu = await screen.findByRole('menu', { name: 'Mara voice actions' });
    await user.click(within(menu).getByRole('menuitemcheckbox', { name: 'Mute' }));
    expect(useVoiceSettings.getState().settings.localMuted).toEqual({ '2': true });
  });

  it('sets a per-person volume to 200% from the tile', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2011');
    await user.click(await screen.findByRole('button', { name: 'Join Voice' }));
    await user.click(await screen.findByRole('button', { name: 'Volume for Mara' }));
    const slider = await screen.findByRole('slider', { name: 'Mara volume' });
    expect(slider).toHaveAttribute('max', '200');
    const { fireEvent } = await import('@testing-library/react');
    fireEvent.change(slider, { target: { value: '200' } });
    expect(useVoiceSettings.getState().settings.userVolumes).toEqual({ '2': 200 });
  });
});

describe('voice connection panel', () => {
  it('shows state, channel, quality and disconnect once joined', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await user.click(within(sidebar).getByRole('button', { name: /^Movie night/ }));
    const panel = await screen.findByRole('region', { name: 'Voice connection' });
    expect(panel).toHaveTextContent('RTC Connecting');
    expect(panel).toHaveTextContent('Movie night');
    // The voice server answered: now "connecting" the engine, then connected with a measured quality.
    act(() => {
      emit('VOICE_SERVER_UPDATE', {
        guild_id: '1001',
        channel_id: '2012',
        endpoint: 'wss://x.test/voice/ws',
        token: 't',
      });
    });
    expect(engines).toHaveLength(1);
    act(() => {
      patchSession({ status: 'connected', quality: { level: 'good', pingMs: 38, lossPct: 0 } });
    });
    expect(panel).toHaveTextContent('Voice Connected');
    expect(within(panel).getAllByRole('img', { name: 'Good connection, 38 ms' }).length).toBe(1);
    expect(await axeViolations(panel)).toEqual([]);
    await user.click(within(panel).getByRole('button', { name: 'Disconnect' }));
    await waitFor(() =>
      expect(screen.queryByRole('region', { name: 'Voice connection' })).toBeNull(),
    );
  });

  it('user panel mute and deafen buttons follow the session', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2001');
    await user.click(await screen.findByRole('button', { name: 'Mute' }));
    expect(useVoiceSession.getState().selfMute).toBe(true);
    expect(screen.getByRole('button', { name: 'Unmute' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(screen.getByRole('button', { name: 'Deafen' }));
    expect(screen.getByRole('button', { name: 'Undeafen' })).toBeInTheDocument();
  });

  it('explains a denied microphone and links to the voice settings', async () => {
    at('/channels/1001/2001');
    act(() => {
      patchSession({
        status: 'error',
        error: { code: 'mic', mic: 'permission-denied', message: 'Microphone access is blocked.' },
      });
    });
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent("Can't use your microphone");
    expect(alert).toHaveTextContent('Microphone access is blocked.');
    expect(within(alert).getByRole('button', { name: 'Voice settings' })).toBeInTheDocument();
    expect(await axeViolations(alert)).toEqual([]);
  });

  it('shows a forced push-to-talk note when the channel denies voice activity', async () => {
    at('/channels/1001/2001');
    act(() => {
      patchSession({
        status: 'connected',
        guildId: '1001',
        channelId: '2011',
        channelName: 'General',
        pttForced: true,
      });
    });
    expect(await screen.findByText('This channel requires push-to-talk.')).toBeInTheDocument();
  });
});

describe('moderator voice menu', () => {
  beforeEach(() => {
    resetStore();
  });

  it('offers server mute, deafen, move and disconnect to moderators and sends the PATCH', async () => {
    const { renderHook } = await import('@testing-library/react');
    const { useVoiceMenuEntries } = await import('../useVoiceMenu');
    const perms = (1n << 22n) | (1n << 23n) | (1n << 24n);
    useStore.getState().applyDispatch('READY', {
      user: { id: 'me', username: 'me' },
      guilds: [],
    });
    useStore.getState().applyDispatch('GUILD_CREATE', {
      id: 'g1',
      name: 'G',
      owner_id: 'someone',
      roles: [
        { id: 'g1', name: '@everyone', color: 0, position: 0, permissions: '0' },
        { id: 'mod', name: 'Mod', color: 0, position: 1, permissions: perms.toString() },
      ],
      channels: [
        { id: 'v1', type: 2, guild_id: 'g1', name: 'Lobby', position: 0 },
        { id: 'v2', type: 2, guild_id: 'g1', name: 'Games', position: 1 },
      ],
      members: [{ user: { id: 'me', username: 'me' }, roles: ['mod'] }],
      voice_states: [{ channel_id: 'v1', user_id: 'u2', mute: false }],
    });
    let body: unknown;
    server.use(
      http.patch('*/api/v1/guilds/g1/voice-states/u2', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    const { result } = renderHook(() => useVoiceMenuEntries('g1', { id: 'u2', name: 'Uma' }));
    const labels = result.current.map((e) => ('label' in e ? String(e.label) : e.type));
    expect(labels).toEqual(
      expect.arrayContaining([
        'User Volume',
        'Mute',
        'Server Mute',
        'Server Deafen',
        'Move To',
        'Disconnect',
      ]),
    );
    const find = (l: string) =>
      result.current.find((e) => 'label' in e && e.label === l) as {
        onSelect?: () => void;
        onCheckedChange?: (b: boolean) => void;
        items?: { label: string; onSelect: () => void }[];
      };
    act(() => find('Server Mute').onCheckedChange!(true));
    await waitFor(() => expect(body).toEqual({ mute: true }));
    act(() => find('Disconnect').onSelect!());
    await waitFor(() => expect(body).toEqual({ channel_id: null }));
    const move = find('Move To').items!;
    expect(move.map((i) => i.label)).toEqual(['Games']);
    act(() => move[0]!.onSelect());
    await waitFor(() => expect(body).toEqual({ channel_id: 'v2' }));
  });

  it('hides moderation without the permissions, and everything for yourself or people not in voice', async () => {
    const { renderHook } = await import('@testing-library/react');
    const { useVoiceMenuEntries } = await import('../useVoiceMenu');
    useStore.getState().applyDispatch('READY', { user: { id: 'me', username: 'me' }, guilds: [] });
    useStore.getState().applyDispatch('GUILD_CREATE', {
      id: 'g1',
      name: 'G',
      owner_id: 'someone',
      roles: [{ id: 'g1', name: '@everyone', color: 0, position: 0, permissions: '0' }],
      channels: [{ id: 'v1', type: 2, guild_id: 'g1', name: 'Lobby', position: 0 }],
      members: [{ user: { id: 'me', username: 'me' }, roles: [] }],
      voice_states: [
        { channel_id: 'v1', user_id: 'u2' },
        { channel_id: 'v1', user_id: 'me' },
      ],
    });
    const regular = renderHook(() => useVoiceMenuEntries('g1', { id: 'u2', name: 'Uma' }));
    const labels = regular.result.current.map((e) => ('label' in e ? String(e.label) : ''));
    expect(labels).toEqual(['User Volume', 'Mute']);
    expect(
      renderHook(() => useVoiceMenuEntries('g1', { id: 'me', name: 'Me' })).result.current,
    ).toEqual([]);
    expect(
      renderHook(() => useVoiceMenuEntries('g1', { id: 'u9', name: 'Zed' })).result.current,
    ).toEqual([]);
  });
});
