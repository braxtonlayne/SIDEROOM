/**
 * UTL-IMG-001: Video to GIF. Pick a video file, choose the start, length, width and frame rate, and the frames
 * are read from a `<video>` element and encoded by sr-tools-wasm (the `gif` crate's encoder with our own
 * quantiser). 15 s and 20 MB caps. Nothing is uploaded; the GIF is offered as a download.
 */
import { Button, Icon, ProgressBar } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { announce } from '../../a11y/announcer';
import { useT } from '../../i18n';
import { captureFrames, planFrames, MAX_SECONDS } from './videoFrames';
import { loadStudioWasm } from './wasm';
import './studio.css';

type Phase =
  { kind: 'idle' } | { kind: 'reading'; done: number; total: number } | { kind: 'encoding' };

function fmtBytes(n: number): string {
  return n < 1024 * 1024
    ? `${Math.max(1, Math.round(n / 1024))} KB`
    : `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export default function GifTool() {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [name, setName] = useState('clip');
  const [meta, setMeta] = useState<{ duration: number; width: number; height: number } | null>(
    null,
  );
  const [invalid, setInvalid] = useState(false);
  const [start, setStart] = useState(0);
  const [length, setLength] = useState(5);
  const [width, setWidth] = useState(480);
  const [fps, setFps] = useState(10);
  const [phase, setPhase] = useState<Phase>({ kind: 'idle' });
  const [error, setError] = useState('');
  const [result, setResult] = useState<{ url: string; size: number } | null>(null);
  const video = useRef<HTMLVideoElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const abort = useRef<AbortController | null>(null);

  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url);
    },
    [url],
  );
  useEffect(
    () => () => {
      if (result) URL.revokeObjectURL(result.url);
    },
    [result],
  );
  useEffect(() => () => abort.current?.abort(), []);

  const pick = (file: File | undefined) => {
    if (!file) return;
    setInvalid(false);
    setMeta(null);
    setResult(null);
    setError('');
    setName(file.name.replace(/\.[^.]+$/, '') || 'clip');
    setUrl(URL.createObjectURL(file));
  };

  const make = async () => {
    const v = video.current;
    if (!v || !meta) return;
    const plan = planFrames(meta.duration, start, length, fps);
    const w = Math.max(16, Math.min(Math.round(width), 1024, meta.width));
    const h = Math.max(16, Math.round((w * meta.height) / meta.width));
    const ctl = new AbortController();
    abort.current = ctl;
    setError('');
    setResult(null);
    setPhase({ kind: 'reading', done: 0, total: plan.times.length });
    try {
      const tools = await loadStudioWasm();
      const builder = tools.gif.create(w, h, plan.delayCs);
      try {
        v.pause();
        await captureFrames(
          v,
          plan.times,
          w,
          h,
          (rgba, i) => {
            builder.addFrame(rgba);
            setPhase({ kind: 'reading', done: i + 1, total: plan.times.length });
          },
          ctl.signal,
        );
        setPhase({ kind: 'encoding' });
        await new Promise((r) => setTimeout(r, 0)); // let the "building" message paint before the encoder blocks
        const bytes = builder.finish();
        const blob = new Blob([bytes as BlobPart], { type: 'image/gif' });
        setResult({ url: URL.createObjectURL(blob), size: blob.size });
        announce(t('studio.gif.done', { size: fmtBytes(blob.size) }));
      } finally {
        builder.free();
      }
    } catch (e) {
      const msg = String((e as Error)?.message ?? e);
      if (msg.includes('cancelled')) setError(t('studio.gif.cancelled'));
      else if (msg.includes('GIF_TOO_LONG')) setError(t('studio.gif.errorLong'));
      else if (msg.includes('GIF_TOO_BIG')) setError(t('studio.gif.errorBig'));
      else if (msg.includes('GIF_TOO_MUCH')) setError(t('studio.gif.errorMemory'));
      else setError(t('studio.gif.errorOther'));
    } finally {
      setPhase({ kind: 'idle' });
      abort.current = null;
    }
  };

  const busy = phase.kind !== 'idle';
  const maxLength = meta
    ? Math.max(0.1, Math.min(MAX_SECONDS, meta.duration - start))
    : MAX_SECONDS;

  return (
    <div className="sr-st">
      <p className="sr-tl-muted sr-st-privacy">{t('studio.gif.hint')}</p>
      <input
        ref={input}
        type="file"
        accept="video/*"
        hidden
        data-testid="gif-file"
        aria-label={t('studio.gif.pick')}
        onChange={(e) => {
          pick(e.target.files?.[0]);
          e.target.value = '';
        }}
      />
      <div className="sr-st-row">
        <Button
          size="sm"
          leadingIcon={<Icon name="film" size={16} />}
          onClick={() => input.current?.click()}
          disabled={busy}
        >
          {t('studio.gif.pick')}
        </Button>
        <span className="sr-tl-muted">{t('studio.gif.limits')}</span>
      </div>
      {url && (
        <video
          ref={video}
          src={url}
          className="sr-st-video"
          muted
          playsInline
          controls
          preload="auto"
          onLoadedMetadata={(e) => {
            const v = e.currentTarget;
            setMeta({
              duration: v.duration,
              width: v.videoWidth || 480,
              height: v.videoHeight || 270,
            });
            setStart(0);
            setLength(Math.min(5, v.duration || 5));
            setWidth(Math.min(480, v.videoWidth || 480));
          }}
          onError={() => setInvalid(true)}
        />
      )}
      {invalid && (
        <p role="alert" className="sr-st-error">
          {t('studio.gif.invalid')}
        </p>
      )}
      {meta && (
        <div className="sr-st-panel">
          <label className="sr-st-field">
            <span>{t('studio.gif.start')}</span>
            <input
              type="number"
              min={0}
              max={Math.max(0, meta.duration - 0.1)}
              step={0.1}
              value={start}
              onChange={(e) => setStart(Math.max(0, Number(e.target.value) || 0))}
            />
          </label>
          <label className="sr-st-field">
            <span>{t('studio.gif.length')}</span>
            <input
              type="number"
              min={0.1}
              max={maxLength}
              step={0.1}
              value={Math.min(length, maxLength)}
              onChange={(e) => setLength(Math.max(0.1, Number(e.target.value) || 1))}
            />
          </label>
          <label className="sr-st-field">
            <span>{t('studio.gif.width')}</span>
            <input
              type="number"
              min={16}
              max={Math.min(1024, meta.width)}
              value={width}
              onChange={(e) => setWidth(Number(e.target.value) || 320)}
            />
          </label>
          <label className="sr-st-field">
            <span>{t('studio.gif.fps')}</span>
            <input
              type="number"
              min={2}
              max={25}
              value={fps}
              onChange={(e) => setFps(Math.min(25, Math.max(2, Number(e.target.value) || 10)))}
            />
          </label>
          <Button size="sm" onClick={() => void make()} disabled={busy}>
            {t('studio.gif.make')}
          </Button>
          {busy && (
            <Button size="sm" variant="ghost" onClick={() => abort.current?.abort()}>
              {t('studio.gif.cancel')}
            </Button>
          )}
        </div>
      )}
      {phase.kind === 'reading' && (
        <ProgressBar
          value={Math.round((phase.done / Math.max(1, phase.total)) * 100)}
          label={t('studio.gif.progress', { done: phase.done, total: phase.total })}
          size={6}
        />
      )}
      {phase.kind === 'encoding' && (
        <p className="sr-tl-muted" role="status">
          {t('studio.gif.encoding')}
        </p>
      )}
      {error && (
        <p role="alert" className="sr-st-error">
          {error}
        </p>
      )}
      {result && (
        <div className="sr-st-result">
          <img src={result.url} alt={t('studio.gif.preview')} className="sr-st-gif" />
          <p className="sr-tl-muted" role="status">
            {t('studio.gif.done', { size: fmtBytes(result.size) })}
          </p>
          <a className="lng-link-btn" href={result.url} download={`${name}.gif`}>
            {t('studio.gif.download')}
          </a>
        </div>
      )}
    </div>
  );
}
