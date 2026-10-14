// Things drawn over the picture: skip intro/credits (CIN-PLY-005), next-episode card with countdown and the
// "Are you still watching?" prompt (CIN-PLY-006), the resume prompt (CIN-PLY-008) and the shortcut feedback.
import { Button, Modal } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { PlayGlyph } from './icons';
import { NEXT_COUNTDOWN_S, formatClock, nextLine, skipLabel } from './model';
import type { Marker, NextEpisode } from './types';

export function SkipButton({ marker, onSkip }: { marker: Marker; onSkip: () => void }) {
  return (
    <button type="button" className="cin-skip" onClick={onSkip}>
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
  const left = useCountdown(autoplay, NEXT_COUNTDOWN_S, () => onPlayNext(true));
  return (
    <section className="cin-next" aria-label="Next episode">
      {next.still_url && <img className="cin-next-still" src={next.still_url} alt="" />}
      <div className="cin-next-body">
        <p className="cin-next-eyebrow">Up next</p>
        <p className="cin-next-title">{nextLine(next)}</p>
        {autoplay && (
          <p className="cin-next-count" role="timer" aria-live="off">
            Starts in {left} s
          </p>
        )}
        <div className="cin-next-actions">
          <Button size="sm" leadingIcon={<PlayGlyph size={16} />} onClick={() => onPlayNext(false)}>
            Play now
          </Button>
          <Button size="sm" variant="secondary" onClick={onCancel}>
            {autoplay ? 'Cancel' : 'Dismiss'}
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
  return (
    <Modal
      open
      onClose={onStop}
      title="Are you still watching?"
      description="Playback paused after a few episodes in a row."
      size="sm"
      footer={
        <>
          <Button variant="secondary" onClick={onStop}>
            Stop
          </Button>
          <Button onClick={onContinue}>Keep watching</Button>
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
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <div className="cin-resume" role="group" aria-label="Resume playback">
      <p className="cin-resume-text">Pick up where you left off?</p>
      <div className="cin-next-actions">
        <button ref={ref} type="button" className="cin-primary" onClick={onResume}>
          Resume from {formatClock(at)}
        </button>
        <button type="button" className="cin-secondary" onClick={onStartOver}>
          Start over
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
        <div key={feedback.id} className="cin-feedback" aria-hidden="true">
          {feedback.text}
        </div>
      )}
      <div className="cin-sr-only" role="status" aria-live="polite">
        {feedback?.text ?? ''}
      </div>
    </>
  );
}
