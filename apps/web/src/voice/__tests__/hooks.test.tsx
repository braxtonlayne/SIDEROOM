import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { resetStore, useStore } from '../../store';
import { useAudioDevices, useMicPermission, useMicTest, useVoiceParticipants } from '../hooks';
import { patchSession, resetSession } from '../session';
import { FakeAnalyser, fakeAudio } from './fakes';

const device = (kind: MediaDeviceKind, deviceId: string, label: string) =>
  ({ kind, deviceId, label, groupId: 'g', toJSON: () => ({}) }) as MediaDeviceInfo;

beforeEach(() => {
  resetStore();
  resetSession();
});
afterEach(() => resetStore());

describe('useAudioDevices', () => {
  it('lists inputs and outputs and refreshes when a device is plugged in', async () => {
    const w = fakeAudio();
    w.devices = [device('audioinput', 'a', 'Built-in mic'), device('audiooutput', 'o', 'Speakers')];
    const { result } = renderHook(() => useAudioDevices(w.deps));
    await waitFor(() => expect(result.current.inputs).toHaveLength(1));
    expect(result.current.outputs.map((d) => d.label)).toEqual(['Speakers']);
    expect(result.current.labelled).toBe(true);
    w.devices = [...w.devices, device('audioinput', 'usb', 'USB mic')];
    act(() => w.emitDeviceChange());
    await waitFor(() => expect(result.current.inputs).toHaveLength(2));
  });

  it('reports unlabelled devices before the microphone permission is granted', async () => {
    const w = fakeAudio();
    w.devices = [device('audioinput', '', '')];
    const { result } = renderHook(() => useAudioDevices(w.deps));
    await waitFor(() => expect(result.current.inputs).toHaveLength(1));
    expect(result.current.labelled).toBe(false);
  });
});

describe('useMicPermission', () => {
  it('reads the permission state and re-checks on demand', async () => {
    const w = fakeAudio();
    let state: PermissionState | 'unsupported' = 'prompt';
    w.deps.queryMicPermission = async () => state;
    const { result } = renderHook(() => useMicPermission(w.deps));
    await waitFor(() => expect(result.current.status).toBe('prompt'));
    state = 'denied';
    act(() => result.current.recheck());
    await waitFor(() => expect(result.current.status).toBe('denied'));
  });
});

describe('useMicTest', () => {
  const opts = {
    deviceId: 'default',
    echoCancellation: true,
    autoGainControl: true,
    inputVolume: 100,
  };

  it('opens the mic, reports a level, and releases the device on stop and unmount', async () => {
    const w = fakeAudio();
    const { result, unmount } = renderHook(() => useMicTest(opts, w.deps));
    await act(() => result.current.start());
    expect(result.current.running).toBe(true);
    (w.contexts[0]!.analysers[0] as FakeAnalyser).amplitude = 0.1;
    await waitFor(() => expect(result.current.level).toBeGreaterThan(-30));
    act(() => result.current.stop());
    expect(result.current.running).toBe(false);
    expect(w.contexts[0]!.closed).toBe(true);
    await act(() => result.current.start());
    unmount();
    expect(w.contexts[1]!.closed).toBe(true);
  });

  it('surfaces a permission error', async () => {
    const w = fakeAudio();
    w.gum.mockRejectedValue(new DOMException('no', 'NotAllowedError'));
    const { result } = renderHook(() => useMicTest(opts, w.deps));
    await act(() => result.current.start());
    expect(result.current.running).toBe(false);
    expect(result.current.error?.code).toBe('permission-denied');
  });

  it('restarts a running test on another device', async () => {
    const w = fakeAudio();
    const { result, rerender } = renderHook((o) => useMicTest(o, w.deps), { initialProps: opts });
    await act(() => result.current.start());
    rerender({ ...opts, deviceId: 'usb-2' });
    await waitFor(() => expect(w.gum).toHaveBeenCalledTimes(2));
    const audio = (w.gum.mock.calls[1]![0] as MediaStreamConstraints)
      .audio as MediaTrackConstraints;
    expect(audio.deviceId).toEqual({ exact: 'usb-2' });
  });
});

describe('useVoiceParticipants', () => {
  it('maps voice states of a channel and merges speaking flags and self transmit', () => {
    useStore.getState().applyDispatch('READY', { user: { id: 'me', username: 'me' }, guilds: [] });
    for (const [user_id, extra] of [
      ['me', {}],
      ['u2', { self_mute: true }],
      ['u3', { deaf: true, mute: true }],
    ] as const) {
      useStore.getState().applyDispatch('VOICE_STATE_UPDATE', {
        guild_id: 'g1',
        channel_id: 'c1',
        user_id,
        ...extra,
      });
    }
    useStore
      .getState()
      .applyDispatch('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c2', user_id: 'other' });
    const { result } = renderHook(() => useVoiceParticipants('c1'));
    expect(result.current.map((p) => p.userId)).toEqual(['me', 'u2', 'u3']);
    expect(result.current.find((p) => p.userId === 'u2')).toMatchObject({
      muted: true,
      deafened: false,
    });
    expect(result.current.find((p) => p.userId === 'u3')).toMatchObject({
      muted: true,
      deafened: true,
      serverMuted: true,
      serverDeafened: true,
    });
    act(() => patchSession({ speaking: { u2: true }, transmitting: true }));
    expect(result.current.find((p) => p.userId === 'u2')!.speaking).toBe(true);
    expect(result.current.find((p) => p.userId === 'me')!.speaking).toBe(true);
    expect(result.current.find((p) => p.userId === 'u3')!.speaking).toBe(false);
  });

  it('is empty for no channel', () => {
    expect(renderHook(() => useVoiceParticipants(undefined)).result.current).toEqual([]);
  });
});
