// BYOS-CNV-002: the small contract between the WebCodecs path and the rest of
// `convert/` (conv-remux owns the Converter interface and calls us through
// `decide()` and `transcodeWithWebCodecs()`; see the handoff for the call shape).
// Nothing here touches the DOM, so it is safe to import from a worker.

/** Containers a source can arrive in; `unknown` is anything the sniffer could not name. */
export type SourceContainer = 'mp4' | 'webm' | 'mkv' | 'avi' | 'ts' | 'ogg' | 'unknown';

/** What conv-remux's probe knows about the first video track. Codec strings are WebCodecs ones. */
export interface VideoTrackInfo {
  /** WebCodecs codec string, e.g. `avc1.640028`, `hvc1.1.6.L120.90`, `vp09.00.10.08`, `av01.0.08M.08`. */
  codec: string;
  width: number;
  height: number;
  frameRate?: number;
}

export interface AudioTrackInfo {
  /** WebCodecs codec string: `mp4a.40.2`, `opus`, `vorbis`, `flac`, `mp3`, `ac-3`, `ec-3`. */
  codec: string;
  sampleRate: number;
  channels: number;
}

export interface SourceInfo {
  container: SourceContainer;
  video?: VideoTrackInfo;
  audio?: AudioTrackInfo;
  durationSec: number;
  sizeBytes: number;
}

export type BrowserEngine = 'chromium' | 'gecko' | 'webkit' | 'unknown';

/**
 * What this browser can do. `decide()` only ever asks through this interface so
 * tests can pass fakes; `detectCapabilities()` builds the real one.
 */
export interface Capabilities {
  engine: BrowserEngine;
  /** `HTMLMediaElement.canPlayType(mime)` is `probably` or `maybe`. */
  canPlay(mime: string): boolean;
  videoDecode(config: VideoDecoderConfig): Promise<boolean>;
  audioDecode(config: AudioDecoderConfig): Promise<boolean>;
  videoEncode(config: VideoEncoderConfig): Promise<boolean>;
  audioEncode(config: AudioEncoderConfig): Promise<boolean>;
}

/** Output containers the WebCodecs path can write. */
export type OutputContainer = 'mp4' | 'webm';

/** The ladder step chosen from the source size (never upscales). */
export interface LadderStep {
  label: '480p' | '720p' | '1080p';
  /** Target height; width follows the source aspect ratio, rounded to even. */
  height: number;
  videoBitrate: number;
  /** H.264 codec string used for this step. */
  avc: string;
}

export type VideoPlan =
  | { mode: 'copy'; codec: string }
  | {
      mode: 'transcode';
      to: 'avc' | 'vp9' | 'vp8';
      step: LadderStep;
      width: number;
      height: number;
      codec: string;
    };

export type AudioPlan =
  | { mode: 'none' }
  | { mode: 'copy'; codec: string }
  | {
      mode: 'transcode';
      to: 'aac' | 'opus';
      codec: string;
      bitrate: number;
      sampleRate: number;
      channels: number;
    };

/** The decision table's answer. */
export type ConvertPath =
  /** Plays as is: nothing to do. */
  | { path: 'native'; reason: string }
  /** Packets are valid for the target browser: re-wrap only (ffmpeg.wasm `-c copy`, BYOS-CNV-001). */
  | { path: 'remux'; reason: string; container: OutputContainer }
  /** Decode and re-encode in this tab with WebCodecs. */
  | {
      path: 'webcodecs';
      reason: string;
      container: OutputContainer;
      video: VideoPlan | null;
      audio: AudioPlan;
    }
  /** ffmpeg.wasm transcoding in this tab (last resort) or the optional worker (BYOS-CNV-003). */
  | { path: 'worker'; reason: string; estimateMinutes: number };

/** One demuxed, still-encoded packet, in decode order. Times are microseconds. */
export interface Packet {
  kind: 'video' | 'audio';
  timestampUs: number;
  durationUs: number;
  key: boolean;
  data: Uint8Array;
}

/** Demuxer output: config for the decoders plus the packets. */
export interface DemuxedSource {
  video?: { config: VideoDecoderConfig; width: number; height: number };
  audio?: { config: AudioDecoderConfig; sampleRate: number; channels: number };
  packets: AsyncIterable<Packet>;
  durationSec: number;
}

export interface TranscodeProgress {
  /** 0..1, by presentation time over duration. */
  fraction: number;
  videoFrames: number;
  audioFrames: number;
}

export interface TranscodeOptions {
  source: DemuxedSource;
  plan: Extract<ConvertPath, { path: 'webcodecs' }>;
  signal?: AbortSignal;
  onProgress?: (p: TranscodeProgress) => void;
  /**
   * Where the muxed bytes go. Default `memory` returns a Blob; `stream` calls
   * `onData(bytes, position)` (positions can rewind for the MP4 header) so a
   * caller can write straight into a FileSystemWritableFileStream.
   */
  output?:
    { kind: 'memory' } | { kind: 'stream'; onData: (data: Uint8Array, position: number) => void };
  /** Test seam: replaces the browser's WebCodecs globals and the muxers. */
  api?: WebCodecsApi;
}

export interface TranscodeResult {
  container: OutputContainer;
  mime: string;
  /** Null when the output was streamed. */
  blob: Blob | null;
  videoFrames: number;
  audioFrames: number;
}

/** The pieces of the platform the pipeline touches, injectable for tests. */
export interface WebCodecsApi {
  VideoDecoder: typeof VideoDecoder;
  VideoEncoder: typeof VideoEncoder;
  AudioDecoder: typeof AudioDecoder;
  AudioEncoder: typeof AudioEncoder;
  EncodedVideoChunk: typeof EncodedVideoChunk;
  EncodedAudioChunk: typeof EncodedAudioChunk;
  createMuxer(opts: MuxerSetup): MuxerHandle;
  /** Draws a frame at a new size; returns a frame the caller owns. */
  scaleFrame(frame: VideoFrame, width: number, height: number): VideoFrame;
}

export interface MuxerSetup {
  container: OutputContainer;
  video?: {
    codec: 'avc' | 'hevc' | 'vp9' | 'av1' | 'vp8';
    width: number;
    height: number;
    frameRate?: number;
  };
  audio?: { codec: 'aac' | 'opus'; sampleRate: number; channels: number };
  output: NonNullable<TranscodeOptions['output']>;
}

export interface MuxerHandle {
  addVideo(chunk: EncodedVideoChunk, meta?: EncodedVideoChunkMetadata): void;
  addAudio(chunk: EncodedAudioChunk, meta?: EncodedAudioChunkMetadata): void;
  finalize(): Blob | null;
}

/** Raised for every failure the UI should show as "could not convert in this tab". */
export class WebCodecsError extends Error {
  constructor(
    message: string,
    readonly code: 'unsupported' | 'decode' | 'encode' | 'mux' | 'aborted',
  ) {
    super(message);
    this.name = 'WebCodecsError';
  }
}
