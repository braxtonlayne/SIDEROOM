// The Converter: probe, remux and subtitle extraction over an `FfmpegEngine` (BYOS-CNV-001). The engine is
// injected, so tests run it with a fake worker and the app with the lazy ffmpeg.wasm engine (`./ffmpeg/engine`).
import { extensionOf, MIME, planRemux, planSubtitles } from './plan';
import { parseProbeLog, progressSeconds } from './probe';
import {
  ConvertError,
  type ConvertOptions,
  type ConvertProgress,
  type ConvertedSubtitle,
  type Converter,
  type FfmpegEngine,
  type ProbeResult,
} from './types';

export const DEFAULT_MAX_MIB = 4096;
const MIB = 1024 * 1024;

/** The last few log lines, for an error message a person can report. */
const tail = (lines: string[]) => lines.slice(-4).join(' | ').slice(0, 400);

/** Turns ffmpeg's `time=` into the progress record, with a speed-based estimate. */
class Progress {
  constructor(
    private o: ConvertOptions | undefined,
    private t0: number,
  ) {}
  emit(phase: ConvertProgress['phase'], fraction: number | null, mediaSec: number) {
    this.o?.onProgress?.({
      phase,
      fraction,
      mediaSec,
      elapsedSec: (Date.now() - this.t0) / 1000,
      etaSec: null,
    });
  }
  time(phase: ConvertProgress['phase'], mediaSec: number, durationSec: number | null) {
    const elapsed = (Date.now() - this.t0) / 1000;
    const fraction = durationSec && durationSec > 0 ? Math.min(1, mediaSec / durationSec) : null;
    const etaSec =
      fraction !== null && fraction > 0.02 && elapsed > 0.5
        ? (elapsed * (1 - fraction)) / fraction
        : null;
    this.o?.onProgress?.({ phase, fraction, mediaSec, elapsedSec: elapsed, etaSec });
  }
}

export function createConverter(makeEngine: () => FfmpegEngine): Converter {
  let engine: FfmpegEngine | null = null;
  // One job at a time: the core is one single-threaded worker.
  let chain: Promise<unknown> = Promise.resolve();

  const run = <T>(job: () => Promise<T>): Promise<T> => {
    const next = chain.then(job, job);
    chain = next.catch(() => undefined);
    return next;
  };

  const guard = (file: File, o: ConvertOptions | undefined) => {
    const limit = o?.maxMiB ?? DEFAULT_MAX_MIB;
    if (file.size > limit * MIB)
      throw new ConvertError('TOO_LARGE', `The file is over ${limit} MiB.`, limit);
    if (o?.signal?.aborted) throw new ConvertError('CANCELLED', 'Cancelled.');
  };

  /** Loads the core, mounts the input, runs `body`, always unmounts; an abort kills the worker. */
  async function session<T>(
    file: File,
    o: ConvertOptions | undefined,
    body: (e: FfmpegEngine, path: string, progress: Progress) => Promise<T>,
  ): Promise<T> {
    guard(file, o);
    const progress = new Progress(o, Date.now());
    progress.emit('loading', null, 0);
    const e = (engine ??= makeEngine());
    const onAbort = () => {
      e.terminate();
      if (engine === e) engine = null;
    };
    o?.signal?.addEventListener('abort', onAbort, { once: true });
    // A killed worker may never answer its pending call: the abort settles the job itself.
    const aborted = new Promise<never>((_, reject) =>
      o?.signal?.addEventListener(
        'abort',
        () => reject(new ConvertError('CANCELLED', 'Cancelled.')),
        { once: true },
      ),
    );
    aborted.catch(() => undefined);
    const work = async (): Promise<T> => {
      await e.load({ signal: o?.signal });
      const ext = extensionOf(file.name) || 'bin';
      const path = await e.mountInput(file, `input.${ext}`);
      try {
        return await body(e, path, progress);
      } finally {
        if (!o?.signal?.aborted) await e.unmountInput().catch(() => undefined);
      }
    };
    try {
      return await Promise.race([work(), aborted]);
    } catch (err) {
      if (o?.signal?.aborted) throw new ConvertError('CANCELLED', 'Cancelled.');
      if (err instanceof ConvertError) throw err;
      throw new ConvertError('ENGINE', err instanceof Error ? err.message : String(err));
    } finally {
      o?.signal?.removeEventListener('abort', onAbort);
    }
  }

  async function probeWith(
    e: FfmpegEngine,
    path: string,
    o?: ConvertOptions,
  ): Promise<ProbeResult> {
    const lines: string[] = [];
    // No output file: ffmpeg prints the input and stops with an error code. The log is what we want.
    await e.exec(['-hide_banner', '-i', path], { signal: o?.signal, onLog: (l) => lines.push(l) });
    const probe = parseProbeLog(lines);
    if (probe.streams.length === 0)
      throw new ConvertError('UNSUPPORTED', `ffmpeg could not read the file. ${tail(lines)}`);
    return probe;
  }

  /** Runs an ffmpeg job, reporting progress from the log. */
  async function exec(
    e: FfmpegEngine,
    args: string[],
    phase: 'remuxing' | 'extracting',
    probe: ProbeResult,
    progress: Progress,
    o?: ConvertOptions,
  ): Promise<void> {
    const lines: string[] = [];
    progress.emit(phase, probe.durationSec ? 0 : null, 0);
    const code = await e.exec(args, {
      signal: o?.signal,
      onLog: (l) => {
        lines.push(l);
        const t = progressSeconds(l);
        if (t !== null) progress.time(phase, t, probe.durationSec);
      },
    });
    if (code !== 0)
      throw new ConvertError('ENGINE', `ffmpeg stopped with code ${code}. ${tail(lines)}`);
  }

  return {
    probe: (file, o) => run(() => session(file, o, (e, path) => probeWith(e, path, o))),

    remux: (file, o) =>
      run(() =>
        session(file, o, async (e, path, progress) => {
          progress.emit('probing', null, 0);
          const probe = o?.probe ?? (await probeWith(e, path, o));
          const plan = planRemux(probe);
          if (!plan.container)
            throw new ConvertError(
              'UNSUPPORTED',
              `Remux cannot make this playable (${plan.reason}).`,
            );
          const out = `output.${plan.container}`;
          const args = ['-hide_banner', '-y', '-i', path];
          for (const i of plan.map) args.push('-map', `0:${i}`);
          // No subtitles in the video file (they come out as sidecars), no data streams.
          args.push('-c', 'copy', '-sn', '-dn', '-avoid_negative_ts', 'make_zero');
          if (plan.container === 'mp4') {
            if (plan.videoCodec === 'hevc') args.push('-tag:v', 'hvc1');
            args.push(
              '-movflags',
              o?.fragmented
                ? '+frag_keyframe+empty_moov+default_base_moof+global_sidx'
                : '+faststart',
            );
          }
          args.push(out);
          await exec(e, args, 'remuxing', probe, progress, o);
          const data = await e.readFile(out).catch(() => null);
          await e.deleteFile(out).catch(() => undefined);
          if (!data || data.length === 0)
            throw new ConvertError('NO_OUTPUT', 'ffmpeg wrote no output.');
          const stem = file.name.replace(/\.[^.]*$/, '') || 'video';
          const name = `${stem}.${plan.container}`;
          const mime = MIME[plan.container] ?? 'application/octet-stream';
          return {
            output: { file: new File([data as BlobPart], name, { type: mime }), name, mime },
            plan,
            probe,
          };
        }),
      ),

    extractSubtitles: (file, o) =>
      run(() =>
        session(file, o, async (e, path, progress) => {
          progress.emit('probing', null, 0);
          const probe = o?.probe ?? (await probeWith(e, path, o));
          const { items, skipped } = planSubtitles(probe, file.name);
          if (items.length === 0) return { subtitles: [], skipped, probe };
          const args = ['-hide_banner', '-y', '-i', path];
          for (const it of items)
            args.push(
              '-map',
              `0:${it.stream.index}`,
              '-c:s',
              it.format === 'ass' ? 'ass' : 'srt',
              it.name,
            );
          await exec(e, args, 'extracting', probe, progress, o);
          const subtitles: ConvertedSubtitle[] = [];
          for (const it of items) {
            const data = await e.readFile(it.name).catch(() => null);
            await e.deleteFile(it.name).catch(() => undefined);
            if (!data || data.length === 0) continue;
            const mime = MIME[it.format] ?? 'text/plain';
            subtitles.push({
              file: new File([data as BlobPart], it.name, { type: mime }),
              name: it.name,
              mime,
              format: it.format,
              streamIndex: it.stream.index,
              ...(it.stream.language ? { language: it.stream.language } : {}),
              ...(it.stream.title ? { title: it.stream.title } : {}),
            });
          }
          return { subtitles, skipped, probe };
        }),
      ),

    withInput: (file, o, fn) => run(() => session(file, o, (e, path) => fn(e, path))),

    dispose() {
      engine?.terminate();
      engine = null;
    },
  };
}
