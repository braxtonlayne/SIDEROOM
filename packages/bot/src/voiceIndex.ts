// `@sideroom/bot/voice`: bots in voice. A separate entry point so that bots that never join a call do not load
// the WebRTC stack. Names mirror `sr_sdk::voice` and `sr_sdk::voice_transport`.
export { E2eeCall, E2eeMedia, Identity, VoiceError, FALLBACK_MS, GRACE_MS, OPUS } from './voice';
export type { Action, EpochKeys } from './voice';
export {
  VoiceLink,
  VoiceServerUpdate,
  VoiceSession,
  TransportError,
  closeIsRetryable,
  defaultReconnectPolicy,
  voiceStateBody,
} from './voiceTransport';
export type {
  ConnectOptions,
  FrameTransform,
  ReconnectPolicy,
  TransportErrorKind,
  VoiceEvent,
} from './voiceTransport';
export {
  AudioError,
  FRAME_SAMPLES,
  OpusDecoder,
  OpusEncoder,
  SAMPLE_RATE,
  audioLevelDbov,
} from './audio';
export type { Application, Channels, Closeable } from './audio';
