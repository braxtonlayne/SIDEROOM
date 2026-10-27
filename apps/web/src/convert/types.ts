// Shared types of the client conversion pipeline (BYOS-CNV-001, BYOS-CNV-002).
//
// OWNER: conv-remux (wave 29). conv-webcodecs imports these and never edits this file; if it needs another
// field it asks for it in its handoff. `decide.ts` and `webcodecs/*` are conv-webcodecs's; they plug in through
// `setDecider` and `registerTranscoder` in `./index.ts` and the types below.

/** What one stream of the source is. */
export interface StreamInfo {
  /** ffmpeg's absolute stream index (`0:N`). */
  index: number;
  kind: 'video' | 'audio' | 'subtitle' | 'other';
  /** ffmpeg's codec name: h264, hevc, vp9, av1, aac, ac3, eac3, dts, opus, mp3, flac, subrip, ass, mov_text ... */
  codec: string;
  /** "High", "LC", "Main 10" ... when the log names one. */
  profile?: string;
  width?: number;
  height?: number;
  fps?: number;
  channels?: number;
  sampleRate?: number;
  /** ISO 639-2 code from the container (`eng`), when present. */
  language?: string;
  title?: string;
  /** Subtitle only: text that can become `.srt` / `.ass` (false for bitmap formats such as PGS and DVD). */
  textual?: boolean;
  /** Overall bitrate of this stream in kb/s, when the log names one. */
  bitrateKbps?: number;
}

export interface ProbeResult {
  /** ffmpeg's demuxer name(s): `matroska,webm`, `mov,mp4,m4a,3gp,3g2,mj2`, `avi`, `mpegts` ... */
  container: string;
  durationSec: number | null;
  bitrateKbps: number | null;
  streams: StreamInfo[];
}

export type ConvertPhase =
  'loading' | 'probing' | 'remuxing' | 'extracting' | 'transcoding' | 'uploading';

export interface ConvertProgress {
  phase: ConvertPhase;
  /** 0 to 1, or `null` when the length of the source is unknown (an indeterminate bar). */
  fraction: number | null;
  /** Seconds of media done so far (ffmpeg's `time=`). */
  mediaSec: number;
  elapsedSec: number;
  /** Seconds left at the current speed; `null` until there is a speed to extrapolate. */
  etaSec: number | null;
}

export interface ConvertOptions {
  signal?: AbortSignal;
  onProgress?: (p: ConvertProgress) => void;
  /** Refuse sources over this many MiB (`byos.client_convert_max_mib`, default 4096). */
  maxMiB?: number;
}

/** A file the converter made. `name` is a bare file name, never a path. */
export interface ConvertedFile {
  file: File;
  name: string;
  mime: string;
}

export type SubtitleFormat = 'srt' | 'ass';

export interface ConvertedSubtitle extends ConvertedFile {
  format: SubtitleFormat;
  language?: string;
  title?: string;
  /** The source stream (`StreamInfo.index`). */
  streamIndex: number;
}

export interface RemuxPlan {
  /** The container to write; `null` when remux cannot make this source playable (see `reason`). */
  container: 'mp4' | 'webm' | null;
  /** Absolute stream indexes to copy (video first). */
  map: number[];
  /** Streams left out, with the reason. */
  dropped: {
    index: number;
    reason: 'incompatible-audio' | 'extra-video' | 'bitmap-subtitle' | 'other';
  }[];
  /** Video codec of the output, for the decision table (`hevc` needs a browser with HEVC). */
  videoCodec: string | null;
  /** Audio codec(s) of the output. */
  audioCodecs: string[];
  /** Why `container` is null. */
  reason?: 'no-video' | 'video-codec' | 'audio-codec';
}

export interface RemuxResult {
  output: ConvertedFile;
  plan: RemuxPlan;
  probe: ProbeResult;
}

export interface ExtractResult {
  subtitles: ConvertedSubtitle[];
  /** Streams that were not text (PGS, DVD): listed, never silently lost. */
  skipped: StreamInfo[];
  probe: ProbeResult;
}

export type ConvertErrorCode =
  'TOO_LARGE' | 'UNSUPPORTED' | 'CANCELLED' | 'ENGINE' | 'NO_OUTPUT' | 'NO_ENGINE';

export class ConvertError extends Error {
  constructor(
    readonly code: ConvertErrorCode,
    message: string,
    /** For `TOO_LARGE`: the limit that applied, in MiB. */
    readonly limitMiB?: number,
  ) {
    super(message);
    this.name = 'ConvertError';
  }
}

/** The one object the UI talks to. */
export interface Converter {
  probe(file: File, o?: ConvertOptions): Promise<ProbeResult>;
  /** `-c copy` into MP4 or WebM. Throws `UNSUPPORTED` when the plan has no container (hand over to a transcoder). */
  remux(
    file: File,
    o?: ConvertOptions & { probe?: ProbeResult; fragmented?: boolean },
  ): Promise<RemuxResult>;
  extractSubtitles(
    file: File,
    o?: ConvertOptions & { probe?: ProbeResult },
  ): Promise<ExtractResult>;
  /**
   * Runs `fn` with the loaded engine and the source mounted (what a Transcoder needs). One job at a time, the
   * same size guard and cancel as the other methods.
   */
  withInput<T>(
    file: File,
    o: ConvertOptions | undefined,
    fn: (engine: FfmpegEngine, inputPath: string) => Promise<T>,
  ): Promise<T>;
  /** Stops the engine and frees its memory. The converter can be used again (it reloads). */
  dispose(): void;
}

// ---- The seam between conv-remux and conv-webcodecs --------------------------------------------------------

/** What the browser can do, filled in by conv-webcodecs's `detectCapabilities()` (or a fake in tests). */
export interface ConvertCapabilities {
  /** `HTMLMediaElement.canPlayType` answers, keyed by the MIME type with codecs that was asked. */
  canPlay?: Record<string, boolean>;
  videoDecode?: Record<string, boolean>;
  videoEncodeH264?: boolean;
  audioEncodeAac?: boolean;
  audioEncodeOpus?: boolean;
  webCodecs: boolean;
}

/** Where a source should go. `native` = upload as is. */
export type ConvertRoute =
  | 'native'
  | 'remux'
  | 'webcodecs'
  /** ffmpeg.wasm transcoding in the tab: the last resort, with an honest estimate. */
  | 'ffmpeg-transcode'
  /** Too heavy for the tab: the optional worker (BYOS-CNV-003). */
  | 'worker';

export interface Decision {
  route: ConvertRoute;
  /** A stable reason code for the UI and the tests (`hevc-unsupported`, `ac3-audio` ...). */
  reason: string;
  /** For `ffmpeg-transcode`: the estimate shown ("about N minutes in this tab"). */
  estimateMin?: number;
}

export type Decider = (
  probe: ProbeResult,
  plan: RemuxPlan,
  caps: ConvertCapabilities,
  file: File,
) => Decision;

/**
 * Registered by conv-webcodecs. Gets the source and the ffmpeg engine (to demux with `-c copy -f mp4`) and
 * returns the playable file. It must honour `o.signal` and report `o.onProgress` with phase `transcoding`.
 */
export interface TranscodeContext {
  file: File;
  probe: ProbeResult;
  options: ConvertOptions;
  engine: FfmpegEngine;
  /** Where the source is readable inside the engine (`/in/input.mkv`). */
  inputPath: string;
}

export interface Transcoder {
  id: string;
  run(ctx: TranscodeContext): Promise<ConvertedFile>;
}

// ---- The ffmpeg engine (what the Converter needs from ffmpeg.wasm) ------------------------------------------

export interface FfmpegEngine {
  /** Loads the core (the lazy chunk and the `.wasm`). Idempotent. */
  load(o?: { signal?: AbortSignal }): Promise<void>;
  /** Makes `file` readable as `/in/<name>` without copying it into the wasm heap where the engine can. */
  mountInput(file: File, name: string): Promise<string>;
  unmountInput(): Promise<void>;
  /** Runs `ffmpeg args`. `onLog` gets every stderr line. Resolves with the exit code. */
  exec(
    args: string[],
    o?: { signal?: AbortSignal; onLog?: (line: string) => void },
  ): Promise<number>;
  readFile(path: string): Promise<Uint8Array>;
  deleteFile(path: string): Promise<void>;
  /** Kills the worker (cancel, or to free memory). */
  terminate(): void;
}
