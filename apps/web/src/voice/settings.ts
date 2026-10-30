/**
 * Voice & Video preferences (LNG-USET-043..049, 053). Device-local: input and output devices, key bindings and
 * per-person volumes make no sense on another machine, so they persist to localStorage only and never sync.
 */
import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';
import { DEFAULT_THRESHOLD_DB, THRESHOLD_RANGE } from './vad';
import { sameKeybind, sanitizeKeybind, type Keybind } from './keybind';

export type InputMode = 'vad' | 'ptt';
export type KeybindAction = 'ptt' | 'toggleMute' | 'toggleDeafen' | 'disconnect';

export const KEYBIND_ACTIONS: { id: KeybindAction; label: string; description: string }[] = [
  { id: 'ptt', label: 'Push to Talk', description: 'Hold to transmit while Push to Talk is on.' },
  { id: 'toggleMute', label: 'Toggle Mute', description: 'Mute or unmute your microphone.' },
  { id: 'toggleDeafen', label: 'Toggle Deafen', description: 'Deafen or undeafen yourself.' },
  { id: 'disconnect', label: 'Disconnect from Voice', description: 'Leave the voice channel.' },
];

export const PTT_RELEASE_RANGE = [0, 2000] as const;
export const VOLUME_RANGE = [0, 200] as const;

export interface VoiceSettings {
  /** `deviceId` of the microphone; "default" follows the system default. */
  inputDeviceId: string;
  outputDeviceId: string;
  /** Microphone gain, percent 0..200. */
  inputVolume: number;
  /** Master volume of everyone else, percent 0..200. */
  outputVolume: number;
  inputMode: InputMode;
  autoSensitivity: boolean;
  /** Manual voice-activity threshold, dBFS. */
  thresholdDb: number;
  /** Milliseconds the mic stays open after the PTT key is released, 0..2000. */
  pttReleaseMs: number;
  /** Browser constraints (noise suppression is intentionally absent, D-045). */
  echoCancellation: boolean;
  autoGainControl: boolean;
  soundJoinLeave: boolean;
  soundMuteDeafen: boolean;
  soundPtt: boolean;
  /** The incoming-call ring. */
  soundRing: boolean;
  /** Ask the network for high packet priority on the voice line (QoS, LNG-USET-050); off by default. */
  qosHighPriority: boolean;
  /** List the verification codes in the call's shield (LNG-USET-050). */
  showPrivacyCodes: boolean;
  /** Volume of the cue sounds, percent 0..100. */
  soundVolume: number;
  /** Percent the other voices are lowered while you speak, 0 (off) to 100. */
  attenuation: number;
  /** Camera chosen in Settings; "default" follows the system. */
  cameraDeviceId: string;
  /** Show a preview and ask before the camera goes live. */
  alwaysPreviewVideo: boolean;
  /** Screen share defaults: quality preset id, share-audio switch and content hint. */
  sharePresetId: string;
  shareAudio: boolean;
  shareHint: 'motion' | 'detail';
  keybinds: Record<KeybindAction, Keybind | null>;
  /** Per-person playback volume, percent 0..200 (absent = 100). */
  userVolumes: Record<string, number>;
  /** People muted on this device only. */
  localMuted: Record<string, true>;
  /** Music played into the call (MUS-JB-001): its own volume, percent 0..200, apart from the voices. */
  musicVolume: number;
  /** Do not play (or receive) the music track of anyone in the call; the voices are untouched. */
  musicMuted: boolean;
}

export const DEFAULT_VOICE_SETTINGS: VoiceSettings = {
  inputDeviceId: 'default',
  outputDeviceId: 'default',
  inputVolume: 100,
  outputVolume: 100,
  inputMode: 'vad',
  autoSensitivity: true,
  thresholdDb: DEFAULT_THRESHOLD_DB,
  pttReleaseMs: 200,
  echoCancellation: true,
  autoGainControl: true,
  soundJoinLeave: true,
  soundMuteDeafen: true,
  soundPtt: true,
  soundRing: true,
  qosHighPriority: false,
  showPrivacyCodes: true,
  soundVolume: 60,
  attenuation: 0,
  cameraDeviceId: 'default',
  alwaysPreviewVideo: false,
  sharePresetId: '720p30',
  shareAudio: true,
  shareHint: 'motion',
  keybinds: { ptt: null, toggleMute: null, toggleDeafen: null, disconnect: null },
  userVolumes: {},
  localMuted: {},
  musicVolume: 100,
  musicMuted: false,
};

const clamp = (n: unknown, lo: number, hi: number, fallback: number) =>
  typeof n === 'number' && Number.isFinite(n)
    ? Math.min(hi, Math.max(lo, Math.round(n)))
    : fallback;
const bool = (v: unknown, fallback: boolean) => (typeof v === 'boolean' ? v : fallback);
const text = (v: unknown, fallback: string) =>
  typeof v === 'string' && v.length > 0 && v.length < 512 ? v : fallback;

/** Stale or hand-edited storage can never break the engine: every field is validated. */
export function sanitizeVoiceSettings(raw: unknown): VoiceSettings {
  const r = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_VOICE_SETTINGS;
  const kb = (typeof r.keybinds === 'object' && r.keybinds !== null ? r.keybinds : {}) as Record<
    string,
    unknown
  >;
  const volumes: Record<string, number> = {};
  if (typeof r.userVolumes === 'object' && r.userVolumes !== null) {
    for (const [id, v] of Object.entries(r.userVolumes as Record<string, unknown>)) {
      const n = clamp(v, VOLUME_RANGE[0], VOLUME_RANGE[1], 100);
      if (n !== 100) volumes[id] = n;
    }
  }
  const muted: Record<string, true> = {};
  if (typeof r.localMuted === 'object' && r.localMuted !== null) {
    for (const [id, v] of Object.entries(r.localMuted as Record<string, unknown>))
      if (v === true) muted[id] = true;
  }
  return {
    inputDeviceId: text(r.inputDeviceId, d.inputDeviceId),
    outputDeviceId: text(r.outputDeviceId, d.outputDeviceId),
    inputVolume: clamp(r.inputVolume, VOLUME_RANGE[0], VOLUME_RANGE[1], d.inputVolume),
    outputVolume: clamp(r.outputVolume, VOLUME_RANGE[0], VOLUME_RANGE[1], d.outputVolume),
    inputMode: r.inputMode === 'ptt' ? 'ptt' : 'vad',
    autoSensitivity: bool(r.autoSensitivity, d.autoSensitivity),
    thresholdDb: clamp(r.thresholdDb, THRESHOLD_RANGE[0], THRESHOLD_RANGE[1], d.thresholdDb),
    pttReleaseMs: clamp(r.pttReleaseMs, PTT_RELEASE_RANGE[0], PTT_RELEASE_RANGE[1], d.pttReleaseMs),
    echoCancellation: bool(r.echoCancellation, d.echoCancellation),
    autoGainControl: bool(r.autoGainControl, d.autoGainControl),
    soundJoinLeave: bool(r.soundJoinLeave, d.soundJoinLeave),
    soundMuteDeafen: bool(r.soundMuteDeafen, d.soundMuteDeafen),
    soundPtt: bool(r.soundPtt, d.soundPtt),
    soundRing: bool(r.soundRing, d.soundRing),
    qosHighPriority: bool(r.qosHighPriority, d.qosHighPriority),
    showPrivacyCodes: bool(r.showPrivacyCodes, d.showPrivacyCodes),
    attenuation: clamp(r.attenuation, 0, 100, d.attenuation),
    cameraDeviceId: text(r.cameraDeviceId, d.cameraDeviceId),
    alwaysPreviewVideo: bool(r.alwaysPreviewVideo, d.alwaysPreviewVideo),
    sharePresetId: text(r.sharePresetId, d.sharePresetId),
    shareAudio: bool(r.shareAudio, d.shareAudio),
    shareHint: r.shareHint === 'detail' ? 'detail' : 'motion',
    soundVolume: clamp(r.soundVolume, 0, 100, d.soundVolume),
    keybinds: {
      ptt: sanitizeKeybind(kb.ptt),
      toggleMute: sanitizeKeybind(kb.toggleMute),
      toggleDeafen: sanitizeKeybind(kb.toggleDeafen),
      disconnect: sanitizeKeybind(kb.disconnect),
    },
    userVolumes: volumes,
    localMuted: muted,
    musicVolume: clamp(r.musicVolume, VOLUME_RANGE[0], VOLUME_RANGE[1], d.musicVolume),
    musicMuted: bool(r.musicMuted, d.musicMuted),
  };
}

const safeStorage: StateStorage = {
  getItem: (k) => {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  setItem: (k, v) => {
    try {
      localStorage.setItem(k, v);
    } catch {
      /* storage unavailable: settings simply are not persisted */
    }
  },
  removeItem: (k) => {
    try {
      localStorage.removeItem(k);
    } catch {
      /* ignore */
    }
  },
};

interface VoiceSettingsState {
  settings: VoiceSettings;
  update(patch: Partial<VoiceSettings>): void;
  setKeybind(action: KeybindAction, bind: Keybind | null): void;
  setUserVolume(userId: string, percent: number): void;
  setLocalMuted(userId: string, muted: boolean): void;
  reset(): void;
}

export const useVoiceSettings = create<VoiceSettingsState>()(
  persist(
    (set) => ({
      settings: DEFAULT_VOICE_SETTINGS,
      update: (patch) =>
        set((s) => ({ settings: sanitizeVoiceSettings({ ...s.settings, ...patch }) })),
      setKeybind: (action, bind) =>
        set((s) => ({
          settings: sanitizeVoiceSettings({
            ...s.settings,
            keybinds: { ...s.settings.keybinds, [action]: bind },
          }),
        })),
      setUserVolume: (userId, percent) =>
        set((s) => ({
          settings: sanitizeVoiceSettings({
            ...s.settings,
            userVolumes: { ...s.settings.userVolumes, [userId]: percent },
          }),
        })),
      setLocalMuted: (userId, muted) =>
        set((s) => {
          const next = { ...s.settings.localMuted };
          if (muted) next[userId] = true;
          else delete next[userId];
          return { settings: { ...s.settings, localMuted: next } };
        }),
      reset: () => set({ settings: DEFAULT_VOICE_SETTINGS }),
    }),
    {
      name: 'sr.voice.settings',
      version: 1,
      storage: createJSONStorage(() => safeStorage),
      partialize: (s) => ({ settings: s.settings }),
      merge: (persisted, current) => ({
        ...current,
        settings: sanitizeVoiceSettings(
          (persisted as { settings?: unknown } | undefined)?.settings,
        ),
      }),
    },
  ),
);

export const getVoiceSettings = () => useVoiceSettings.getState().settings;

/** The mode in effect: a channel that denies USE_VAD forces Push to Talk (D-046). */
export function effectiveInputMode(
  s: Pick<VoiceSettings, 'inputMode'>,
  pttForced: boolean,
): InputMode {
  return pttForced ? 'ptt' : s.inputMode;
}

export const userVolumeOf = (s: VoiceSettings, userId: string) => s.userVolumes[userId] ?? 100;

/** Another voice action that already uses this bind, as a message; null when the bind is free. */
export function keybindConflict(
  binds: VoiceSettings['keybinds'],
  action: KeybindAction,
  bind: Keybind,
): string | null {
  const other = KEYBIND_ACTIONS.find((a) => a.id !== action && sameKeybind(binds[a.id], bind));
  return other ? `That key is already used for "${other.label}".` : null;
}
