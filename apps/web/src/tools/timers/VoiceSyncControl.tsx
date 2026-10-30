import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useVoicePerms } from '../../lounge/voice/perms';
import { useVoiceSession } from '../../voice/session';
import {
  type Segment,
  type TimerState,
  elapsedMs,
  formatClock,
  fromWire,
  remainingMs,
  statusLine,
  toWire,
} from './timerCore';
import {
  type SharedTimer,
  publishVoiceTimer,
  startVoiceTimerListener,
  useVoiceTimers,
} from './voiceTimer';

/**
 * "Sync to voice channel": publishes the timer to the channel the member is connected to, as its status line
 * and a `VOICE_TIMER_UPDATE` the others' clients follow. Needs SET_VOICE_CHANNEL_STATUS (the server checks).
 */
export default function VoiceSyncControl({ state }: { state: TimerState }) {
  const t = useT();
  const channelId = useVoiceSession((s) => s.channelId);
  const guildId = useVoiceSession((s) => s.guildId);
  if (!channelId) return <p className="ud-muted">{t('tools.timers.voiceJoin')}</p>;
  return <Connected state={state} channelId={channelId} guildId={guildId} />;
}

/** Rendered only while connected, so the channel and permission lookups happen only then. */
function Connected({
  state,
  channelId,
  guildId,
}: {
  state: TimerState;
  channelId: string;
  guildId: string | null;
}) {
  const t = useT();
  const perms = useVoicePerms(guildId ?? undefined, channelId);
  const [synced, setSynced] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const allowed = perms.setVoiceStatus || perms.manageChannels;
  const on = synced === channelId;
  const idle = state.phase === 'idle' || state.phase === 'done';

  const labels: Record<Segment | 'countdown' | 'stopwatch', string> = {
    focus: t('tools.timers.statusFocus'),
    short: t('tools.timers.statusShort'),
    long: t('tools.timers.statusLong'),
    countdown: t('tools.timers.statusCountdown'),
    stopwatch: t('tools.timers.statusStopwatch'),
  };

  const publish = async (on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await publishVoiceTimer(
        channelId,
        on ? toWire(state) : null,
        on ? statusLine(state, Date.now(), labels) : '',
      );
      setSynced(on ? channelId : null);
    } catch (e) {
      setError(
        e instanceof ApiError && e.status === 403
          ? t('tools.timers.voiceDenied')
          : t('tools.timers.voiceFailed'),
      );
    } finally {
      setBusy(false);
    }
  };

  // While shared, every change of the timer (pause, resume, skip, a new segment) is sent on; a timer that is
  // reset or has finished stops being shared.
  const sent = useRef<TimerState | null>(null);
  useEffect(() => {
    if (!on) {
      sent.current = null;
      return;
    }
    if (sent.current === null) {
      sent.current = state;
      return;
    }
    if (sent.current === state) return;
    sent.current = state;
    void publish(!idle);
    // `publish` closes over the state it is called with.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, state]);

  // Follow other members' timers in this channel for as long as this control is on screen.
  useEffect(() => startVoiceTimerListener(), []);
  const shared = useVoiceTimers((s) => s.byChannel[channelId]);

  return (
    <div className="ud-voice">
      <button
        type="button"
        className="ud-btn"
        disabled={busy || !allowed || (!on && idle)}
        aria-pressed={on}
        onClick={() => void publish(!on)}
      >
        {on ? t('tools.timers.voiceStop') : t('tools.timers.voiceStart')}
      </button>
      {!allowed ? <p className="ud-muted">{t('tools.timers.voiceDenied')}</p> : null}
      {on ? <p className="ud-muted">{t('tools.timers.voiceOn')}</p> : null}
      {error ? (
        <p role="alert" className="ud-error">
          {error}
        </p>
      ) : null}
      {shared && !on ? <SharedClock shared={shared} labels={labels} /> : null}
    </div>
  );
}

/** Another member's timer in the connected channel, on the server's clock. */
function SharedClock({
  shared,
  labels,
}: {
  shared: SharedTimer;
  labels: Record<Segment | 'countdown' | 'stopwatch', string>;
}) {
  const t = useT();
  const [now, setNow] = useState(() => Date.now());
  const state = fromWire(shared.timer);
  useEffect(() => {
    if (state.phase !== 'running') return undefined;
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [state.phase]);
  const at = now + shared.skewMs;
  const left = remainingMs(state, at);
  const name = state.kind === 'pomodoro' ? labels[state.segment] : labels[state.kind];
  return (
    <p className="ud-muted" role="status">
      {t('tools.timers.sharedClock', {
        name,
        clock: formatClock(left ?? elapsedMs(state, at)),
        phase: state.phase,
      })}
    </p>
  );
}
