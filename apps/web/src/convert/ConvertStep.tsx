// The shared "Convert" step of the upload dialogs (BYOS-CNV-001): the Cinema submit picker and the bucket browser
// use it (Library takes books and comics, Music arrives in wave 30). It gets the files that do not play in a
// browser as they are, converts them one at a time in this tab (remux through ffmpeg.wasm, or the WebCodecs
// transcoder when conv-webcodecs has registered one and the decision table picks it), shows progress with a
// cancel, and hands the results to the dialog, which uploads them by its own path.
import { Button, Checkbox, Notice } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { useT } from '../i18n';
import { ConvertProgressBar } from './ConvertProgress';
import { decide, getConverter, getTranscoder } from './index';
import {
  ConvertError,
  type ConvertedFile,
  type ConvertedSubtitle,
  type ConvertProgress,
  type Converter,
  type Decision,
  type StreamInfo,
} from './types';
import './convert.css';

export interface ConvertItem {
  original: File;
  /** The playable file (MP4 or WebM). */
  output: ConvertedFile;
  subtitles: ConvertedSubtitle[];
  /** Which route made it (`remux`, `webcodecs`). */
  route: Decision['route'];
}

type Row =
  | { state: 'waiting' }
  | { state: 'working'; progress: ConvertProgress }
  | { state: 'done'; item: ConvertItem; skippedSubs: StreamInfo[] }
  | { state: 'native' }
  | { state: 'failed'; message: string };

const REASON_KEY: Record<string, string> = {
  'no-video': 'convert.reason.noVideo',
  'video-codec': 'convert.reason.videoCodec',
  'audio-codec': 'convert.reason.audioCodec',
};

export function ConvertStep({
  files,
  required = false,
  maxMiB,
  converter,
  onDone,
  onAsIs,
  onCancel,
}: {
  /** Files that need conversion (`needsConversion(name)`). */
  files: File[];
  /** The server refuses files that were not converted (`byos.require_client_remux`): no "upload as they are". */
  required?: boolean;
  /** `byos.client_convert_max_mib`; the converter's default (4096) when unset. */
  maxMiB?: number;
  converter?: Converter;
  /** Every file converted (the dialog uploads `items`; `native` files are the ones that needed nothing). */
  onDone: (items: ConvertItem[], native: File[]) => void;
  /** The person chose to upload the files unchanged. */
  onAsIs: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [rows, setRows] = useState<Row[]>(() => files.map(() => ({ state: 'waiting' })));
  const [subs, setSubs] = useState(true);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState<{ items: ConvertItem[]; native: File[] } | null>(null);
  const abort = useRef<AbortController | null>(null);

  const setRow = (i: number, r: Row) => setRows((cur) => cur.map((x, j) => (j === i ? r : x)));

  const failure = (e: unknown): string => {
    if (e instanceof ConvertError) {
      switch (e.code) {
        case 'TOO_LARGE':
          return t('convert.error.tooLarge', { limit: e.limitMiB ?? 0 });
        case 'UNSUPPORTED':
          return e.message.startsWith('Remux cannot')
            ? t('convert.error.needsTranscode', {
                reason: t(
                  REASON_KEY[/\((.*)\)/.exec(e.message)?.[1] ?? ''] ?? 'convert.reason.videoCodec',
                ),
              })
            : t('convert.error.unsupported');
        case 'CANCELLED':
          return t('convert.error.cancelled');
        case 'NO_OUTPUT':
          return t('convert.error.noOutput');
        case 'NO_ENGINE':
          return t('convert.error.noEngine');
        default:
          return t('convert.error.engine', { detail: e.message });
      }
    }
    return t('convert.error.engine', { detail: e instanceof Error ? e.message : String(e) });
  };

  const run = async () => {
    const conv = converter ?? getConverter();
    const ctl = new AbortController();
    abort.current = ctl;
    setRunning(true);
    const items: ConvertItem[] = [];
    const native: File[] = [];
    let failed = false;
    for (let i = 0; i < files.length && !ctl.signal.aborted; i++) {
      const file = files[i]!;
      const opts = {
        signal: ctl.signal,
        ...(maxMiB ? { maxMiB } : {}),
        onProgress: (progress: ConvertProgress) => setRow(i, { state: 'working', progress }),
      };
      setRow(i, {
        state: 'working',
        progress: { phase: 'loading', fraction: null, mediaSec: 0, elapsedSec: 0, etaSec: null },
      });
      try {
        const probe = await conv.probe(file, opts);
        const decision = decide(probe, file);
        if (decision.route === 'native') {
          native.push(file);
          setRow(i, { state: 'native' });
          continue;
        }
        let output: ConvertedFile;
        let skippedSubs: StreamInfo[] = [];
        let extracted: ConvertedSubtitle[] = [];
        if (decision.route === 'remux') {
          const r = await conv.remux(file, { ...opts, probe });
          output = r.output;
        } else if (decision.route === 'webcodecs' && getTranscoder()) {
          const tc = getTranscoder()!;
          output = await conv.withInput(file, opts, (engine, inputPath) =>
            tc.run({ file, probe, options: opts, engine, inputPath }),
          );
        } else {
          // `ffmpeg-transcode` and `worker`: not something a remux can do; say so with the reason.
          throw new ConvertError(
            'UNSUPPORTED',
            `Remux cannot make this playable (${decision.reason}).`,
          );
        }
        if (subs && probe.streams.some((s) => s.kind === 'subtitle')) {
          const e = await conv.extractSubtitles(file, { ...opts, probe });
          extracted = e.subtitles;
          skippedSubs = e.skipped;
        }
        const item: ConvertItem = {
          original: file,
          output,
          subtitles: extracted,
          route: decision.route,
        };
        items.push(item);
        setRow(i, { state: 'done', item, skippedSubs });
      } catch (e) {
        failed = true;
        setRow(i, { state: 'failed', message: failure(e) });
        if (e instanceof ConvertError && e.code === 'CANCELLED') break;
      }
    }
    abort.current = null;
    setRunning(false);
    if (!ctl.signal.aborted && !failed) setFinished({ items, native });
  };

  const stop = () => abort.current?.abort();
  const stateText = (r: Row) =>
    t(
      r.state === 'waiting'
        ? 'convert.step.state.waiting'
        : r.state === 'working'
          ? 'convert.step.state.working'
          : r.state === 'done'
            ? 'convert.step.state.done'
            : r.state === 'native'
              ? 'convert.step.state.skipped'
              : 'convert.step.state.failed',
    );

  return (
    <section className="cnv-step" aria-label={t('convert.step.title')}>
      <h3 className="cnv-title">{t('convert.step.title')}</h3>
      <p className="us-muted">{t('convert.step.intro', { count: files.length })}</p>
      {required && <Notice variant="info">{t('convert.step.required')}</Notice>}
      <ul className="cnv-list" aria-label={t('convert.step.list')}>
        {files.map((f, i) => {
          const r = rows[i]!;
          return (
            <li key={`${f.name}-${i}`}>
              <strong>{f.name}</strong>
              {r.state === 'working' ? (
                <ConvertProgressBar name={f.name} progress={r.progress} />
              ) : (
                <span>{stateText(r)}</span>
              )}
              {r.state === 'failed' && <span role="alert">{r.message}</span>}
              {r.state === 'done' && r.item.subtitles.length > 0 && (
                <span className="us-muted">
                  {t('convert.step.subtitles', { count: r.item.subtitles.length })}
                </span>
              )}
              {r.state === 'done' && r.skippedSubs.length > 0 && (
                <span className="us-muted">
                  {t('convert.step.bitmapSubtitles', { count: r.skippedSubs.length })}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {finished ? (
        <>
          <p role="status">{t('convert.step.done', { count: finished.items.length })}</p>
          <div className="us-actions">
            <Button onClick={() => onDone(finished.items, finished.native)}>
              {t('convert.step.continue')}
            </Button>
          </div>
        </>
      ) : (
        <>
          <Checkbox
            checked={subs}
            disabled={running}
            onChange={(e) => setSubs(e.target.checked)}
            label={t('convert.step.extractSubtitles')}
          />
          <div className="us-actions">
            {running ? (
              <Button variant="secondary" onClick={stop}>
                {t('convert.step.stop')}
              </Button>
            ) : (
              <Button onClick={() => void run()}>{t('convert.step.start')}</Button>
            )}
            {!required && (
              <Button variant="ghost" disabled={running} onClick={onAsIs}>
                {t('convert.step.asIs')}
              </Button>
            )}
            <Button variant="ghost" disabled={running} onClick={onCancel}>
              {t('convert.step.cancel')}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
