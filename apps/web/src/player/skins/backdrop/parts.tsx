/**
 * MUS-SKN-001: live controls the backdrop skins add on top of the shared ones in `../../controls`. Every one is
 * a real control on the player store, with a label and a focus ring; the skins only change how they look.
 */
import type { ReactNode } from 'react';
import { useT } from '../../../i18n';
import { formatTime, playIndex } from '../../controls';
import { openPlayerLibrary } from '../../presence';
import { seek, setEqEnabled, toggleShuffle, usePlayerStore } from '../../store';

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="1em" height="1em" fill="currentColor" aria-hidden="true">
    {children}
  </svg>
);
export const IconShuffle = () => (
  <Svg>
    <path d="M4 7h3.5l7 10H19v-2l3 3-3 3v-2h-5.5l-7-10H4zm10.5 0H19V5l3 3-3 3V9h-4.5l-1.6 2.3-1.2-1.8zM4 17h3.5l1.6-2.3 1.2 1.8L8.5 19H4z" />
  </Svg>
);
export const IconLibrary = () => (
  <Svg>
    <path d="M4 4h3v16H4zm5 0h3v16H9zm5.2.6l2.9-.8 4.1 15.4-2.9.8z" />
  </Svg>
);
export const IconEject = () => (
  <Svg>
    <path d="M12 5l7 8H5zM5 16h14v3H5z" />
  </Svg>
);

export function ShuffleButton({
  className,
  children,
}: {
  className?: string;
  children?: ReactNode;
}) {
  const t = useT();
  const on = usePlayerStore((s) => s.shuffle);
  return (
    <button
      type="button"
      className={className}
      aria-pressed={on}
      aria-label={t('player.skinx.shuffle')}
      data-on={on}
      onClick={toggleShuffle}
    >
      {children ?? <IconShuffle />}
    </button>
  );
}

/** The equalizer on or off (the same switch as the library's Equalizer section). */
export function EqSwitch({
  className,
  children,
  label,
}: {
  className?: string;
  children?: ReactNode;
  /** The accessible name, when the skin's visible text differs (it must start with that text). */
  label?: string;
}) {
  const t = useT();
  const on = usePlayerStore((s) => s.eqEnabled);
  return (
    <button
      type="button"
      className={className}
      aria-pressed={on}
      aria-label={label ?? t('music.lib.eq.enabled')}
      data-on={on}
      onClick={() => setEqEnabled(!on)}
    >
      {children}
    </button>
  );
}

/** Opens the Full Library Manager skin on its Library tab (the rail's music button does the same). */
export function LibraryButton({
  className,
  children,
  label,
}: {
  className?: string;
  children?: ReactNode;
  label?: string;
}) {
  const t = useT();
  return (
    <button
      type="button"
      className={className}
      aria-label={label ?? t('player.skinx.openLibrary')}
      onClick={openPlayerLibrary}
    >
      {children ?? <IconLibrary />}
    </button>
  );
}

/** Four cue points that jump to the start, a quarter, half and three quarters of the track. */
export function CuePads({ className }: { className?: string }) {
  const t = useT();
  const duration = usePlayerStore((s) => s.duration);
  return (
    <div className={className} role="group" aria-label={t('player.skinx.cues')}>
      {[0, 25, 50, 75].map((pct, i) => (
        <button
          key={pct}
          type="button"
          className="sr-ply-cue"
          data-cue={i + 1}
          aria-label={t('player.skinx.cue', { n: i + 1, pct })}
          disabled={duration <= 0}
          onClick={() => seek((duration * pct) / 100)}
        >
          <span aria-hidden="true">{t('player.skinx.cueShort', { n: i + 1 })}</span>
        </button>
      ))}
    </div>
  );
}

/** The queue as a list of buttons: each plays its track. */
export function QueueButtons({
  className,
  showLength = true,
  max,
}: {
  className?: string;
  showLength?: boolean;
  max?: number;
}) {
  const t = useT();
  const queue = usePlayerStore((s) => s.queue);
  const index = usePlayerStore((s) => s.index);
  const duration = usePlayerStore((s) => s.duration);
  if (queue.length === 0) return <p className="sr-ply-empty">{t('player.queue.empty')}</p>;
  const shown = max ? queue.slice(0, max) : queue;
  return (
    <ol className={className} aria-label={t('player.queue.title')}>
      {shown.map((track, i) => {
        const len = i === index && duration > 0 ? duration : track.durationSec;
        return (
          <li key={`${i}:${track.id}`} data-current={i === index}>
            <button
              type="button"
              aria-current={i === index ? 'true' : undefined}
              onClick={() => playIndex(i)}
            >
              <span className="sr-ply-q-n">{String(i + 1).padStart(2, '0')}</span>
              <span className="sr-ply-q-title">
                {track.artist ? `${track.title} — ${track.artist}` : track.title}
              </span>
              {showLength && (
                <span className="sr-ply-q-len">{len && len > 0 ? formatTime(len) : '--:--'}</span>
              )}
            </button>
          </li>
        );
      })}
    </ol>
  );
}

/** "n/total" for the queue. */
export function useQueueCount(): { n: number; total: number } {
  const n = usePlayerStore((s) => s.index + 1);
  const total = usePlayerStore((s) => s.queue.length);
  return { n, total };
}
