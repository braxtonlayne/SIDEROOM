// BYOS-CNV-002: the decision table. Given what the probe found in a source and
// what this browser can do, pick one of
//   native    - the browser plays it as is
//   remux     - re-wrap the packets (BYOS-CNV-001, ffmpeg.wasm `-c copy`)
//   webcodecs - decode and re-encode in this tab (this folder)
//   worker    - ffmpeg.wasm transcode in the tab (last resort, honest estimate)
//               or the optional transcoder worker (BYOS-CNV-003)
// All browser questions go through `Capabilities`, so the table is unit-tested
// with fakes; `detectCapabilities()` is the only code that touches globals.

import type {
  AudioPlan,
  AudioTrackInfo,
  BrowserEngine,
  Capabilities,
  ConvertPath,
  LadderStep,
  OutputContainer,
  SourceInfo,
  VideoPlan,
  VideoTrackInfo,
} from './webcodecs/types';

export type CodecFamily =
  | 'avc'
  | 'hevc'
  | 'vp8'
  | 'vp9'
  | 'av1'
  | 'aac'
  | 'mp3'
  | 'opus'
  | 'vorbis'
  | 'flac'
  | 'ac3'
  | 'eac3'
  | 'other';

/** Maps a WebCodecs codec string to its family. */
export function codecFamily(codec: string): CodecFamily {
  const c = codec.toLowerCase();
  if (c.startsWith('avc1') || c.startsWith('avc3')) return 'avc';
  if (c.startsWith('hvc1') || c.startsWith('hev1')) return 'hevc';
  if (c === 'vp8' || c === 'vp08') return 'vp8';
  if (c.startsWith('vp09') || c === 'vp9') return 'vp9';
  if (c.startsWith('av01')) return 'av1';
  if (c.startsWith('mp4a.40') || c === 'aac') return 'aac';
  if (c === 'mp3' || c === 'mp4a.6b' || c === 'mp4a.69') return 'mp3';
  if (c === 'opus') return 'opus';
  if (c === 'vorbis') return 'vorbis';
  if (c === 'flac') return 'flac';
  if (c === 'ac-3') return 'ac3';
  if (c === 'ec-3') return 'eac3';
  return 'other';
}

const MP4_VIDEO: CodecFamily[] = ['avc', 'hevc', 'vp9', 'av1'];
const MP4_AUDIO: CodecFamily[] = ['aac', 'mp3', 'opus', 'flac'];
const WEBM_VIDEO: CodecFamily[] = ['vp8', 'vp9', 'av1'];
const WEBM_AUDIO: CodecFamily[] = ['opus', 'vorbis'];

/** The ladder: the step is chosen from the source height and never upscales. */
export const LADDER: readonly LadderStep[] = [
  { label: '480p', height: 480, videoBitrate: 1_500_000, avc: 'avc1.64001e' },
  { label: '720p', height: 720, videoBitrate: 3_000_000, avc: 'avc1.64001f' },
  { label: '1080p', height: 1080, videoBitrate: 6_000_000, avc: 'avc1.640028' },
];

export function pickLadderStep(sourceHeight: number): LadderStep {
  return LADDER.find((s) => sourceHeight <= s.height) ?? LADDER[LADDER.length - 1]!;
}

/** Target size for a step: height capped at the step, width keeps the aspect, both even. */
export function ladderSize(src: { width: number; height: number }, step: LadderStep) {
  const height = Math.max(2, Math.min(src.height, step.height) & ~1);
  const width = Math.max(2, Math.round((src.width * height) / src.height / 2) * 2);
  return { width, height };
}

function mimeFor(container: OutputContainer, video?: string, audio?: string): string {
  const codecs = [video, audio].filter(Boolean).join(', ');
  const type = video ? 'video' : 'audio';
  return `${type}/${container}${codecs ? `; codecs="${codecs}"` : ''}`;
}

/** Can the source track be re-wrapped into `container` and played here? */
function trackPlayable(
  caps: Capabilities,
  container: OutputContainer,
  kind: 'video' | 'audio',
  codec: string,
): boolean {
  const fam = codecFamily(codec);
  const allowed =
    container === 'mp4'
      ? kind === 'video'
        ? MP4_VIDEO
        : MP4_AUDIO
      : kind === 'video'
        ? WEBM_VIDEO
        : WEBM_AUDIO;
  if (!allowed.includes(fam)) return false;
  return caps.canPlay(
    kind === 'video' ? mimeFor(container, codec) : mimeFor(container, undefined, codec),
  );
}

function nativeContainer(info: SourceInfo): OutputContainer | null {
  return info.container === 'mp4' || info.container === 'webm' ? info.container : null;
}

/** ffmpeg.wasm (single thread, software x264) speed as a fraction of real time. */
export function wasmSpeed(video: VideoTrackInfo | undefined): number {
  if (!video) return 20;
  const pixels = Math.max(1, video.width * video.height);
  return Math.min(1, (0.2 * (1280 * 720)) / pixels);
}

export function estimateWasmMinutes(info: SourceInfo): number {
  return Math.max(1, Math.ceil(info.durationSec / wasmSpeed(info.video) / 60));
}

async function planVideo(
  info: SourceInfo,
  caps: Capabilities,
  container: OutputContainer,
): Promise<VideoPlan | null | 'fail'> {
  const v = info.video;
  if (!v) return null;
  if (trackPlayable(caps, container, 'video', v.codec)) return { mode: 'copy', codec: v.codec };
  // Must be decodable here before anything else matters.
  const decodable = await caps.videoDecode({
    codec: v.codec,
    codedWidth: v.width,
    codedHeight: v.height,
  });
  if (!decodable) return 'fail';
  const step = pickLadderStep(v.height);
  const { width, height } = ladderSize(v, step);
  const framerate = v.frameRate ?? 30;
  const tries: Array<{ to: 'avc' | 'vp9' | 'vp8'; codec: string }> =
    container === 'mp4'
      ? [{ to: 'avc', codec: step.avc }]
      : [
          { to: 'vp9', codec: 'vp09.00.10.08' },
          { to: 'vp8', codec: 'vp8' },
        ];
  for (const t of tries) {
    if (
      await caps.videoEncode({
        codec: t.codec,
        width,
        height,
        bitrate: step.videoBitrate,
        framerate,
      })
    ) {
      return { mode: 'transcode', to: t.to, step, width, height, codec: t.codec };
    }
  }
  return 'fail';
}

async function planAudio(
  a: AudioTrackInfo | undefined,
  caps: Capabilities,
  container: OutputContainer,
): Promise<AudioPlan | 'fail'> {
  if (!a) return { mode: 'none' };
  // The muxers can only write AAC (MP4) and Opus; other playable audio is re-encoded here.
  const copyable: CodecFamily[] = container === 'mp4' ? ['aac', 'opus'] : ['opus'];
  if (copyable.includes(codecFamily(a.codec)) && trackPlayable(caps, container, 'audio', a.codec)) {
    return { mode: 'copy', codec: a.codec };
  }
  const decodable = await caps.audioDecode({
    codec: a.codec,
    sampleRate: a.sampleRate,
    numberOfChannels: a.channels,
  });
  if (!decodable) return 'fail';
  const channels = Math.min(2, Math.max(1, a.channels));
  const sampleRate = 48_000;
  const bitrate = channels === 1 ? 96_000 : 160_000;
  const to: 'aac' | 'opus' = container === 'mp4' ? 'aac' : 'opus';
  const codec = to === 'aac' ? 'mp4a.40.2' : 'opus';
  const ok = await caps.audioEncode({ codec, sampleRate, numberOfChannels: channels, bitrate });
  return ok ? { mode: 'transcode', to, codec, bitrate, sampleRate, channels } : 'fail';
}

/**
 * The decision table. Order: native, remux, WebCodecs (MP4 first, WebM when MP4
 * is not feasible: AAC encoding is Chromium-and-Safari only), then the worker.
 */
export async function decide(info: SourceInfo, caps: Capabilities): Promise<ConvertPath> {
  const v = info.video?.codec;
  const a = info.audio?.codec;
  if (!v && !a)
    return { path: 'worker', reason: 'no audio or video track found', estimateMinutes: 1 };

  const native = nativeContainer(info);
  const playableIn = (c: OutputContainer) =>
    (!v || trackPlayable(caps, c, 'video', v)) && (!a || trackPlayable(caps, c, 'audio', a));

  if (native && playableIn(native)) {
    return { path: 'native', reason: `${native} with codecs this browser plays` };
  }
  for (const c of ['mp4', 'webm'] as const) {
    if (playableIn(c))
      return {
        path: 'remux',
        container: c,
        reason: `codecs play as ${c}; only the container changes`,
      };
  }

  const whyNot: string[] = [];
  for (const container of ['mp4', 'webm'] as const) {
    const video = await planVideo(info, caps, container);
    if (video === 'fail') {
      whyNot.push(`${container}: no ${v} decode or encoder`);
      continue;
    }
    const audio = await planAudio(info.audio, caps, container);
    if (audio === 'fail') {
      whyNot.push(`${container}: no ${a} decode or encoder`);
      continue;
    }
    const parts = [
      video
        ? video.mode === 'copy'
          ? 'video copied'
          : `video to ${video.to} ${video.step.label}`
        : null,
      audio.mode === 'transcode'
        ? `audio to ${audio.to}`
        : audio.mode === 'copy'
          ? 'audio copied'
          : null,
    ].filter(Boolean);
    return {
      path: 'webcodecs',
      container,
      video,
      audio,
      reason: `${parts.join(', ')} as ${container}`,
    };
  }
  return {
    path: 'worker',
    estimateMinutes: estimateWasmMinutes(info),
    reason: `WebCodecs cannot do it here (${whyNot.join('; ')})`,
  };
}

/** One line for logs and the Playwright annotation: which engine took which path. */
export function describeDecision(engine: BrowserEngine, d: ConvertPath): string {
  return `${engine}:${d.path}${'container' in d ? `/${d.container}` : ''} - ${d.reason}`;
}

function engineFromUserAgent(ua: string): BrowserEngine {
  if (/firefox\//i.test(ua)) return 'gecko';
  if (/chrome\/|chromium\/|edg\//i.test(ua)) return 'chromium';
  if (/safari\//i.test(ua)) return 'webkit';
  return 'unknown';
}

async function supported(fn: () => Promise<{ supported?: boolean }>): Promise<boolean> {
  try {
    return (await fn()).supported === true;
  } catch {
    return false;
  }
}

/** The real capabilities of this browser. Absent APIs answer false. */
export interface CodecGlobals {
  VideoDecoder?: typeof VideoDecoder;
  VideoEncoder?: typeof VideoEncoder;
  AudioDecoder?: typeof AudioDecoder;
  AudioEncoder?: typeof AudioEncoder;
}

export function detectCapabilities(
  g: CodecGlobals = globalThis as unknown as CodecGlobals,
): Capabilities {
  const video = typeof document !== 'undefined' ? document.createElement('video') : null;
  return {
    engine: engineFromUserAgent(typeof navigator === 'undefined' ? '' : navigator.userAgent),
    canPlay: (mime) => (video ? video.canPlayType(mime) !== '' : false),
    videoDecode: (c) =>
      g.VideoDecoder
        ? supported(() => g.VideoDecoder!.isConfigSupported(c))
        : Promise.resolve(false),
    audioDecode: (c) =>
      g.AudioDecoder
        ? supported(() => g.AudioDecoder!.isConfigSupported(c))
        : Promise.resolve(false),
    videoEncode: (c) =>
      g.VideoEncoder
        ? supported(() => g.VideoEncoder!.isConfigSupported(c))
        : Promise.resolve(false),
    audioEncode: (c) =>
      g.AudioEncoder
        ? supported(() => g.AudioEncoder!.isConfigSupported(c))
        : Promise.resolve(false),
  };
}
