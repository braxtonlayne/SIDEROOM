// The bridge to the WebCodecs path (BYOS-CNV-002, `./webcodecs/*` and `./decide.ts`). The module is imported
// lazily (its own chunk, with the muxers behind it), so a page that never converts never loads it. The types are
// the module's own: a signature change there is a type error here.
import type { Capabilities, ConvertPath, SourceContainer, SourceInfo } from './webcodecs/types';
import {
  ConvertError,
  type ConvertedFile,
  type ConvertOptions,
  type Converter,
  type ProbeResult,
} from './types';

type Wc = typeof import('./webcodecs');

let cached: Promise<Wc | null> | null = null;
export const loadWebCodecs = (): Promise<Wc | null> => {
  cached ??= import('./webcodecs').catch(() => null);
  return cached;
};

/** ffmpeg codec names to the WebCodecs codec strings `decide` expects (a representative string per codec). */
const CODEC: Record<string, string> = {
  h264: 'avc1.640028',
  hevc: 'hvc1.1.6.L120.90',
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

/** ffmpeg's demuxer name (`matroska,webm`, `mov,mp4,...`) to the decision table's container. */
export function sourceContainer(demuxer: string, fileName: string): SourceContainer {
  const d = demuxer.toLowerCase();
  const ext = /\.([^.]+)$/.exec(fileName)?.[1]?.toLowerCase() ?? '';
  if (d.startsWith('mov') || d.includes('mp4')) return 'mp4';
  // ffmpeg names Matroska and WebM with one demuxer; the extension tells them apart.
  if (d.startsWith('matroska')) return ext === 'webm' ? 'webm' : 'mkv';
  if (d === 'avi') return 'avi';
  if (d === 'mpegts') return 'ts';
  if (d === 'ogg') return 'ogg';
  return 'unknown';
}

export function sourceInfo(probe: ProbeResult, file: File): SourceInfo {
  const v = probe.streams.find((s) => s.kind === 'video');
  const a = probe.streams.find((s) => s.kind === 'audio');
  return {
    container: sourceContainer(probe.container, file.name),
    ...(v
      ? {
          video: {
            codec: CODEC[v.codec] ?? v.codec,
            width: v.width ?? 0,
            height: v.height ?? 0,
            ...(v.fps ? { frameRate: v.fps } : {}),
          },
        }
      : {}),
    ...(a
      ? {
          audio: {
            codec: CODEC[a.codec] ?? a.codec,
            sampleRate: a.sampleRate ?? 48_000,
            channels: a.channels ?? 2,
          },
        }
      : {}),
    durationSec: probe.durationSec ?? 0,
    sizeBytes: file.size,
  };
}

/** Asks the decision table where the source should go; `null` when the module cannot load or the table throws. */
export async function planWithWebCodecs(
  probe: ProbeResult,
  file: File,
  caps?: Capabilities,
): Promise<ConvertPath | null> {
  const wc = await loadWebCodecs();
  if (!wc) return null;
  try {
    return await wc.decide(sourceInfo(probe, file), caps ?? wc.detectCapabilities());
  } catch {
    return null;
  }
}

/** Demuxes with ffmpeg.wasm (`-c copy` into fragmented MP4) and transcodes with WebCodecs. */
export async function transcodeInTab(
  conv: Converter,
  file: File,
  probe: ProbeResult,
  plan: Extract<ConvertPath, { path: 'webcodecs' }>,
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
        ...(o.signal ? { signal: o.signal } : {}),
        onProgress: ({ fraction }) => {
          const elapsed = (Date.now() - t0) / 1000;
          o.onProgress?.({
            phase: 'transcoding',
            fraction,
            mediaSec: fraction * (probe.durationSec ?? 0),
            elapsedSec: elapsed,
            etaSec: fraction > 0.02 ? (elapsed * (1 - fraction)) / fraction : null,
          });
        },
      });
      if (!out.blob) throw new ConvertError('NO_OUTPUT', 'The transcoder wrote no file.');
      const ext = out.container === 'webm' ? 'webm' : 'mp4';
      const name = `${file.name.replace(/\.[^.]*$/, '') || 'video'}.${ext}`;
      return { file: new File([out.blob], name, { type: out.mime }), name, mime: out.mime };
    });
  } catch (e) {
    if (e instanceof ConvertError) throw e;
    // A WebCodecsError (or anything the transcoder throws) falls through to the worker hint, never raw.
    throw new ConvertError('UNSUPPORTED', 'Remux cannot make this playable (video-codec).');
  }
}
