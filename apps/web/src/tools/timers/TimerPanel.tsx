import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import '../utl-dev.css';
import {
  MAX_COUNTDOWN_MS,
  type Segment,
  type TimerKind,
  formatClock,
  parseClock,
} from './timerCore';
import { useFocusTimer } from './useFocusTimer';
import VoiceSyncControl from './VoiceSyncControl';

/** The face shared by the pomodoro, stopwatch and countdown tools. */
export default function TimerPanel({ kind }: { kind: TimerKind } & Partial<ToolProps>) {
  const t = useT();
  const [announce, setAnnounce] = useState('');
  const timer = useFocusTimer(kind, {
    onFinish: (finished) => {
      const last = finished[finished.length - 1] as Segment | undefined;
      setAnnounce(
        kind === 'pomodoro'
          ? last === 'focus'
            ? t('tools.timers.focusDone')
            : t('tools.timers.breakDone')
          : t('tools.timers.countdownDone'),
      );
    },
  });
  const { state } = timer;
  const [custom, setCustom] = useState('');
  const [customError, setCustomError] = useState(false);
  const [laps, setLaps] = useState<number[]>([]);
  useEffect(() => {
    if (kind === 'stopwatch' && state.phase === 'idle') setLaps([]);
  }, [kind, state.phase]);

  const segmentName =
    state.segment === 'focus'
      ? t('tools.timers.segmentFocus')
      : state.segment === 'short'
        ? t('tools.timers.segmentShort')
        : t('tools.timers.segmentLong');

  const applyCustom = () => {
    const ms = parseClock(custom);
    if (ms === null || ms > MAX_COUNTDOWN_MS) {
      setCustomError(true);
      return;
    }
    setCustomError(false);
    timer.setCountdownMs(ms);
  };

  const startLabel =
    state.phase === 'paused'
      ? t('tools.timers.resume')
      : state.phase === 'done'
        ? t('tools.timers.restart')
        : t('tools.timers.start');

  return (
    <div className="ud-timer">
      {kind === 'pomodoro' ? (
        <p className="ud-segment" data-segment={state.segment}>
          {segmentName}
          <span className="ud-muted"> · {t('tools.timers.sessions', { n: state.completed })}</span>
        </p>
      ) : null}
      <div
        className="ud-clock"
        role="timer"
        aria-label={t(`tools.${kind}.name`)}
        data-phase={state.phase}
      >
        {timer.clock}
      </div>
      {kind !== 'stopwatch' ? (
        <progress
          className="ud-progress"
          value={Math.round(timer.progress * 1000)}
          max={1000}
          aria-hidden="true"
        />
      ) : null}
      <div className="ud-row">
        <button
          type="button"
          className="ud-btn ud-btn-primary"
          onClick={() => (timer.running ? timer.pause() : timer.start())}
        >
          {timer.running ? t('tools.timers.pause') : startLabel}
        </button>
        <button type="button" className="ud-btn" onClick={timer.reset}>
          {t('tools.timers.reset')}
        </button>
        {kind === 'pomodoro' ? (
          <button type="button" className="ud-btn" onClick={timer.skip}>
            {t('tools.timers.skip')}
          </button>
        ) : null}
        {kind === 'stopwatch' ? (
          <button
            type="button"
            className="ud-btn"
            disabled={!timer.running}
            onClick={() => setLaps((l) => [...l, timer.elapsedMs])}
          >
            {t('tools.timers.lap')}
          </button>
        ) : null}
      </div>

      {kind === 'countdown' && state.phase !== 'running' ? (
        <div className="ud-row">
          <label className="ud-field">
            <span>{t('tools.timers.length')}</span>
            <input
              value={custom}
              inputMode="text"
              placeholder="5:00"
              aria-invalid={customError}
              onChange={(e) => setCustom(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') applyCustom();
              }}
            />
          </label>
          <button type="button" className="ud-btn" onClick={applyCustom}>
            {t('tools.timers.set')}
          </button>
          {customError ? (
            <p role="alert" className="ud-error">
              {t('tools.timers.badLength')}
            </p>
          ) : null}
        </div>
      ) : null}

      {kind === 'pomodoro' && state.phase === 'idle' ? (
        <fieldset className="ud-row ud-lengths">
          <legend className="ud-muted">{t('tools.timers.lengths')}</legend>
          {(
            [
              ['focusMs', 'tools.timers.segmentFocus'],
              ['shortMs', 'tools.timers.segmentShort'],
              ['longMs', 'tools.timers.segmentLong'],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="ud-field ud-field-narrow">
              <span>{t(label)}</span>
              <input
                type="number"
                min={1}
                max={180}
                value={Math.round(timer.config[key] / 60_000)}
                onChange={(e) => {
                  const m = Math.min(180, Math.max(1, Number(e.target.value) || 1));
                  timer.setConfig({ [key]: m * 60_000 });
                }}
              />
            </label>
          ))}
        </fieldset>
      ) : null}

      {laps.length > 0 ? (
        <ol className="ud-laps" aria-label={t('tools.timers.laps')}>
          {laps.map((ms, i) => (
            <li key={i}>
              {formatClock(ms - (laps[i - 1] ?? 0), true)}
              <span className="ud-muted"> · {formatClock(ms, true)}</span>
            </li>
          ))}
        </ol>
      ) : null}

      <VoiceSyncControl state={state} />
      <p className="ud-sr" role="status" aria-live="polite">
        {announce}
      </p>
    </div>
  );
}
