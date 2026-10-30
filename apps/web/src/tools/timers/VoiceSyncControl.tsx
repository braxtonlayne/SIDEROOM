import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useVoicePerms } from '../../lounge/voice/perms';
import { useVoiceSession } from '../../voice/session';
import { type Segment, type TimerState, statusLine, toWire } from './timerCore';
import { publishVoiceTimer } from './voiceTimer';

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

  const run = async (publish: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const wire = publish ? toWire(state) : null;
      await publishVoiceTimer(
        channelId,
        wire,
        publish ? statusLine(state, Date.now(), labels) : '',
      );
      setSynced(publish ? channelId : null);
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

  return (
    <div className="ud-voice">
      <button
        type="button"
        className="ud-btn"
        disabled={busy || !allowed || (!on && idle)}
        aria-pressed={on}
        onClick={() => void run(!on)}
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
    </div>
  );
}
