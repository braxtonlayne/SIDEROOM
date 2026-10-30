/** MUS-PLY-001: the controls every skin shares. Skins differ in layout and styling, never in behaviour. */
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useT } from '../i18n';
import { useReducedMotionPref } from '../settings/user/gifAutoplay';
import { activeGraph } from './engine';
import { readLevel } from './graph';
import {
  clear,
  currentTrack,
  cycleRepeat,
  next,
  pause,
  play,
  prev,
  seek,
  setDock,
  setSkin,
  setVolume,
  toggle,
  toggleMute,
  usePlayerStore,
} from './store';
import { PLAYER_DOCKS, PLAYER_SKINS } from './types';

export function formatTime(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) return '0:00';
  const s = Math.floor(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="1em" height="1em" fill="currentColor" aria-hidden="true">
    {children}
  </svg>
);
export const IconPlay = () => (
  <Svg>
    <path d="M8 5v14l11-7z" />
  </Svg>
);
export const IconPause = () => (
  <Svg>
    <path d="M6 5h4v14H6zM14 5h4v14h-4z" />
  </Svg>
);
export const IconPrev = () => (
  <Svg>
    <path d="M6 5h2v14H6zM20 5v14L9 12z" />
  </Svg>
);
export const IconNext = () => (
  <Svg>
    <path d="M16 5h2v14h-2zM4 5l11 7L4 19z" />
  </Svg>
);
export const IconStop = () => (
  <Svg>
    <path d="M6 6h12v12H6z" />
  </Svg>
);
export const IconVolume = ({ muted }: { muted: boolean }) => (
  <Svg>
    <path d="M4 9v6h4l5 4V5L8 9z" />
    {muted ? (
      <path d="M16 9l5 6m0-6l-5 6" stroke="currentColor" strokeWidth="2" />
    ) : (
      <path d="M16 8a5 5 0 010 8" stroke="currentColor" strokeWidth="2" fill="none" />
    )}
  </Svg>
);
export const IconRepeat = () => (
  <Svg>
    <path d="M7 7h10v3l4-4-4-4v3H5v6h2zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2z" />
  </Svg>
);
export const IconClose = () => (
  <Svg>
    <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth="2" />
  </Svg>
);
export const IconSkin = () => (
  <Svg>
    <path d="M12 3a9 9 0 100 18c1.1 0 2-.9 2-2 0-.5-.2-1-.5-1.3-.3-.4-.5-.8-.5-1.2 0-1.1.9-2 2-2h2.4a3.6 3.6 0 003.6-3.6C21 6.6 17 3 12 3zM7.5 12a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm3-4a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm5 0a1.5 1.5 0 110-3 1.5 1.5 0 010 3z" />
  </Svg>
);
export const IconDock = () => (
  <Svg>
    <path d="M4 4h16v16H4zm2 2v12h12V6z" />
  </Svg>
);

export const isBusy = (status: string) => status === 'playing' || status === 'loading';

/** The now-playing line, "Artist - Title" or a hint for idle, ended and failed states. */
export function useNowPlayingText(): string {
  const t = useT();
  const track = usePlayerStore((s) => currentTrack(s));
  const status = usePlayerStore((s) => s.status);
  if (status === 'error') return t('player.error');
  if (!track) return t('player.nothing');
  return track.artist ? `${track.title} — ${track.artist}` : track.title;
}

export function PlayPauseButton({ className }: { className?: string }) {
  const t = useT();
  const status = usePlayerStore((s) => s.status);
  const busy = isBusy(status);
  return (
    <button
      type="button"
      className={className}
      aria-label={busy ? t('player.pause') : t('player.play')}
      onClick={toggle}
    >
      {busy ? <IconPause /> : <IconPlay />}
    </button>
  );
}

export function PrevButton({ className }: { className?: string }) {
  const t = useT();
  return (
    <button type="button" className={className} aria-label={t('player.prev')} onClick={prev}>
      <IconPrev />
    </button>
  );
}

export function NextButton({ className }: { className?: string }) {
  const t = useT();
  return (
    <button
      type="button"
      className={className}
      aria-label={t('player.next')}
      onClick={() => next()}
    >
      <IconNext />
    </button>
  );
}

export function StopButton({ className }: { className?: string }) {
  const t = useT();
  return (
    <button
      type="button"
      className={className}
      aria-label={t('player.stop')}
      onClick={() => {
        pause();
        seek(0);
      }}
    >
      <IconStop />
    </button>
  );
}

export function RepeatButton({ className }: { className?: string }) {
  const t = useT();
  const repeat = usePlayerStore((s) => s.repeat);
  return (
    <button
      type="button"
      className={className}
      aria-pressed={repeat !== 'off'}
      aria-label={t(`player.repeat.${repeat}`)}
      data-repeat={repeat}
      onClick={cycleRepeat}
    >
      <IconRepeat />
      {repeat === 'one' && <span className="sr-ply-badge">1</span>}
    </button>
  );
}

export function CloseButton({ className }: { className?: string }) {
  const t = useT();
  return (
    <button type="button" className={className} aria-label={t('player.close')} onClick={clear}>
      <IconClose />
    </button>
  );
}

const PLACE_KEY = {
  'bottom-end': 'player.place.bottomEnd',
  'bottom-start': 'player.place.bottomStart',
  'top-end': 'player.place.topEnd',
  'top-start': 'player.place.topStart',
} as const;

export function DockButton({ className }: { className?: string }) {
  const t = useT();
  const dock = usePlayerStore((s) => s.dock);
  const nextDock = PLAYER_DOCKS[(PLAYER_DOCKS.indexOf(dock) + 1) % PLAYER_DOCKS.length] ?? dock;
  return (
    <button
      type="button"
      className={className}
      aria-label={t('player.moveTo', { place: t(PLACE_KEY[nextDock]) })}
      onClick={() => setDock(nextDock)}
    >
      <IconDock />
    </button>
  );
}

/** One small button that moves to the next skin; the full picker is in Settings > Appearance. */
export function SkinButton({ className }: { className?: string }) {
  const t = useT();
  const skin = usePlayerStore((s) => s.skin);
  const nextSkin = PLAYER_SKINS[(PLAYER_SKINS.indexOf(skin) + 1) % PLAYER_SKINS.length] ?? skin;
  return (
    <button
      type="button"
      className={className}
      aria-label={t('player.cycleSkin', { skin: t(`player.skins.${nextSkin}`) })}
      title={t(`player.skins.${nextSkin}`)}
      onClick={() => setSkin(nextSkin)}
    >
      <IconSkin />
    </button>
  );
}

export function SeekBar({ className }: { className?: string }) {
  const t = useT();
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const hasTrack = usePlayerStore((s) => s.index >= 0);
  return (
    <input
      type="range"
      className={className}
      aria-label={t('player.seek')}
      aria-valuetext={`${formatTime(position)} / ${formatTime(duration)}`}
      min={0}
      max={duration > 0 ? Math.floor(duration) : 0}
      step={1}
      value={Math.min(Math.floor(position), Math.floor(duration))}
      disabled={!hasTrack || duration <= 0}
      onChange={(e) => seek(Number(e.target.value))}
    />
  );
}

export function VolumeControl({ className }: { className?: string }) {
  const t = useT();
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  return (
    <span className="sr-ply-volume">
      <button
        type="button"
        className={className}
        aria-pressed={muted}
        aria-label={muted ? t('player.unmute') : t('player.mute')}
        onClick={toggleMute}
      >
        <IconVolume muted={muted || volume === 0} />
      </button>
      <input
        type="range"
        aria-label={t('player.volume')}
        min={0}
        max={100}
        step={1}
        value={Math.round((muted ? 0 : volume) * 100)}
        onChange={(e) => setVolume(Number(e.target.value) / 100)}
      />
    </span>
  );
}

export function TimeReadout() {
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  return (
    <span className="sr-ply-time">
      {formatTime(position)} / {formatTime(duration)}
    </span>
  );
}

/**
 * The LCD ticker. It scrolls only while a track plays and never under reduced motion, where the line is shown
 * whole and truncated instead (`data-motion="off"`).
 */
export function Ticker({ text }: { text: string }) {
  const reduced = useReducedMotionPref();
  const status = usePlayerStore((s) => s.status);
  const moving = !reduced && status === 'playing';
  const box = useRef<HTMLSpanElement>(null);
  const [shift, setShift] = useState(0);
  // A title that does not fit pans to its end and back (pausing at both ends); one that fits stays put.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const inner = el.firstElementChild as HTMLElement | null;
      if (inner) setShift(Math.max(0, Math.ceil(inner.scrollWidth - el.clientWidth)));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [text]);
  return (
    <span
      ref={box}
      className="sr-ply-ticker"
      data-motion={moving ? 'on' : 'off'}
      data-overflow={shift > 1}
      style={
        {
          '--ply-shift': `${shift}px`,
          '--ply-dur': `${Math.max(6, shift / 24 + 4)}s`,
        } as React.CSSProperties
      }
      title={text}
    >
      <span className="sr-ply-ticker-text">{text}</span>
    </span>
  );
}

const VU_SEGMENTS = 12;

/** A level bar read from the analyser while playing. Still (all segments off) under reduced motion. */
export function VuBar() {
  const reduced = useReducedMotionPref();
  const playing = usePlayerStore((s) => s.status === 'playing');
  const [lit, setLit] = useState(0);
  const scratch = useRef<Uint8Array | null>(null);
  useEffect(() => {
    if (!playing || reduced) {
      setLit(0);
      return;
    }
    let raf = 0;
    const tick = () => {
      const g = activeGraph();
      if (g) {
        scratch.current ??= new Uint8Array(g.analyser.frequencyBinCount);
        setLit(Math.round(Math.min(1, readLevel(g, scratch.current) * 2.2) * VU_SEGMENTS));
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, reduced]);
  return (
    <span className="sr-ply-vu" aria-hidden="true" data-testid="player-vu">
      {Array.from({ length: VU_SEGMENTS }, (_, i) => (
        <span key={i} data-on={i < lit} data-hot={i >= VU_SEGMENTS - 3} />
      ))}
    </span>
  );
}

/** Play a track in the queue (the queue list and the library slot use it). */
export const playIndex = (i: number) => play(i);
