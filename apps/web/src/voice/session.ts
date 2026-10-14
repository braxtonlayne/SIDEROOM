/**
 * The viewer's live voice session as UI state (VOX-CLI-010: a global store, never unmounted with a route).
 * The controller (controller.ts) is the only writer; components read through the selectors/hooks here.
 */
import { create } from 'zustand';
import { UNKNOWN_QUALITY, type ConnectionQuality } from './quality';

export type SessionStatus =
  'idle' | 'joining' | 'connecting' | 'connected' | 'reconnecting' | 'error';

export interface SessionError {
  code: 'mic' | 'connect' | 'gateway' | 'disconnected' | 'timeout' | 'refused' | 'replaced';
  message: string;
  /** Present for microphone failures so the UI can offer the right fix. */
  mic?: 'permission-denied' | 'no-device' | 'in-use' | 'unsupported' | 'unknown';
}

export interface VoiceSession {
  status: SessionStatus;
  guildId: string | null;
  channelId: string | null;
  channelName: string | null;
  selfMute: boolean;
  selfDeaf: boolean;
  /** A moderator muted / deafened the viewer. */
  serverMute: boolean;
  serverDeaf: boolean;
  /** The channel denies USE_VAD: Push to Talk is forced. */
  pttForced: boolean;
  pttActive: boolean;
  /** The microphone is open and sending right now. */
  transmitting: boolean;
  speaking: Record<string, boolean>;
  quality: ConnectionQuality;
  error: SessionError | null;
}

export const IDLE_SESSION: VoiceSession = {
  status: 'idle',
  guildId: null,
  channelId: null,
  channelName: null,
  selfMute: false,
  selfDeaf: false,
  serverMute: false,
  serverDeaf: false,
  pttForced: false,
  pttActive: false,
  transmitting: false,
  speaking: {},
  quality: UNKNOWN_QUALITY,
  error: null,
};

export const useVoiceSession = create<VoiceSession>()(() => IDLE_SESSION);

export const patchSession = (patch: Partial<VoiceSession>) => useVoiceSession.setState(patch);
export const resetSession = (keep: Partial<VoiceSession> = {}) =>
  useVoiceSession.setState({ ...IDLE_SESSION, ...keep });

/** True from the moment the viewer asks to join until they leave (even while reconnecting). */
export const isInVoice = (s: Pick<VoiceSession, 'status'>) =>
  s.status !== 'idle' && s.status !== 'error';
