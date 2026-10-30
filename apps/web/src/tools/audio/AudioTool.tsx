// UTL-AUD-001: the audio trimmer and normalizer. Used as a drawer tool (pick a file, export WAV or Opus) and
// embedded where an upload has limits (voice message, soundboard): then `limits` caps the length and the size and
// `onUse` receives the finished file.
import { Button } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n';
import {
  decodeAudioFile,
  durationOf,
  peaks,
  type PcmSource,
} from '../../settings/server/sections/soundTrim';
import {
  DEFAULT_CEILING_DB,
  DEFAULT_TARGET_DB,
  encodeOpus,
  encodeWav,
  opusMimeType,
  processAudio,
  type Processed,
} from './dsp';
import './audio-tool.css';

const BINS = 200;
const MIN_PART = 0.05;

export interface AudioLimits {
  /** Longest kept part, in seconds. */
  maxSeconds?: number;
  /** Largest exported file, in bytes. */
  maxBytes?: number;
  /** The only format `onUse` may receive: voice messages need Ogg, sounds take WAV. */
  format: 'wav' | 'ogg';
}

export interface AudioToolProps {
  /** A file to open straight away (embedded use). */
  file?: File | Blob;
  limits?: AudioLimits;
  onUse?: (file: File) => void;
  onCancel?: () => void;
}

function playPart(p: Processed, onEnd: () => void): () => void {
  const w = globalThis as unknown as {
    AudioContext?: new () => AudioContext;
    webkitAudioContext?: new () => AudioContext;
  };
  const Ctx = w.AudioContext ?? w.webkitAudioContext;
  const len = p.channels[0]?.length ?? 0;
  if (!Ctx || len === 0) {
    onEnd();
    return () => undefined;
  }
  const ctx = new Ctx();
  const buf = ctx.createBuffer(p.channels.length, len, p.sampleRate);
  p.channels.forEach((ch, c) => buf.copyToChannel(ch, c));
  const src = ctx.createBufferSource();
  src.buffer = buf;
  src.connect(ctx.destination);
  let ended = false;
  const finish = () => {
    if (ended) return;
    ended = true;
    void ctx.close?.();
    onEnd();
  };
  src.onended = finish;
  src.start();
  return () => {
    try {
      src.stop();
    } catch {
      /* already stopped */
    }
    finish();
  };
}

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export function AudioTool({ file: initial, limits, onUse, onCancel }: AudioToolProps) {
  const t = useT();
  const [source, setSource] = useState<{ audio: PcmSource; name: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [start, setStart] = useState(0);
  const [end, setEnd] = useState(0);
  const [fadeIn, setFadeIn] = useState(0);
  const [fadeOut, setFadeOut] = useState(0);
  const [normalize, setNormalize] = useState(false);
  const [targetDb, setTargetDb] = useState(DEFAULT_TARGET_DB);
  const [playing, setPlaying] = useState(false);
  const [busy, setBusy] = useState(false);
  const stopRef = useRef<(() => void) | null>(null);

  const open = async (f: Blob, name: string) => {
    setError(null);
    try {
      const audio = await decodeAudioFile(f);
      const total = durationOf(audio);
      setSource({ audio, name });
      setStart(0);
      setEnd(limits?.maxSeconds ? Math.min(total, limits.maxSeconds) : total);
    } catch {
      setSource(null);
      setError(t('tools.audio.unreadable'));
    }
  };
  useEffect(() => {
    if (initial) void open(initial, (initial as File).name || 'audio');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initial]);
  useEffect(() => () => stopRef.current?.(), []);

  const total = source ? durationOf(source.audio) : 0;
  const bars = useMemo(() => (source ? peaks(source.audio, BINS) : new Float32Array(0)), [source]);
  const maxPart = limits?.maxSeconds ?? total;

  const build = (): Processed | null =>
    source &&
    processAudio(source.audio, {
      startSeconds: start,
      endSeconds: end,
      fadeInSeconds: fadeIn,
      fadeOutSeconds: fadeOut,
      targetDb: normalize ? targetDb : null,
      ceilingDb: DEFAULT_CEILING_DB,
    });

  const setStartClamped = (v: number) => {
    stopRef.current?.();
    const s = Math.min(Math.max(0, v), end - MIN_PART);
    setStart(s);
    if (end - s > maxPart) setEnd(s + maxPart);
  };
  const setEndClamped = (v: number) => {
    stopRef.current?.();
    const e = Math.max(Math.min(total, v), start + MIN_PART);
    setEnd(e);
    if (e - start > maxPart) setStart(e - maxPart);
  };

  const togglePlay = () => {
    if (playing) {
      stopRef.current?.();
      return;
    }
    const p = build();
    if (!p) return;
    setPlaying(true);
    stopRef.current = playPart(p, () => {
      stopRef.current = null;
      setPlaying(false);
    });
  };

  const baseName = (source?.name ?? 'audio').replace(/\.[^.]+$/, '') || 'audio';
  const run = async (kind: 'wav' | 'opus') => {
    const p = build();
    if (!p) return;
    stopRef.current?.();
    setError(null);
    setBusy(true);
    try {
      let blob: Blob;
      let name: string;
      if (kind === 'wav') {
        blob = new Blob([encodeWav(p.channels, p.sampleRate).buffer as ArrayBuffer], {
          type: 'audio/wav',
        });
        name = `${baseName}-trimmed.wav`;
      } else {
        const ogg = limits?.format === 'ogg';
        const mime = opusMimeType(ogg);
        if (!mime || (ogg && !mime.startsWith('audio/ogg'))) {
          setError(t('tools.audio.opusUnsupported'));
          return;
        }
        blob = await encodeOpus(p, mime);
        name = `${baseName}-trimmed.${mime.startsWith('audio/ogg') ? 'ogg' : 'webm'}`;
      }
      if (limits?.maxBytes && blob.size > limits.maxBytes) {
        setError(
          t('tools.audio.tooBig', {
            kb: String(Math.ceil(blob.size / 1024)),
            max: String(Math.floor(limits.maxBytes / 1024)),
          }),
        );
        return;
      }
      if (onUse) onUse(new File([blob], name, { type: blob.type }));
      else download(blob, name);
    } catch {
      setError(t('tools.audio.exportFailed'));
    } finally {
      setBusy(false);
    }
  };

  const embeddedKind = limits?.format === 'ogg' ? 'opus' : 'wav';
  const x = total ? (start / total) * BINS : 0;
  const w = total ? ((end - start) / total) * BINS : 0;
  return (
    <div className="utl-aud" role="group" aria-label={t('tools.audio.title')}>
      {!initial && (
        <label className="utl-aud-field">
          <span>{t('tools.audio.choose')}</span>
          <input
            type="file"
            accept="audio/*"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void open(f, f.name);
            }}
          />
        </label>
      )}
      {source && (
        <>
          <svg
            className="utl-aud-wave"
            viewBox={`0 0 ${BINS} 40`}
            preserveAspectRatio="none"
            aria-hidden="true"
            data-testid="audio-waveform"
          >
            {Array.from(bars, (p, i) => (
              <rect key={i} x={i} y={20 - p * 20} width={0.7} height={Math.max(0.5, p * 40)} />
            ))}
            <rect className="utl-aud-window" x={x} y={0} width={w} height={40} />
          </svg>
          <div className="utl-aud-row">
            <label className="utl-aud-field">
              <span>{t('tools.audio.trimStart')}</span>
              <input
                type="range"
                min={0}
                max={total}
                step={0.01}
                value={start}
                aria-valuetext={`${start.toFixed(2)} s`}
                onChange={(e) => setStartClamped(Number(e.target.value))}
              />
            </label>
            <label className="utl-aud-field">
              <span>{t('tools.audio.trimEnd')}</span>
              <input
                type="range"
                min={0}
                max={total}
                step={0.01}
                value={end}
                aria-valuetext={`${end.toFixed(2)} s`}
                onChange={(e) => setEndClamped(Number(e.target.value))}
              />
            </label>
          </div>
          <p className="utl-aud-sub" data-testid="audio-range">
            {t('tools.audio.range', {
              from: start.toFixed(2),
              to: end.toFixed(2),
              length: (end - start).toFixed(2),
            })}
          </p>
          <div className="utl-aud-row">
            <label className="utl-aud-field">
              <span>{t('tools.audio.fadeIn')}</span>
              <input
                type="number"
                min={0}
                max={end - start}
                step={0.1}
                value={fadeIn}
                onChange={(e) => setFadeIn(Math.max(0, Number(e.target.value) || 0))}
              />
            </label>
            <label className="utl-aud-field">
              <span>{t('tools.audio.fadeOut')}</span>
              <input
                type="number"
                min={0}
                max={end - start}
                step={0.1}
                value={fadeOut}
                onChange={(e) => setFadeOut(Math.max(0, Number(e.target.value) || 0))}
              />
            </label>
          </div>
          <label className="utl-aud-check">
            <input
              type="checkbox"
              checked={normalize}
              onChange={(e) => setNormalize(e.target.checked)}
            />
            <span>{t('tools.audio.normalize')}</span>
          </label>
          {normalize && (
            <label className="utl-aud-field">
              <span>{t('tools.audio.target', { db: String(targetDb) })}</span>
              <input
                type="range"
                min={-30}
                max={-6}
                step={1}
                value={targetDb}
                onChange={(e) => setTargetDb(Number(e.target.value))}
              />
            </label>
          )}
          <p className="utl-aud-sub">{t('tools.audio.normalizeNote')}</p>
          <div className="utl-aud-actions">
            <Button variant="secondary" onClick={togglePlay}>
              {playing ? t('tools.audio.stop') : t('tools.audio.play')}
            </Button>
            {onUse ? (
              <Button disabled={busy} onClick={() => void run(embeddedKind)}>
                {busy ? t('tools.audio.working') : t('tools.audio.use')}
              </Button>
            ) : (
              <>
                <Button disabled={busy} onClick={() => void run('wav')}>
                  {t('tools.audio.exportWav')}
                </Button>
                <Button disabled={busy} variant="secondary" onClick={() => void run('opus')}>
                  {busy ? t('tools.audio.working') : t('tools.audio.exportOpus')}
                </Button>
              </>
            )}
            {onCancel && (
              <Button variant="ghost" onClick={onCancel}>
                {t('tools.audio.cancel')}
              </Button>
            )}
          </div>
        </>
      )}
      {error && (
        <p role="alert" className="utl-aud-error">
          {error}
        </p>
      )}
    </div>
  );
}
