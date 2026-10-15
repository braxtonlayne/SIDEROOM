import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { afterAll, beforeAll, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { createAppRouter } from '../../../router';
import { resetComposerDrafts } from '../../Composer';
import { useLoungeUi } from '../../store';
import { setVoiceController, VoiceController } from '../../../voice/controller';
import type { ConnectParams, EngineEvents, VoiceEngine } from '../../../voice/engine';
import { patchSession, resetSession, useVoiceSession } from '../../../voice/session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import {
  resetLocalVideo,
  useLocalVideo,
  useRemoteVideo,
  type VideoDeps,
} from '../../../voice/video';
import { clearVideoLimitsCache } from '../../../voice/limits';
import { JoinPreview } from '../JoinPreview';
import { MockDataProvider } from '../../mockData';
import { PingGraph, pingPoints } from '../PingGraph';
import { StreamTile } from '../Tiles';
import { ScreenSharePicker } from '../ScreenSharePicker';
import { FREE_PROFILES } from '../../../voice/video';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

class StubEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'idle';
  constructor(readonly events: EngineEvents) {}
  connect(p?: ConnectParams) {
    void p;
    return new Promise<void>(() => undefined);
  }
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
}

class Track {
  stopped = false;
  contentHint = '';
  constructor(readonly kind: 'video' | 'audio') {}
  addEventListener() {}
  removeEventListener() {}
  stop() {
    this.stopped = true;
  }
}
class Stream {
  tracks: Track[];
  constructor(audio = false) {
    this.tracks = [new Track('video'), ...(audio ? [new Track('audio')] : [])];
  }
  getVideoTracks() {
    return this.tracks.filter((t) => t.kind === 'video');
  }
  getAudioTracks() {
    return this.tracks.filter((t) => t.kind === 'audio');
  }
  getTracks() {
    return this.tracks;
  }
}

let video: VideoDeps & {
  streams: Stream[];
  display: DisplayMediaStreamOptions[];
  cams: MediaDeviceInfo[];
};

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const info = (id: string, label: string, kind: MediaDeviceKind) =>
  ({ deviceId: id, label, kind, groupId: '' }) as MediaDeviceInfo;

beforeEach(() => {
  resetComposerDrafts();
  localStorage.clear();
  clearVideoLimitsCache();
  resetLocalVideo();
  useRemoteVideo.setState({ camera: {}, screen: {}, watching: {} });
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  resetSession();
  const streams: Stream[] = [];
  const display: DisplayMediaStreamOptions[] = [];
  const cams = [
    info('cam-1', 'Front camera', 'videoinput'),
    info('mic-1', 'USB mic', 'audioinput'),
  ];
  video = {
    streams,
    display,
    cams,
    getUserMedia: async () => {
      const s = new Stream();
      streams.push(s);
      return s as unknown as MediaStream;
    },
    getDisplayMedia: async (o) => {
      display.push(o);
      const s = new Stream(!!o.audio);
      streams.push(s);
      return s as unknown as MediaStream;
    },
    enumerateDevices: async () => cams,
  };
  // The device menus read the browser's device list.
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: {
      enumerateDevices: async () => cams,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    },
  });
  server.use(
    http.get('*/api/v1/donations/me', () =>
      HttpResponse.json({
        patron: { active: false, in_grace: false, permanent: false },
        limits: { video_profiles: [{ height: 720, fps: 30 }], patron: false, stream_viewers: 25 },
        preferences: {},
      }),
    ),
    http.get('*/api/v1/guilds/:id/support', () => new HttpResponse(null, { status: 404 })),
  );
  setVoiceController(
    new VoiceController({
      createEngine: (events) => new StubEngine(events),
      sendGateway: () => true,
      subscribe: () => () => undefined,
      currentUserId: () => '1',
      voiceStatesOf: () => ({}),
      audio: {
        getUserMedia: () => Promise.reject(new Error('no media in tests')),
        createAudioContext: () => null,
        createAudioElement: () => document.createElement('audio'),
        enumerateDevices: async () => cams,
        onDeviceChange: () => () => undefined,
        queryMicPermission: async () => 'prompt',
      },
      now: () => Date.now(),
      video,
    }),
  );
});
afterEach(() => setVoiceController(null));

async function joined(user: ReturnType<typeof userEvent.setup>) {
  at('/channels/1001/2011');
  await user.click(await screen.findByRole('button', { name: 'Join Voice' }, { timeout: 8000 }));
  return screen.findByRole('toolbar', { name: 'Call controls' });
}

describe('ping graph', () => {
  it('scales the readings into the box and summarises them', () => {
    expect(pingPoints([])).toBe('');
    const pts = pingPoints([50, 100], 100, 30).split(' ');
    expect(pts).toHaveLength(2);
    expect(pts[0]!.startsWith('0,')).toBe(true);
    expect(pts[1]!.startsWith('100,')).toBe(true);
  });

  it('asks for more readings first, then shows average and peak', () => {
    const { rerender } = render(<PingGraph history={[40]} />);
    expect(screen.getByText('Measuring connection')).toBeInTheDocument();
    rerender(<PingGraph history={[40, 60, 80]} />);
    expect(screen.getByText('Avg 60 ms, peak 80 ms')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /average 60 ms, peak 80 ms/ })).toBeInTheDocument();
  });
});

describe('screen share picker', () => {
  it('disables the qualities above the entitlement with an upgrade hint, and starts with the choices', async () => {
    const user = userEvent.setup();
    const onStart = vi.fn();
    render(
      <ScreenSharePicker
        open
        onClose={() => undefined}
        profiles={FREE_PROFILES}
        onStart={onStart}
      />,
    );
    const dialog = await screen.findByRole('dialog', { name: 'Share your screen' });
    expect(within(dialog).getByRole('radio', { name: /720p 30 fps/ })).toBeEnabled();
    for (const name of [/720p 60 fps/, /1080p 30 fps/, /1080p 60 fps/])
      expect(within(dialog).getByRole('radio', { name })).toBeDisabled();
    expect(within(dialog).getAllByText('Needs Patron or a higher server level')).toHaveLength(3);
    expect(within(dialog).getByTestId('share-preview-only')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('radio', { name: /A browser tab/ }));
    await user.click(within(dialog).getByRole('radio', { name: /Detail/ }));
    await user.click(within(dialog).getByRole('switch', { name: /Share audio/ }));
    await user.click(within(dialog).getByRole('button', { name: 'Go Live' }));
    expect(onStart).toHaveBeenCalledWith({
      preset: expect.objectContaining({ id: '720p30' }),
      source: 'tab',
      audio: false,
      hint: 'detail',
    });
  });

  it('unlocks everything for a Patron profile and has no a11y violations', async () => {
    render(
      <ScreenSharePicker
        open
        onClose={() => undefined}
        profiles={[{ height: 1080, fps: 60 }]}
        onStart={() => undefined}
      />,
    );
    const dialog = await screen.findByRole('dialog');
    for (const r of within(dialog).getAllByRole('radio')) expect(r).toBeEnabled();
    expect(await axeViolations(dialog)).toEqual([]);
  });
});

describe('join preview', () => {
  it('shows a self view and releases the camera when turned off or unmounted', async () => {
    const user = userEvent.setup();
    const { unmount } = render(<JoinPreview deps={video} />);
    await user.click(screen.getByRole('button', { name: 'Preview camera' }));
    expect(await screen.findByLabelText('Your camera preview')).toBeInTheDocument();
    const first = video.streams[0]!;
    await user.click(screen.getByRole('button', { name: 'Turn off camera preview' }));
    expect(first.tracks.every((t) => t.stopped)).toBe(true);
    await user.click(screen.getByRole('button', { name: 'Preview camera' }));
    await screen.findByLabelText('Your camera preview');
    unmount();
    expect(video.streams[1]!.tracks.every((t) => t.stopped)).toBe(true);
  });

  it('explains a blocked camera', async () => {
    const user = userEvent.setup();
    render(
      <JoinPreview
        deps={{
          ...video,
          getUserMedia: async () => {
            throw new DOMException('no', 'NotAllowedError');
          },
        }}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Preview camera' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Camera access is blocked');
  });
});

describe('call controls', () => {
  it('turns the camera on and off and shows the self view in the tile', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    await user.click(within(bar).getByRole('button', { name: 'Turn on camera' }));
    await waitFor(() => expect(useVoiceSession.getState().camera).toBe(true));
    const tile = await screen.findByTestId('voice-tile-1');
    expect(within(tile).getByLabelText("Braxton's camera")).toBeInTheDocument();
    expect(within(tile).getByTestId('tile-quality')).toBeInTheDocument();
    await user.click(within(bar).getByRole('button', { name: 'Turn off camera' }));
    expect(useVoiceSession.getState().camera).toBe(false);
    expect(
      within(screen.getByTestId('voice-tile-1')).queryByLabelText("Braxton's camera"),
    ).toBeNull();
    expect(video.streams[0]!.tracks.every((t) => t.stopped)).toBe(true);
  });

  it('lists cameras and input devices in the drop-down menus', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    await user.click(within(bar).getByRole('button', { name: 'Camera options' }));
    const menu = await screen.findByRole('menu', { name: 'Camera options' });
    expect(within(menu).getByRole('menuitemradio', { name: 'Front camera' })).toBeInTheDocument();
    await user.keyboard('{Escape}');
    await user.click(within(bar).getByRole('button', { name: 'Microphone options' }));
    const mic = await screen.findByRole('menu', { name: 'Microphone options' });
    await user.click(within(mic).getByRole('menuitemradio', { name: 'USB mic' }));
    expect(useVoiceSettings.getState().settings.inputDeviceId).toBe('mic-1');
  });

  it('shares the screen through the picker, shows a LIVE stream tile and stops it', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    await user.click(within(bar).getByRole('button', { name: 'Share your screen' }));
    const dialog = await screen.findByRole('dialog', { name: 'Share your screen' });
    await user.click(within(dialog).getByRole('button', { name: 'Go Live' }));
    await waitFor(() => expect(useVoiceSession.getState().screen).toBe(true));
    expect(video.display[0]).toMatchObject({
      audio: true,
      video: { height: { ideal: 720 }, frameRate: { ideal: 30, max: 30 } },
    });
    const tile = await screen.findByTestId('stream-tile-1');
    expect(within(tile).getByText('LIVE')).toBeInTheDocument();
    expect(within(tile).getByText('Streaming at 720p 30 fps')).toBeInTheDocument();
    expect(useLocalVideo.getState().screen?.hasAudio).toBe(true);
    await user.click(within(tile).getByRole('button', { name: 'Stop sharing' }));
    expect(useVoiceSession.getState().screen).toBe(false);
    expect(screen.queryByTestId('stream-tile-1')).toBeNull();
  });

  it('shows a camera error from the controller and lets it be dismissed', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    video.getUserMedia = async () => {
      throw new DOMException('no', 'NotAllowedError');
    };
    await user.click(within(bar).getByRole('button', { name: 'Turn on camera' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Camera access is blocked');
    await user.click(within(alert).getByRole('button', { name: 'Dismiss' }));
    await waitFor(() => expect(screen.queryByText(/Camera access is blocked/)).toBeNull());
  });

  it('opens the text chat beside the call and closes it', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    await user.click(within(bar).getByRole('button', { name: 'Text chat' }));
    const chat = await screen.findByRole('complementary', { name: 'Text chat for General' });
    expect(within(chat).getByRole('textbox')).toBeInTheDocument();
    await user.click(within(chat).getByRole('button', { name: 'Close text chat' }));
    expect(screen.queryByRole('complementary', { name: 'Text chat for General' })).toBeNull();
  });

  it('focus mode spotlights one tile and keeps the others in a strip', async () => {
    const user = userEvent.setup();
    await joined(user);
    await user.click(await screen.findByRole('button', { name: 'Focus on Mara' }));
    const spot = await screen.findByRole('list', { name: 'In General' });
    expect(within(spot).getByText('Mara')).toBeInTheDocument();
    expect(within(spot).queryByText('Doge')).toBeNull();
    const strip = screen.getByRole('list', { name: 'Other people in this call' });
    expect(within(strip).getByText('Doge')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Leave focus mode' }));
    expect(screen.queryByRole('list', { name: 'Other people in this call' })).toBeNull();
  });

  it('hides participants without video from the More menu', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    await user.click(within(bar).getByRole('button', { name: 'More options' }));
    const menu = await screen.findByRole('menu', { name: 'More options menu' });
    await user.click(
      within(menu).getByRole('menuitemcheckbox', { name: 'Hide non-video participants' }),
    );
    expect(await screen.findByText('Nobody has their camera on.')).toBeInTheDocument();
    expect(screen.queryByTestId('voice-tile-2')).toBeNull();
    await user.click(within(bar).getByRole('button', { name: 'Turn on camera' }));
    await screen.findByTestId('voice-tile-1');
    expect(screen.queryByTestId('voice-tile-2')).toBeNull();
  });

  it('shows another person LIVE with Watch Stream, then their video once it arrives', async () => {
    const user = userEvent.setup();
    render(
      <MockDataProvider>
        <ul>
          <StreamTile userId="2" name="Mara" />
        </ul>
      </MockDataProvider>,
    );
    const tile = await screen.findByTestId('stream-tile-2');
    expect(within(tile).getByText('LIVE')).toBeInTheDocument();
    await user.click(within(tile).getByRole('button', { name: 'Watch Stream' }));
    expect(useRemoteVideo.getState().watching['2']).toBe(true);
    expect(within(tile).getByRole('status')).toHaveTextContent('not available to watch yet');
    act(() => useRemoteVideo.setState({ screen: { '2': new Stream() as unknown as MediaStream } }));
    expect(within(tile).getByLabelText("Mara's stream", { selector: 'video' })).toBeInTheDocument();
    expect(within(tile).getByRole('button', { name: 'Stop Watching' })).toBeInTheDocument();
    expect(within(tile).getByRole('button', { name: 'Full screen' })).toBeInTheDocument();
    await user.click(within(tile).getByRole('button', { name: 'Stop Watching' }));
    expect(useRemoteVideo.getState().watching['2']).toBe(false);
  });

  it('has no a11y violations with every control present', async () => {
    const user = userEvent.setup();
    const bar = await joined(user);
    expect(await axeViolations(bar)).toEqual([]);
  });
});

describe('voice user menu (VOX-UI-008)', () => {
  it('offers Profile, Mention, Message, Add Note, volume, Mute and Copy User ID on a tile', async () => {
    const user = userEvent.setup();
    await joined(user);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    const tile = await screen.findByRole('group', { name: /^Mara/ });
    await user.pointer({ keys: '[MouseRight]', target: tile });
    const menu = await screen.findByRole('menu', { name: 'Mara voice actions' });
    for (const name of ['Profile', 'Mention', 'Message', 'Add Note', 'Copy User ID'])
      expect(within(menu).getByRole('menuitem', { name })).toBeInTheDocument();
    expect(within(menu).getByRole('menuitem', { name: 'User Volume' })).toBeInTheDocument();
    expect(within(menu).getByRole('menuitemcheckbox', { name: 'Mute' })).toBeInTheDocument();
    await user.click(within(menu).getByRole('menuitem', { name: 'Mention' }));
    expect(writeText).toHaveBeenCalledWith('<@2>');
  });

  it('has no menu on your own tile', async () => {
    const user = userEvent.setup();
    await joined(user);
    const tile = await screen.findByTestId('voice-tile-1');
    await user.pointer({ keys: '[MouseRight]', target: tile });
    expect(screen.queryByRole('menu')).toBeNull();
  });
});

describe('dragging people between channels (VOX-UI-009)', () => {
  it('moves a dragged person with the moderator route and ignores other servers or the same channel', async () => {
    const { dropVoiceUser } = await import('../voiceDrag');
    let body: unknown;
    let url = '';
    server.use(
      http.patch('*/api/v1/guilds/g1/voice-states/u2', async ({ request }) => {
        url = new URL(request.url).pathname;
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    await dropVoiceUser({ userId: 'u2', guildId: 'g1', fromChannelId: 'v1' }, 'g1', 'v1');
    await dropVoiceUser({ userId: 'u2', guildId: 'other', fromChannelId: 'v1' }, 'g1', 'v2');
    expect(body).toBeUndefined();
    await dropVoiceUser({ userId: 'u2', guildId: 'g1', fromChannelId: 'v1' }, 'g1', 'v2');
    expect(url).toBe('/api/v1/guilds/g1/voice-states/u2');
    expect(body).toEqual({ channel_id: 'v2' });
  });

  it('highlights a voice channel under a dragged person and sends the move on drop', async () => {
    const { VOICE_USER_MIME } = await import('../voiceDrag');
    const { fireEvent } = await import('@testing-library/react');
    let body: unknown;
    server.use(
      http.patch('*/api/v1/guilds/1001/voice-states/2', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const target = within(sidebar).getByRole('button', { name: /^Movie night/ });
    const payload = JSON.stringify({ userId: '2', guildId: '1001', fromChannelId: '2011' });
    const dataTransfer = {
      types: [VOICE_USER_MIME],
      getData: (t: string) => (t === VOICE_USER_MIME ? payload : ''),
      dropEffect: 'none',
    };
    fireEvent.dragOver(target, { dataTransfer });
    expect(target).toHaveAttribute('data-voice-drop');
    fireEvent.drop(target, { dataTransfer });
    await waitFor(() => expect(body).toEqual({ channel_id: '2012' }));
    expect(target).not.toHaveAttribute('data-voice-drop');
  });

  it('ignores a file dragged over a voice channel', async () => {
    const { fireEvent } = await import('@testing-library/react');
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const target = within(sidebar).getByRole('button', { name: /^Movie night/ });
    fireEvent.dragOver(target, { dataTransfer: { types: ['Files'] } });
    expect(target).not.toHaveAttribute('data-voice-drop');
  });
});

describe('voice panel notices (VOX-UI-013 / VOX-UI-014)', () => {
  it('shows the governor notice and the refusal with upgrade links, and dismisses the refusal', async () => {
    const user = userEvent.setup();
    at('/channels/1001/2001');
    act(() => {
      patchSession({
        status: 'connected',
        guildId: '1001',
        channelId: '2011',
        channelName: 'General',
        reducedQuality: true,
        refusal: { code: 'videoLimit', message: 'This channel has reached its video limit (5).' },
      });
    });
    const panel = await screen.findByRole('region', { name: 'Voice connection' });
    expect(within(panel).getByTestId('governor-notice')).toHaveTextContent('Reduced quality');
    const refusal = within(panel).getByTestId('refusal-notice');
    expect(refusal).toHaveTextContent('video limit (5)');
    expect(within(refusal).getByRole('link', { name: 'Support this server' })).toHaveAttribute(
      'href',
      '/support',
    );
    expect(within(refusal).getByRole('link', { name: 'Become a Patron' })).toBeInTheDocument();
    await user.click(within(refusal).getByRole('button', { name: 'Dismiss' }));
    expect(useVoiceSession.getState().refusal).toBeNull();
    expect(await axeViolations(panel)).toEqual([]);
  });

  it('the camera and share buttons in the panel follow the session', async () => {
    const user = userEvent.setup();
    await joined(user);
    const panel = await screen.findByRole('region', { name: 'Voice connection' });
    await user.click(within(panel).getByRole('button', { name: 'Turn on camera' }));
    await waitFor(() => expect(useVoiceSession.getState().camera).toBe(true));
    expect(within(panel).getByRole('button', { name: 'Turn off camera' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(within(panel).getByRole('button', { name: 'Share your screen' }));
    const dialog = await screen.findByRole('dialog', { name: 'Share your screen' });
    await user.click(within(dialog).getByRole('button', { name: 'Go Live' }));
    await waitFor(() => expect(useVoiceSession.getState().screen).toBe(true));
    await user.click(within(panel).getByRole('button', { name: 'Stop sharing' }));
    expect(useVoiceSession.getState().screen).toBe(false);
  });
});
