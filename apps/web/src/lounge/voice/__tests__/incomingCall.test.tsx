import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { resetStore, useStore } from '../../../store';
import { emitDispatch } from '../../../store/events';
import { applyCallEvent, RING_TIMEOUT_MS, resetCalls, useCalls } from '../../../voice/calls';
import { setVoiceController, VoiceController } from '../../../voice/controller';
import type { EngineEvents, VoiceEngine } from '../../../voice/engine';
import { resetSession, useVoiceSession } from '../../../voice/session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import { resetLocalVideo, type VideoDeps } from '../../../voice/video';
import { MockDataProvider } from '../../mockData';
import { IncomingCallOverlay } from '../IncomingCall';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

class StubEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'idle';
  constructor(readonly events: EngineEvents) {}
  connect() {
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

let sent: { op: number; d: Record<string, unknown> }[];
let oscillators: number;

const fakeContext = () =>
  ({
    currentTime: 0,
    destination: {},
    resume: () => Promise.resolve(),
    close: () => Promise.resolve(),
    createOscillator: () => {
      oscillators++;
      return { type: 'sine', frequency: {}, connect() {}, start() {}, stop() {} };
    },
    createGain: () => ({
      gain: { setValueAtTime() {}, linearRampToValueAtTime() {} },
      connect() {},
    }),
  }) as unknown as AudioContext;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  resetStore();
  useStore.setState({ currentUserId: '1' });
  resetCalls();
  resetSession();
  resetLocalVideo();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  sent = [];
  oscillators = 0;
  const video: VideoDeps = {
    getUserMedia: async () =>
      ({
        getVideoTracks: () => [{ addEventListener() {} }],
        getTracks: () => [],
      }) as unknown as MediaStream,
    getDisplayMedia: async () => ({}) as MediaStream,
    enumerateDevices: async () => [],
  };
  setVoiceController(
    new VoiceController({
      createEngine: (events) => new StubEngine(events),
      sendGateway: (op, d) => (sent.push({ op, d: d as Record<string, unknown> }), true),
      subscribe: () => () => undefined,
      currentUserId: () => '1',
      voiceStatesOf: () => ({}),
      audio: {
        getUserMedia: () => Promise.reject(new Error('none')),
        createAudioContext: fakeContext,
        createAudioElement: () => document.createElement('audio'),
        enumerateDevices: async () => [],
        onDeviceChange: () => () => undefined,
        queryMicPermission: async () => 'prompt',
      },
      now: () => Date.now(),
      video,
    }),
  );
});
afterEach(() => {
  setVoiceController(null);
  vi.useRealTimers();
});

const renderOverlay = () =>
  render(
    <MockDataProvider>
      <IncomingCallOverlay />
    </MockDataProvider>,
  );

describe('call events', () => {
  it('rings only for the viewer, and stops on update or delete', () => {
    applyCallEvent('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['9'] });
    expect(useCalls.getState().incoming).toEqual([]);
    applyCallEvent('CALL_CREATE', {
      channel_id: 'd1',
      initiator_id: '2',
      ringing: ['1'],
      video: true,
    });
    expect(useCalls.getState().incoming).toMatchObject([
      { channelId: 'd1', callerId: '2', video: true, group: false },
    ]);
    // A duplicate event does not stack a second ring.
    applyCallEvent('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['1'] });
    expect(useCalls.getState().incoming).toHaveLength(1);
    applyCallEvent('CALL_UPDATE', { channel_id: 'd1', ringing: [] });
    expect(useCalls.getState().incoming).toEqual([]);
    applyCallEvent('CALL_CREATE', { channel_id: 'd2', ringing: ['1'], group: true });
    applyCallEvent('CALL_DELETE', { channel_id: 'd2' });
    expect(useCalls.getState().incoming).toEqual([]);
    applyCallEvent('MESSAGE_CREATE', { channel_id: 'd3' });
    applyCallEvent('CALL_CREATE', { ringing: ['1'] });
    expect(useCalls.getState().incoming).toEqual([]);
  });
});

describe('incoming call overlay', () => {
  it('shows the caller with Accept, Accept with video and Decline, rings, and has no a11y violations', async () => {
    renderOverlay();
    expect(screen.queryByTestId('incoming-call')).toBeNull();
    act(() => emitDispatch('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['1'] }));
    const overlay = await screen.findByRole('alertdialog', { name: 'Incoming call from Mara' });
    expect(within(overlay).getByText('Incoming call')).toBeInTheDocument();
    for (const name of ['Accept', 'Accept with video', 'Decline'])
      expect(within(overlay).getByRole('button', { name })).toBeInTheDocument();
    expect(oscillators).toBeGreaterThan(0);
    const first = oscillators;
    act(() => {
      vi.advanceTimersByTime(3100);
    });
    expect(oscillators).toBeGreaterThan(first);
    expect(await axeViolations(overlay)).toEqual([]);
  });

  it('accept joins the call and removes the overlay', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderOverlay();
    act(() =>
      applyCallEvent('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['1'] }),
    );
    await user.click(await screen.findByRole('button', { name: 'Accept' }));
    expect(screen.queryByTestId('incoming-call')).toBeNull();
    expect(useVoiceSession.getState()).toMatchObject({ channelId: 'd1', guildId: '@me' });
    expect(sent.at(-1)).toMatchObject({ d: { guild_id: null, channel_id: 'd1' } });
  });

  it('accept with video turns the camera on once connected', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderOverlay();
    act(() =>
      applyCallEvent('CALL_CREATE', {
        channel_id: 'd1',
        initiator_id: '2',
        ringing: ['1'],
        video: true,
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Accept with video' }));
    expect(useVoiceSession.getState().camera).toBe(false);
    await act(async () => {
      useVoiceSession.setState({ status: 'connected' });
      for (let i = 0; i < 6; i++) await Promise.resolve();
    });
    expect(useVoiceSession.getState().camera).toBe(true);
  });

  it('G-39 decline tells the server to stop ringing', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const calls: string[] = [];
    const spy = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      calls.push(`${init?.method ?? 'GET'} ${String(input)}`);
      return Promise.resolve(new Response(null, { status: 204 }));
    });
    try {
      renderOverlay();
      act(() =>
        applyCallEvent('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['1'] }),
      );
      await user.click(await screen.findByRole('button', { name: 'Decline' }));
      await vi.waitFor(() => expect(calls).toContain('POST /api/v1/channels/d1/call/stop-ringing'));
      expect(screen.queryByTestId('incoming-call')).toBeNull();
    } finally {
      spy.mockRestore();
    }
  });

  it('the local fallback timer matches the server ring (45 s)', () => {
    expect(RING_TIMEOUT_MS).toBe(45_000);
  });

  it('decline dismisses without joining, and a ring times out by itself', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderOverlay();
    act(() =>
      applyCallEvent('CALL_CREATE', { channel_id: 'd1', initiator_id: '2', ringing: ['1'] }),
    );
    await user.click(await screen.findByRole('button', { name: 'Decline' }));
    expect(screen.queryByTestId('incoming-call')).toBeNull();
    expect(useVoiceSession.getState().channelId).toBeNull();
    act(() =>
      applyCallEvent('CALL_CREATE', { channel_id: 'd2', initiator_id: '3', ringing: ['1'] }),
    );
    expect(await screen.findByTestId('incoming-call')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(RING_TIMEOUT_MS + 100);
    });
    expect(screen.queryByTestId('incoming-call')).toBeNull();
  });
});
