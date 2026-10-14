// Things drawn over the picture: skip intro/credits (CIN-PLY-005), next-episode card with countdown and the
// "Are you still watching?" prompt (CIN-PLY-006), the resume prompt (CIN-PLY-008) and the shortcut feedback.
import { Button, Modal } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { PlayGlyph } from './icons';
import { NEXT_COUNTDOWN_S, formatClock, nextLine, skipLabel } from './model';
import type { Marker, NextEpisode } from './types';

export function SkipButton({ marker, onSkip }: { marker: Marker; onSkip: () => void }) {
  return (
    <button type="button" className="cpl-skip" onClick={onSkip}>
      {skipLabel(marker)}
    </button>
  );
}

/** Counts down once per second while `active`; calls `onDone` at zero. */
export function useCountdown(active: boolean, from: number, onDone: () => void) {
  const [left, setLeft] = useState(from);
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    if (!active) {
      setLeft(from);
      return;
    }
    setLeft(from);
    const id = setInterval(() => {
      setLeft((n) => {
        if (n <= 1) {
          clearInterval(id);
          // After the state update, never during it.
          queueMicrotask(() => done.current());
          return 0;
        }
        return n - 1;
      });
    }, 1000);
    return () => clearInterval(id);
  }, [active, from]);
  return left;
}

export interface NextCardProps {
  next: NextEpisode;
  /** Count down and start the next episode by itself. */
  autoplay: boolean;
  /** `auto`: the countdown ran out (counts toward "Are you still watching?"); false when the viewer pressed it. */
  onPlayNext: (auto: boolean) => void;
  onCancel: () => void;
}

export function NextCard({ next, autoplay, onPlayNext, onCancel }: NextCardProps) {
  const t = useT();
  const left = useCountdown(autoplay, NEXT_COUNTDOWN_S, () => onPlayNext(true));
  return (
    <section className="cpl-next" aria-label={t('cinema.overlays.nextEpisode')}>
      {next.still_url && <img className="cpl-next-still" src={next.still_url} alt="" />}
      <div className="cpl-next-body">
        <p className="cpl-next-eyebrow">{t('cinema.overlays.upNext')}</p>
        <p className="cpl-next-title">{nextLine(next)}</p>
        {autoplay && (
          <p className="cpl-next-count" role="timer" aria-live="off">
            {t('cinema.overlays.startsIn', { n: left })}
          </p>
        )}
        <div className="cpl-next-actions">
          <Button size="sm" leadingIcon={<PlayGlyph size={16} />} onClick={() => onPlayNext(false)}>
            {t('cinema.overlays.playNow')}
          </Button>
          <Button size="sm" variant="secondary" onClick={onCancel}>
            {autoplay ? t('cinema.overlays.cancel') : t('cinema.overlays.dismiss')}
          </Button>
        </div>
      </div>
    </section>
  );
}

export function StillWatching({
  onContinue,
  onStop,
}: {
  onContinue: () => void;
  onStop: () => void;
}) {
  const t = useT();
  return (
    <Modal
      open
      onClose={onStop}
      title={t('cinema.overlays.stillWatchingTitle')}
      description={t('cinema.overlays.stillWatchingBody')}
      size="sm"
      footer={
        <>
          <Button variant="secondary" onClick={onStop}>
            {t('cinema.overlays.stop')}
          </Button>
          <Button onClick={onContinue}>{t('cinema.overlays.keepWatching')}</Button>
        </>
      }
    >
      {null}
    </Modal>
  );
}

export function ResumePrompt({
  at,
  onResume,
  onStartOver,
}: {
  at: number;
  onResume: () => void;
  onStartOver: () => void;
}) {
  const t = useT();
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <div className="cpl-resume" role="group" aria-label={t('cinema.overlays.resumePlayback')}>
      <p className="cpl-resume-text">{t('cinema.overlays.pickUp')}</p>
      <div className="cpl-next-actions">
        <button ref={ref} type="button" className="cpl-primary" onClick={onResume}>
          {t('cinema.overlays.resumeFrom', { time: formatClock(at) })}
        </button>
        <button type="button" className="cpl-secondary" onClick={onStartOver}>
          {t('cinema.overlays.startOver')}
        </button>
      </div>
    </div>
  );
}

/** Short on-screen echo of a shortcut ("+10 s", "Volume 55%"), mirrored to a polite live region. */
export function Feedback({ feedback }: { feedback: { id: number; text: string } | null }) {
  return (
    <>
      {feedback && (
        <div key={feedback.id} className="cpl-feedback" aria-hidden="true">
          {feedback.text}
        </div>
      )}
      <div className="cpl-sr-only" role="status" aria-live="polite">
        {feedback?.text ?? ''}
      </div>
    </>
  );
}
