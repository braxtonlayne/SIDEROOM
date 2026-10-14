// Overlays of the theatre: floating reactions (WP-FE-006), "Waiting for ..." (buffering coordination), the
// join-playback button the browser's autoplay rule sometimes needs, and the ready check (WP-FE-007).
import { Button, Icon } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { partyController, useParty } from './runtime';
import { waitingLabel } from './sync';

/** Emoji drift up the screen and fade. With reduced motion they appear in a row and just fade out. */
export function ReactionsOverlay() {
  const t = useT();
  const reactions = useParty((s) => s.reactions);
  const party = useParty((s) => s.party);
  const name = (id: string) => {
    const p = party?.participants.find((x) => x.user_id === id);
    return p ? p.display_name || p.username : t('watchparty.overlays.someone');
  };
  return (
    <div className="wp-reactions" aria-hidden="true">
      {reactions.map((r, i) => (
        <span
          key={r.id}
          className="wp-reaction"
          style={{ left: `${8 + ((r.id * 37) % 84)}%`, ['--wp-slot' as string]: i % 8 }}
          title={name(r.userId)}
        >
          {r.emoji}
        </span>
      ))}
    </div>
  );
}

/** A polite live region: "Waiting for Ana" while the server holds everyone for a stalled player. */
export function WaitingBanner() {
  const t = useT();
  const waiting = useParty((s) => s.waitingFor);
  const auto = useParty((s) => s.party?.playback.auto_paused ?? false);
  const local = useParty((s) => s.localBuffering);
  const label = auto ? (waitingLabel(waiting) ?? t('watchparty.overlays.waitingForSomeone')) : null;
  return (
    <div className="wp-waiting" role="status" aria-live="polite">
      {label && (
        <span className="wp-waiting-pill">
          <Icon name="clock" size={16} /> {label}…
        </span>
      )}
      {!label && local && (
        <span className="wp-waiting-pill">
          <Icon name="clock" size={16} /> {t('watchparty.overlays.catchingUp')}
        </span>
      )}
    </div>
  );
}

/** The browser wants a click before it plays sound; one button joins the playback in sync. */
export function GestureButton() {
  const t = useT();
  const needs = useParty((s) => s.needsGesture);
  if (!needs) return null;
  return (
    <div className="wp-gesture">
      <Button onClick={() => void partyController().resumeFromGesture()}>
        {t('watchparty.overlays.joinPlayback')}
      </Button>
    </div>
  );
}

/**
 * Ready check: when a title was just opened and nothing has played yet, everybody can mark themselves ready and
 * the host sees who is missing. It never blocks anything; the host presses Play when they like.
 */
export function ReadyCheck() {
  const t = useT();
  const party = useParty((s) => s.party);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const key = party ? [party.media.asset_id, party.media.episode_id].join(':') : '';
  const started = !!party?.playback.playing;
  useEffect(() => {
    if (started) setDismissed(key);
  }, [started, key]);
  if (!party || dismissed === key) return null;
  // Only before anything played: paused at the very start, at least two people.
  if (party.playback.playing || party.playback.position_ms > 0 || party.participants.length < 2)
    return null;
  const me = partyController().meId;
  const mine = party.participants.find((p) => p.user_id === me);
  const ready = party.participants.filter((p) => p.ready).length;
  const total = party.participants.length;
  const toggle = async () => {
    setBusy(true);
    try {
      await partyController().setReady(!mine?.ready);
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="wp-ready" aria-label={t('watchparty.overlays.readyCheck')}>
      <p>
        <strong>{t('watchparty.overlays.readyCount', { ready, total })}</strong>
        {party.you?.can_control
          ? t('watchparty.overlays.pressPlay')
          : t('watchparty.overlays.hostWillStart')}
      </p>
      <div className="wp-ready-actions">
        <Button size="sm" onClick={() => void toggle()} loading={busy} aria-pressed={!!mine?.ready}>
          {mine?.ready ? t('watchparty.overlays.notReady') : t('watchparty.overlays.imReady')}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setDismissed(key)}>
          {t('watchparty.overlays.dismiss')}
        </Button>
      </div>
    </section>
  );
}
