// The trimmer of the soundboard upload dialog (J-06, D-201): a waveform with a five second window, a preview, and
// "Use this part", which hands the window back as a WAV file.
import { Button } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../../i18n';
import { ActionStatus } from '../ui';
import { durationOf, maxStart, peaks, trimToWav, type PcmSource } from './soundTrim';

const BINS = 160;

/** Plays `[start, start + length)` of the sound; returns a stop function. `onEnd` runs when it ends or is stopped. */
function previewWindow(
  audio: PcmSource,
  start: number,
  length: number,
  onEnd: () => void,
): () => void {
  const w = globalThis as unknown as {
    AudioContext?: new () => AudioContext;
    webkitAudioContext?: new () => AudioContext;
  };
  const Ctx = w.AudioContext ?? w.webkitAudioContext;
  if (!Ctx) {
    onEnd();
    return () => undefined;
  }
  const ctx = new Ctx();
  const src = ctx.createBufferSource();
  src.buffer = audio as AudioBuffer;
  src.connect(ctx.destination);
  let ended = false;
  const finish = () => {
    if (ended) return;
    ended = true;
    void ctx.close?.();
    onEnd();
  };
  src.onended = finish;
  src.start(0, start, length);
  return () => {
    try {
      src.stop();
    } catch {
      /* already stopped */
    }
    finish();
  };
}

export function SoundTrimmer({
  audio,
  fileName,
  maxSeconds,
  maxBytes,
  onUse,
  onCancel,
}: {
  audio: PcmSource;
  /** The source file's name; the cut is named after it. */
  fileName: string;
  maxSeconds: number;
  maxBytes: number;
  onUse: (file: File) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const total = durationOf(audio);
  const last = maxStart(audio, maxSeconds);
  const [start, setStart] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stop = useRef<(() => void) | null>(null);
  const bars = useMemo(() => peaks(audio, BINS), [audio]);
  useEffect(() => () => stop.current?.(), []);

  const end = Math.min(total, start + maxSeconds);
  const togglePlay = () => {
    if (playing) {
      stop.current?.();
      return;
    }
    setPlaying(true);
    stop.current = previewWindow(audio, start, end - start, () => {
      stop.current = null;
      setPlaying(false);
    });
  };

  const use = () => {
    stop.current?.();
    const out = trimToWav(audio, start, maxSeconds, maxBytes);
    if (!out.ok) {
      setError(
        t('settings.soundboard.trimTooBig', {
          kb: String(Math.ceil(out.bytes / 1024)),
          max: String(Math.floor(maxBytes / 1024)),
        }),
      );
      return;
    }
    setError(null);
    const base = fileName.replace(/\.[^.]+$/, '') || 'sound';
    onUse(new File([out.bytes.buffer as ArrayBuffer], `${base}.wav`, { type: 'audio/wav' }));
  };

  const x = (start / total) * BINS;
  const w = ((end - start) / total) * BINS;
  return (
    <div className="ss-trimmer" role="group" aria-label={t('settings.soundboard.trimTitle')}>
      <p className="ss-sub">
        {t('settings.soundboard.trimIntro', {
          seconds: total.toFixed(1),
          max: String(maxSeconds),
        })}
      </p>
      <svg
        className="ss-trimmer-wave"
        viewBox={`0 0 ${BINS} 40`}
        preserveAspectRatio="none"
        aria-hidden="true"
        data-testid="trim-waveform"
      >
        {Array.from(bars, (p, i) => (
          <rect key={i} x={i} y={20 - p * 20} width={0.7} height={Math.max(0.5, p * 40)} />
        ))}
        <rect className="ss-trimmer-window" x={x} y={0} width={w} height={40} />
      </svg>
      <label className="ss-field-label" htmlFor="ss-trim-start">
        {t('settings.soundboard.trimStart')}
      </label>
      <input
        id="ss-trim-start"
        type="range"
        min={0}
        max={last}
        step={0.01}
        value={start}
        disabled={last === 0}
        aria-valuetext={t('settings.soundboard.trimRange', {
          from: start.toFixed(2),
          to: end.toFixed(2),
        })}
        onChange={(e) => {
          stop.current?.();
          setStart(Number(e.target.value));
        }}
      />
      <p className="ss-sub" data-testid="trim-range">
        {t('settings.soundboard.trimRange', { from: start.toFixed(2), to: end.toFixed(2) })}
      </p>
      <div className="ss-actions">
        <Button variant="secondary" onClick={togglePlay}>
          {playing ? t('settings.soundboard.trimStop') : t('settings.soundboard.trimPlay')}
        </Button>
        <Button onClick={use}>{t('settings.soundboard.trimUse')}</Button>
        <Button variant="ghost" onClick={onCancel}>
          {t('settings.soundboard.trimCancel')}
        </Button>
      </div>
      {error && <ActionStatus error={error} />}
    </div>
  );
}
