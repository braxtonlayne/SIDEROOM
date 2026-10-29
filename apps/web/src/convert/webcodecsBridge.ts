// The bridge to conv-webcodecs (BYOS-CNV-002), whose files (`./webcodecs/*`, `./decide.ts`) arrive at integration.
// `import.meta.glob` finds the module when it exists and yields nothing when it does not, so this branch builds and
// tests alone, and the integrated tree gets the WebCodecs path with no edit. The shapes below are the commander's
// message of wave 29; a mismatch is a type error at integration (the `Wc` interface is the one place to fix).
import {
  ConvertError,
  type ConvertedFile,
  type ConvertOptions,
  type Converter,
  type ProbeResult,
} from './types';

interface SourceInfo {
  container: string;
  video?: { codec: string; width: number; height: number; frameRate: number };
  audio?: { codec: string; sampleRate: number; channels: number };
  durationSec: number;
  sizeBytes: number;
}
export interface WcPlan {
  path: 'native' | 'remux' | 'webcodecs' | 'worker';
  container?: string;
  estimateMinutes?: number;
}
interface Wc {
  decide(info: SourceInfo, caps: unknown): Promise<WcPlan>;
  detectCapabilities(): unknown;
  demuxFmp4(chunks: unknown, durationSec: number): Promise<unknown>;
  chunked(bytes: Uint8Array): unknown;
  transcodeWithWebCodecs(o: {
    source: unknown;
    plan: WcPlan;
    signal?: AbortSignal;
    onProgress?: (p: number | { fraction?: number; progress?: number }) => void;
  }): Promise<{ blob: Blob; mime: string; container: string }>;
  FRAGMENT_ARGS: string[];
  WebCodecsError: new (...a: never[]) => Error;
}

const found = import.meta.glob('./webcodecs/index.ts');
let cached: Promise<Wc | null> | null = null;
export const loadWebCodecs = (): Promise<Wc | null> => {
  const load = Object.values(found)[0];
  cached ??= load ? (load() as Promise<Wc>).catch(() => null) : Promise.resolve(null);
  return cached;
};

/** ffmpeg codec names to the WebCodecs codec strings `decide` expects (a representative string per codec). */
const CODEC: Record<string, string> = {
  h264: 'avc1.640028',
  hevc: 'hev1.1.6.L120.B0',
  vp8: 'vp8',
  vp9: 'vp09.00.10.08',
  av1: 'av01.0.08M.08',
  aac: 'mp4a.40.2',
  mp3: 'mp3',
  opus: 'opus',
  vorbis: 'vorbis',
  flac: 'flac',
  ac3: 'ac-3',
  eac3: 'ec-3',
};

export function sourceInfo(probe: ProbeResult, file: File): SourceInfo {
  const v = probe.streams.find((s) => s.kind === 'video');
  const a = probe.streams.find((s) => s.kind === 'audio');
  return {
    container: probe.container,
    ...(v
      ? {
          video: {
            codec: CODEC[v.codec] ?? v.codec,
            width: v.width ?? 0,
            height: v.height ?? 0,
            frameRate: v.fps ?? 0,
          },
        }
      : {}),
    ...(a
      ? {
          audio: {
            codec: CODEC[a.codec] ?? a.codec,
            sampleRate: a.sampleRate ?? 0,
            channels: a.channels ?? 2,
          },
        }
      : {}),
    durationSec: probe.durationSec ?? 0,
    sizeBytes: file.size,
  };
}

/** Asks conv-webcodecs where the source should go; `null` when its module is not part of this build. */
export async function planWithWebCodecs(probe: ProbeResult, file: File): Promise<WcPlan | null> {
  const wc = await loadWebCodecs();
  if (!wc) return null;
  try {
    return await wc.decide(sourceInfo(probe, file), wc.detectCapabilities());
  } catch {
    return null;
  }
}

/** Demuxes with ffmpeg.wasm (`-c copy` into fragmented MP4) and transcodes with WebCodecs. */
export async function transcodeInTab(
  conv: Converter,
  file: File,
  probe: ProbeResult,
  plan: WcPlan,
  o: ConvertOptions,
): Promise<ConvertedFile> {
  const wc = await loadWebCodecs();
  if (!wc) throw new ConvertError('UNSUPPORTED', 'WebCodecs is not available.');
  try {
    return await conv.withInput(file, o, async (engine, inputPath) => {
      o.onProgress?.({
        phase: 'probing',
        fraction: null,
        mediaSec: 0,
        elapsedSec: 0,
        etaSec: null,
      });
      const code = await engine.exec(
        ['-i', inputPath, '-map', '0:v:0?', '-map', '0:a:0?', ...wc.FRAGMENT_ARGS, 'frag.mp4'],
        { signal: o.signal },
      );
      if (code !== 0) throw new ConvertError('ENGINE', `ffmpeg stopped with code ${code}.`);
      const bytes = await engine.readFile('frag.mp4');
      await engine.deleteFile('frag.mp4').catch(() => undefined);
      const source = await wc.demuxFmp4(wc.chunked(bytes), probe.durationSec ?? 0);
      const t0 = Date.now();
      const out = await wc.transcodeWithWebCodecs({
        source,
        plan,
        signal: o.signal,
        onProgress: (p) => {
          const f = typeof p === 'number' ? p : (p.fraction ?? p.progress ?? null);
          const elapsed = (Date.now() - t0) / 1000;
          o.onProgress?.({
            phase: 'transcoding',
            fraction: f,
            mediaSec: f === null ? 0 : f * (probe.durationSec ?? 0),
            elapsedSec: elapsed,
            etaSec: f && f > 0.02 ? (elapsed * (1 - f)) / f : null,
          });
        },
      });
      const ext = out.container === 'webm' ? 'webm' : 'mp4';
      const name = `${file.name.replace(/\.[^.]*$/, '') || 'video'}.${ext}`;
      return { file: new File([out.blob], name, { type: out.mime }), name, mime: out.mime };
    });
  } catch (e) {
    if (e instanceof ConvertError) throw e;
    // A WebCodecsError (or anything the transcoder throws) falls through to the worker hint, never raw.
    throw new ConvertError('UNSUPPORTED', 'Remux cannot make this playable (transcode).');
  }
}
