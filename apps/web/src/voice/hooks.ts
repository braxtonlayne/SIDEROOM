/** React hooks over the voice stores. UI code reads voice state only through here. */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from '../store/store';
import type { VoiceState } from '../store/types';
import { browserAudioDeps, MicError, toMicError, type AudioDeps } from './deps';
import { MicPipeline } from './mic';
import { useVoiceSession } from './session';

export interface VoiceParticipant {
  userId: string;
  speaking: boolean;
  /** Muted by themselves or by a moderator. */
  muted: boolean;
  deafened: boolean;
  serverMuted: boolean;
  serverDeafened: boolean;
  video: boolean;
  streaming: boolean;
}

const EMPTY: VoiceParticipant[] = [];

function toParticipant(v: VoiceState, speaking: boolean): VoiceParticipant {
  return {
    userId: v.user_id,
    speaking,
    muted: !!v.self_mute || !!v.mute,
    deafened: !!v.self_deaf || !!v.deaf,
    serverMuted: !!v.mute,
    serverDeafened: !!v.deaf,
    video: !!v.self_video,
    streaming: !!v.self_stream,
  };
}

/** Everyone connected to a voice channel, in join order of the store, with live speaking flags. */
export function useVoiceParticipants(channelId: string | undefined): VoiceParticipant[] {
  const voiceStates = useStore((s) => s.voiceStates);
  const me = useStore((s) => s.currentUserId);
  const speaking = useVoiceSession((s) => s.speaking);
  const transmitting = useVoiceSession((s) => s.transmitting);
  return useMemo(() => {
    if (!channelId) return EMPTY;
    const out: VoiceParticipant[] = [];
    for (const guild of Object.values(voiceStates)) {
      for (const v of Object.values(guild)) {
        if (v.channel_id !== channelId) continue;
        out.push(toParticipant(v, v.user_id === me ? transmitting : !!speaking[v.user_id]));
      }
    }
    return out.length ? out : EMPTY;
  }, [voiceStates, me, speaking, transmitting, channelId]);
}

/** The viewer's own voice state (server mute / deafen), when connected. */
export function useOwnVoiceState(): VoiceState | undefined {
  const voiceStates = useStore((s) => s.voiceStates);
  const me = useStore((s) => s.currentUserId);
  return useMemo(() => {
    if (!me) return undefined;
    for (const guild of Object.values(voiceStates)) if (guild[me]) return guild[me];
    return undefined;
  }, [voiceStates, me]);
}

/** Output / input devices with hot-plug refresh. Labels appear once the microphone permission is granted. */
export interface DeviceLists {
  inputs: MediaDeviceInfo[];
  outputs: MediaDeviceInfo[];
  /** True once the browser exposes device labels (permission granted at least once). */
  labelled: boolean;
  refresh(): Promise<void>;
}

export function useAudioDevices(deps: AudioDeps = browserAudioDeps): DeviceLists {
  const [list, setList] = useState<MediaDeviceInfo[]>([]);
  const alive = useRef(true);
  const refresh = useCallback(async () => {
    const all = await deps.enumerateDevices().catch(() => []);
    if (alive.current) setList(all);
  }, [deps]);
  useEffect(() => {
    alive.current = true;
    void refresh();
    const off = deps.onDeviceChange(() => void refresh());
    return () => {
      alive.current = false;
      off();
    };
  }, [deps, refresh]);
  return useMemo(
    () => ({
      inputs: list.filter((d) => d.kind === 'audioinput'),
      outputs: list.filter((d) => d.kind === 'audiooutput'),
      labelled: list.some((d) => d.label !== ''),
      refresh,
    }),
    [list, refresh],
  );
}

export type PermissionStatus = PermissionState | 'unsupported';

/** The microphone permission, kept current where the Permissions API supports it. */
export function useMicPermission(deps: AudioDeps = browserAudioDeps): {
  status: PermissionStatus;
  recheck(): void;
} {
  const [status, setStatus] = useState<PermissionStatus>('prompt');
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let live = true;
    void deps.queryMicPermission().then((s) => {
      if (live) setStatus(s);
    });
    return () => {
      live = false;
    };
  }, [deps, tick]);
  return { status, recheck: () => setTick((t) => t + 1) };
}

export interface MicTestOptions {
  deviceId: string;
  echoCancellation: boolean;
  autoGainControl: boolean;
  inputVolume: number;
}

export interface MicTest {
  running: boolean;
  /** dBFS while running. */
  level: number;
  error: MicError | null;
  start(): Promise<void>;
  stop(): void;
}

/**
 * Open the microphone for a level meter (settings page "Let's check"). Restarts when the device or
 * constraints change; always releases the device on unmount.
 */
export function useMicTest(opts: MicTestOptions, deps: AudioDeps = browserAudioDeps): MicTest {
  const pipe = useRef<MicPipeline | null>(null);
  const [running, setRunning] = useState(false);
  const [level, setLevel] = useState(-100);
  const [error, setError] = useState<MicError | null>(null);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  const stop = useCallback(() => {
    pipe.current?.stop();
    pipe.current = null;
    setRunning(false);
    setLevel(-100);
  }, []);

  const start = useCallback(async () => {
    pipe.current?.stop();
    const p = new MicPipeline(deps);
    pipe.current = p;
    setError(null);
    try {
      await p.start(optsRef.current);
      if (pipe.current !== p) {
        p.stop();
        return;
      }
      p.onLevel(setLevel);
      p.onLost(() => {
        setError(toMicError(new DOMException('gone', 'NotFoundError')));
        stop();
      });
      setRunning(true);
    } catch (e) {
      pipe.current = null;
      setError(toMicError(e));
      setRunning(false);
    }
  }, [deps, stop]);

  const { deviceId, echoCancellation, autoGainControl } = opts;
  useEffect(() => {
    if (pipe.current?.active) void start();
    // Only a change to the capture settings restarts a running test.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deviceId, echoCancellation, autoGainControl]);

  useEffect(() => {
    pipe.current?.setInputVolume(opts.inputVolume);
  }, [opts.inputVolume]);

  useEffect(() => () => pipe.current?.stop(), []);

  return { running, level, error, start, stop };
}
