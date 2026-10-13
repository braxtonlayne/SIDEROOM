/**
 * Browser media seam. Everything the voice code needs from the platform (getUserMedia, WebAudio, audio
 * elements, device lists) goes through `AudioDeps`, so unit tests inject fakes and never touch real devices.
 */
export interface AudioDeps {
  getUserMedia(constraints: MediaStreamConstraints): Promise<MediaStream>;
  /** A fresh AudioContext, or null where WebAudio is unavailable (tests, very old browsers). */
  createAudioContext(): AudioContext | null;
  createAudioElement(): HTMLAudioElement;
  enumerateDevices(): Promise<MediaDeviceInfo[]>;
  /** Subscribe to device hot-plug; returns the unsubscribe function. */
  onDeviceChange(cb: () => void): () => void;
  queryMicPermission(): Promise<PermissionState | 'unsupported'>;
}

export const browserAudioDeps: AudioDeps = {
  getUserMedia: (c) => {
    const md = navigator.mediaDevices;
    if (!md?.getUserMedia)
      return Promise.reject(new DOMException('No media devices', 'NotFoundError'));
    return md.getUserMedia(c);
  },
  createAudioContext: () => {
    const Ctor =
      globalThis.AudioContext ??
      (globalThis as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctor) return null;
    try {
      return new Ctor({ latencyHint: 'interactive' });
    } catch {
      return null;
    }
  },
  createAudioElement: () => {
    const el = document.createElement('audio');
    el.autoplay = true;
    return el;
  },
  enumerateDevices: async () => {
    const md = navigator.mediaDevices;
    return md?.enumerateDevices ? md.enumerateDevices() : [];
  },
  onDeviceChange: (cb) => {
    const md = navigator.mediaDevices;
    if (!md?.addEventListener) return () => undefined;
    md.addEventListener('devicechange', cb);
    return () => md.removeEventListener('devicechange', cb);
  },
  queryMicPermission: async () => {
    try {
      const p = await navigator.permissions?.query({ name: 'microphone' as PermissionName });
      return p ? p.state : 'unsupported';
    } catch {
      return 'unsupported';
    }
  },
};

/** Why a microphone could not be opened, in words a person can act on. */
export type MicErrorCode = 'permission-denied' | 'no-device' | 'in-use' | 'unsupported' | 'unknown';

export class MicError extends Error {
  constructor(
    readonly code: MicErrorCode,
    message: string,
  ) {
    super(message);
    this.name = 'MicError';
  }
}

export function toMicError(e: unknown): MicError {
  if (e instanceof MicError) return e;
  const name = e instanceof DOMException || e instanceof Error ? e.name : '';
  switch (name) {
    case 'NotAllowedError':
    case 'SecurityError':
    case 'PermissionDeniedError':
      return new MicError(
        'permission-denied',
        'Microphone access is blocked. Allow it in your browser (the lock icon in the address bar), then try again.',
      );
    case 'NotFoundError':
    case 'DevicesNotFoundError':
    case 'OverconstrainedError':
      return new MicError(
        'no-device',
        'No microphone was found. Plug one in or pick another input device.',
      );
    case 'NotReadableError':
    case 'AbortError':
      return new MicError(
        'in-use',
        'The microphone is busy. Close other apps that use it and try again.',
      );
    default:
      return new MicError('unknown', 'The microphone could not be started.');
  }
}

/** Audio-only constraints (D-045: noise suppression is always off; echo cancellation and AGC are the user's choice). */
export function micConstraints(o: {
  deviceId: string;
  echoCancellation: boolean;
  autoGainControl: boolean;
}): MediaStreamConstraints {
  return {
    audio: {
      ...(o.deviceId && o.deviceId !== 'default' ? { deviceId: { exact: o.deviceId } } : {}),
      echoCancellation: o.echoCancellation,
      autoGainControl: o.autoGainControl,
      noiseSuppression: false,
      channelCount: 1,
      sampleRate: 48000,
    },
    video: false,
  };
}
