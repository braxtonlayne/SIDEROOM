/**
 * MUS-SKN-002 "Pastel Study Deck": a soft clay-and-cream study desk window (after reference 19, with a little of
 * the retro desktop window and the pixel handheld). Painted: the pink stitched shell, the turntable on its mint
 * pad (the record turns and the tone arm swings in while a track plays), the pixel sleeping cat on the little
 * screen, the frosting on the seek ribbon and its bunny, the tomato, the notebook spirals and stickers, and the
 * ambience mixer (it makes no sound: there are no ambience sound files yet, and the card says so).
 * Live: the ticker, the time and a rainbow level meter on the screen, the seek ribbon, the round transport keys,
 * the focus timer and the notes pad (through `SkinExtras`: wave 32's shared tools once bound, a working
 * in-memory default until then), the queue, shuffle, repeat, volume, skin, dock and close.
 */
import { useId, type CSSProperties, type ReactElement, type ReactNode } from 'react';
import { useT } from '../../../i18n';
import {
  CloseButton,
  DockButton,
  formatTime,
  isBusy,
  NextButton,
  PlayPauseButton,
  PrevButton,
  RepeatButton,
  SeekBar,
  SkinButton,
  StopButton,
  Ticker,
  TimeReadout,
  VolumeControl,
  useNowPlayingText,
} from '../../controls';
import { usePlayerStore } from '../../store';
import { useSkinExtras, type SkinExtras } from './extras';
import { Art, BackdropFrame, LedMeter } from './frame';
import { QueueButtons, ShuffleButton, useQueueCount } from './parts';
import './study.css';

// ---- Painted pieces ----

/** A little cloud with a face (the window's mascot) and a tea leaf. */
function TitleArt() {
  return (
    <Art className="st-title-art">
      <svg viewBox="0 0 46 22">
        <path
          d="M7 20 a5.5 5.5 0 0 1 0 -11 a7.5 7.5 0 0 1 13 -4 a6.5 6.5 0 0 1 10.5 4.5 a5.2 5.2 0 0 1 0 10.5 z"
          className="st-cloud-fill"
        />
        <circle cx="13" cy="13.5" r="1.2" className="st-face-eye" />
        <circle cx="22" cy="13.5" r="1.2" className="st-face-eye" />
        <path d="M16 15.2 q1.5 1.4 3 0" className="st-face-mouth" />
        <ellipse cx="10.4" cy="16.2" rx="1.7" ry="1" className="st-face-blush" />
        <ellipse cx="24.6" cy="16.2" rx="1.7" ry="1" className="st-face-blush" />
        <path d="M34 19 q1 -11 11 -14 q-1 11 -11 14 z" className="st-leaf" />
        <path d="M34.6 18.4 q3.4 -5.6 9 -12" className="st-leaf-vein" />
      </svg>
    </Art>
  );
}

/**
 * The turntable: a pink platter with a strawberry label and a tone arm, on a mint pad with three knobs (the pad
 * and knobs are CSS, so the pad can stretch to the height of its row).
 */
function Turntable() {
  const t = useT();
  const id = useId();
  const g = (n: string) => `${id}-${n}`;
  return (
    <>
      <Art className="st-deck-art">
        <span className="st-knob st-knob-tl" />
        <span className="st-knob st-knob-br" />
        <span className="st-knob-lilac" />
        <span className="st-rpm">{t('player.skinx.study.rpm')}</span>
      </Art>
      <Art className="st-turntable">
        <svg viewBox="10 4 142 134">
          <defs>
            <radialGradient id={g('vinyl')} cx="0.42" cy="0.38" r="0.7">
              <stop offset="0" className="st-stop-pink-hi" />
              <stop offset="0.75" className="st-stop-pink" />
              <stop offset="1" className="st-stop-pink-lo" />
            </radialGradient>
          </defs>
          {/* Platter shadow and the record. */}
          <circle cx="73" cy="77" r="57" className="st-platter-shadow" />
          <circle cx="70" cy="73" r="57" className="st-platter" />
          <g className="st-record">
            <circle cx="70" cy="73" r="53" fill={`url(#${g('vinyl')})`} />
            <circle cx="70" cy="73" r="52.2" className="st-vinyl-rim" />
            {[48, 45.5, 43, 40, 37, 34, 31, 28, 25].map((r) => (
              <circle key={r} cx="70" cy="73" r={r} className="st-groove" />
            ))}
            <circle cx="70" cy="73" r="20" className="st-label" />
            <circle cx="70" cy="73" r="17" className="st-label-stitch" />
            {/* The strawberry, with a sleepy smile, sitting below the spindle. */}
            <g transform="translate(0 5.5)">
              <path
                d="M70 64 c7 -1 12 3 11 9 c-1 7 -7 11 -11 13 c-4 -2 -10 -6 -11 -13 c-1 -6 4 -10 11 -9 z"
                className="st-berry"
              />
              <path
                d="M63 64.5 l3.5 1 l3.5 -3.5 l3.5 3.5 l3.5 -1 l-2.5 3.5 h-9 z"
                className="st-berry-leaf"
              />
              <circle cx="66" cy="73.5" r="1" className="st-face-eye" />
              <circle cx="74" cy="73.5" r="1" className="st-face-eye" />
              <path d="M68.4 76.2 q1.6 1.4 3.2 0" className="st-face-mouth" />
              <ellipse cx="63.6" cy="76.4" rx="1.6" ry="0.9" className="st-face-blush" />
              <ellipse cx="76.4" cy="76.4" rx="1.6" ry="0.9" className="st-face-blush" />
              {[
                [65, 81],
                [75, 81],
                [70, 83.5],
                [62.5, 70],
                [77.5, 70],
              ].map(([x, y]) => (
                <ellipse
                  key={`${x}-${y}`}
                  cx={x}
                  cy={y}
                  rx="0.6"
                  ry="0.9"
                  className="st-berry-seed"
                />
              ))}
            </g>
          </g>
          {/* Light on the vinyl stays put while the record turns under it. */}
          <path d="M33 52 a45 45 0 0 1 30 -23 l2 9 a36 36 0 0 0 -24 18 z" className="st-sheen" />
          <path d="M107 94 a45 45 0 0 1 -30 23 l-2 -9 a36 36 0 0 0 24 -18 z" className="st-sheen" />
          <circle cx="70" cy="73" r="3" className="st-spindle" />
          <circle cx="69.2" cy="72.2" r="1" className="st-spindle-hi" />
          {/* Tone arm: rests beside the record, swings in while a track plays. */}
          <g className="st-arm">
            <path d="M138 22 C 142 52 136 80 114 101" className="st-arm-tube-shadow" />
            <path d="M136 20 C 140 50 134 78 112 99" className="st-arm-tube" />
            <path d="M135 22 C 139 50 133 77 112 97" className="st-arm-tube-hi" />
            <rect
              x="104"
              y="94"
              width="13"
              height="18"
              rx="3.5"
              className="st-headshell"
              transform="rotate(44 110 103)"
            />
            <circle cx="136" cy="20" r="12" className="st-arm-base" />
            <circle cx="136" cy="20" r="7" className="st-arm-ring" />
            <circle cx="136" cy="20" r="2.6" className="st-spindle" />
          </g>
        </svg>
      </Art>
    </>
  );
}

/** A pixel picture, one string per row, one character per cell (`.` is empty). Runs of a colour share a rect. */
const CAT_ROWS = [
  '',
  '..o.......o',
  '.opo.....opo',
  '.oppo...oppo',
  '.occcoooooccco',
  'occcccccccccco....oooooooo',
  'occcccccccccccoooocccsccsccoo',
  'occcccccccccccccccccsccscccccoo',
  'ocoooccccoooccccccccccccccccccco',
  'occcccccccccccccccscccsccccccccco',
  'olllllpplllllocccccccccccccccccco',
  '.olllllllllllocccccccccccclllllco',
  '..ollllllllloccccccccccclllllllo',
];
const PIXEL_CLASS: Record<string, string> = {
  o: 'st-px-o',
  c: 'st-px-c',
  s: 'st-px-s',
  l: 'st-px-l',
  p: 'st-px-p',
};

function PixelRuns({ rows }: { rows: string[] }) {
  const rects: ReactElement[] = [];
  rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      const ch = row[x]!;
      let end = x + 1;
      while (end < row.length && row[end] === ch) end++;
      const cls = PIXEL_CLASS[ch];
      if (cls)
        rects.push(
          <rect key={`${y}-${x}`} x={x} y={y} width={end - x} height={1} className={cls} />,
        );
      x = end;
    }
  });
  return <>{rects}</>;
}

/** One pixel book: an outline, a cover colour, a pale band and the page edges. */
function PixelBook({ x, y, w, tone }: { x: number; y: number; w: number; tone: string }) {
  return (
    <>
      <rect x={x} y={y} width={w} height={4} className="st-px-o" />
      <rect x={x + 1} y={y + 1} width={w - 6} height={2} className={`st-px-${tone}`} />
      <rect x={x + 4} y={y + 2} width={w - 12} height={1} className="st-px-band" />
      <rect x={x + w - 5} y={y + 1} width={4} height={2} className="st-px-page" />
    </>
  );
}

/** The sleeping cat on a pile of books with a cup of tea, drawn on the pixel screen. Its "z"s drift while playing. */
function PixelCat() {
  return (
    <Art className="st-cat">
      <svg viewBox="-1 -4 48 27" shapeRendering="crispEdges">
        <PixelRuns rows={CAT_ROWS} />
        <PixelBook x={1} y={13} w={33} tone="g" />
        <PixelBook x={0} y={16} w={37} tone="r" />
        <PixelBook x={2} y={19} w={34} tone="u" />
        {/* The cup. */}
        <rect x={38} y={16} width={7} height={7} className="st-px-o" />
        <rect x={39} y={17} width={5} height={5} className="st-px-m" />
        <rect x={40} y={18} width={1} height={1} className="st-px-p" />
        <rect x={42} y={18} width={1} height={1} className="st-px-p" />
        <rect x={40} y={19} width={3} height={1} className="st-px-p" />
        <rect x={41} y={20} width={1} height={1} className="st-px-p" />
        <rect x={45} y={17} width={1} height={1} className="st-px-o" />
        <rect x={46} y={18} width={1} height={3} className="st-px-o" />
        <rect x={45} y={21} width={1} height={1} className="st-px-o" />
        <g className="st-steam">
          <rect x={40} y={13} width={1} height={2} className="st-px-steam" />
          <rect x={41} y={11} width={1} height={2} className="st-px-steam" />
          <rect x={40} y={9} width={1} height={2} className="st-px-steam" />
          <rect x={43} y={14} width={1} height={1} className="st-px-steam" />
          <rect x={44} y={12} width={1} height={2} className="st-px-steam" />
        </g>
        <g className="st-z st-z1">
          <path
            d="M15 0 h4 v1 h-1 v1 h-1 v1 h-1 v1 h3 v1 h-4 v-1 h1 v-1 h1 v-1 h1 v-1 h-3 z"
            className="st-px-z"
          />
        </g>
        <g className="st-z st-z2">
          <path
            d="M21 -4 h3 v1 h-1 v1 h-1 v1 h2 v1 h-3 v-1 h1 v-1 h1 v-1 h-2 z"
            className="st-px-z"
          />
        </g>
      </svg>
    </Art>
  );
}

/**
 * The ribbon: a pink tube with cream frosting up to the play position and a bunny riding it; the real seek
 * slider lies on top with an invisible track, so the bunny is what the member drags.
 */
function SeekRibbon() {
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const pct = duration > 0 ? Math.min(1, Math.max(0, position / duration)) : 0;
  const wave = `${useId()}-wave`;
  return (
    <div className="st-ribbon" style={{ '--st-pct': pct.toFixed(4) } as CSSProperties}>
      <div className="st-ribbon-rail">
        <Art className="st-ribbon-art">
          <span className="st-tube">
            <span className="st-frosting">
              <svg preserveAspectRatio="none">
                <defs>
                  <pattern id={wave} width="14" height="18" patternUnits="userSpaceOnUse">
                    <path d="M-7 9 q3.5 -6 7 0 t7 0 t7 0 t7 0" className="st-wave" />
                  </pattern>
                </defs>
                <rect width="100%" height="100%" fill={`url(#${wave})`} />
              </svg>
            </span>
          </span>
          <span className="st-bunny">
            <svg viewBox="0 0 30 32">
              <ellipse cx="10.5" cy="8" rx="3.2" ry="7.5" className="st-bunny-ear" />
              <ellipse cx="19.5" cy="8" rx="3.2" ry="7.5" className="st-bunny-ear" />
              <ellipse cx="10.5" cy="8.5" rx="1.4" ry="5" className="st-bunny-ear-in" />
              <ellipse cx="19.5" cy="8.5" rx="1.4" ry="5" className="st-bunny-ear-in" />
              <ellipse cx="15" cy="21.5" rx="11.5" ry="9" className="st-bunny-head" />
              <circle cx="11" cy="21" r="1.2" className="st-face-eye" />
              <circle cx="19" cy="21" r="1.2" className="st-face-eye" />
              <path d="M13.6 23.4 q1.4 1.2 2.8 0" className="st-face-mouth" />
              <ellipse cx="8" cy="24" rx="2" ry="1.1" className="st-face-blush" />
              <ellipse cx="22" cy="24" rx="2" ry="1.1" className="st-face-blush" />
            </svg>
          </span>
        </Art>
        <SeekBar className="st-seek" />
      </div>
      <Art className="st-ribbon-times">
        <span>{formatTime(position)}</span>
        <span className="st-ribbon-rule" />
        <span>-{formatTime(Math.max(0, duration - position))}</span>
      </Art>
    </div>
  );
}

/** A round clay key with its painted caption. */
function Key({ caption, tone, children }: { caption: string; tone: string; children: ReactNode }) {
  return (
    <span className={`st-key st-key-${tone}`}>
      {children}
      <Art className="st-key-cap">{caption}</Art>
    </span>
  );
}

function Transport() {
  const t = useT();
  const busy = usePlayerStore((s) => isBusy(s.status));
  return (
    <div className="st-keys" role="group" aria-label={t('player.controls')}>
      <Key caption={t('player.skinx.study.capPrev')} tone="lilac">
        <PrevButton className="st-key-btn" />
      </Key>
      <Key
        caption={busy ? t('player.skinx.study.capPause') : t('player.skinx.study.capPlay')}
        tone="mint"
      >
        <PlayPauseButton className="st-key-btn" />
      </Key>
      <Key caption={t('player.skinx.study.capStop')} tone="peach">
        <StopButton className="st-key-btn" />
      </Key>
      <Key caption={t('player.skinx.study.capNext')} tone="sky">
        <NextButton className="st-key-btn" />
      </Key>
    </div>
  );
}

function SlotThumb({ kind }: { kind: 'rain' | 'crackle' | 'cafe' }) {
  if (kind === 'rain')
    return (
      <svg viewBox="0 0 30 20">
        <path
          d="M7 18 a5.5 5.5 0 0 1 0 -11 a7 7 0 0 1 13 -2.5 a6 6 0 0 1 3.5 13.5 z"
          className="st-cloud-fill"
        />
        <circle cx="12" cy="12.5" r="1" className="st-face-eye" />
        <circle cx="18" cy="12.5" r="1" className="st-face-eye" />
      </svg>
    );
  if (kind === 'crackle')
    return (
      <svg viewBox="0 0 28 28">
        <circle cx="14" cy="14" r="12.5" className="st-thumb-vinyl" />
        <circle cx="14" cy="14" r="9" className="st-thumb-groove" />
        <circle cx="14" cy="14" r="4.8" className="st-thumb-label" />
        <circle cx="14" cy="14" r="1.3" className="st-spindle" />
      </svg>
    );
  return (
    <svg viewBox="0 0 30 24">
      <ellipse cx="14" cy="21" rx="12.5" ry="2.8" className="st-thumb-saucer" />
      <path d="M5 7 h18 v5 a9 8 0 0 1 -18 0 z" className="st-thumb-cup" />
      <path d="M23 9 a3.5 3.5 0 0 1 0 7" className="st-thumb-handle" />
      <ellipse cx="14" cy="7.2" rx="9" ry="1.8" className="st-thumb-tea" />
    </svg>
  );
}

/** The cosy ambience mixer. It is painted only: there are no ambience sound files yet, and the card says so. */
function AmbienceMixer() {
  const t = useT();
  const id = useId();
  const slots = [
    ['rain', t('player.skinx.study.rain'), 0.65],
    ['crackle', t('player.skinx.study.crackle'), 0.4],
    ['cafe', t('player.skinx.study.cafe'), 0.15],
  ] as const;
  return (
    <section
      className="st-card st-mixer"
      aria-labelledby={`${id}-h`}
      aria-describedby={`${id}-d`}
      title={t('player.skinx.study.mixerSoon')}
    >
      <h2 id={`${id}-h`} className="st-card-h st-mixer-h">
        <Art className="st-rain-icon">
          <svg viewBox="0 0 24 24">
            <path
              d="M6 14 a4.5 4.5 0 0 1 0 -9 a6 6 0 0 1 11 -1 a4.8 4.8 0 0 1 1.5 10 z"
              className="st-cloud-fill"
            />
            <path d="M8 17.5 l-1 3 M12.5 17.5 l-1 3 M17 17.5 l-1 3" className="st-rain-drop" />
          </svg>
        </Art>
        {t('player.skinx.study.mixer')}
      </h2>
      <Art className="st-mixer-slots">
        {slots.map(([kind, label, v]) => (
          <span
            key={kind}
            className={`st-slot st-slot-${kind}`}
            style={{ '--v': v } as CSSProperties}
          >
            <span className="st-slot-track">
              <span className="st-slot-groove" />
              <span className="st-slot-thumb">
                <SlotThumb kind={kind} />
              </span>
            </span>
            <span className="st-slot-label">{label}</span>
            <span className="st-slot-val">
              {t('player.skinx.study.pct', { n: Math.round(v * 100) })}
            </span>
          </span>
        ))}
      </Art>
      <p id={`${id}-d`} className="st-mixer-note">
        {t('player.skinx.study.mixerNote')}
      </p>
    </section>
  );
}

// ---- Live extras (bound to wave 32's tools by the integrator) ----

function Tomato() {
  return (
    <Art className="st-tomato">
      <svg viewBox="0 0 28 26">
        <ellipse cx="14" cy="16" rx="12" ry="9.5" className="st-tomato-body" />
        <ellipse cx="9.5" cy="12.8" rx="3.6" ry="2" className="st-tomato-shine" />
        <path
          d="M14 7 l-5 -2 l3 3 l-5 1 l5 1 l2 2 l2 -2 l5 -1 l-5 -1 l3 -3 z"
          className="st-tomato-leaf"
        />
        <path d="M14 7 q1 -4 3 -5" className="st-tomato-stem" />
      </svg>
    </Art>
  );
}

function FocusTimer({ extras }: { extras: SkinExtras }) {
  const t = useT();
  const id = useId();
  const timer = extras.useFocusTimer();
  const r = 15;
  const c = 2 * Math.PI * r;
  return (
    <section
      className="st-card st-timer"
      aria-labelledby="st-timer-h"
      data-segment={timer.segment}
      data-running={timer.running}
    >
      <div className="st-timer-head">
        <Tomato />
        <h2 id="st-timer-h" className="st-card-h">
          {t('player.skinx.study.timer')}
        </h2>
      </div>
      <div className="st-timer-face">
        <div className="st-timer-read">
          <span className="st-timer-clock" role="timer" aria-live="off">
            {timer.clock}
          </span>
          <p className="st-timer-seg">
            {timer.segment === 'focus'
              ? t('player.skinx.study.focus')
              : t('player.skinx.study.break')}
          </p>
        </div>
        <Art className="st-ring">
          <svg viewBox="0 0 40 40">
            <defs>
              <linearGradient id={`${id}-ring`} x1="0" y1="0" x2="1" y2="1">
                <stop offset="0" className="st-stop-butter" />
                <stop offset="1" className="st-stop-pink-lo" />
              </linearGradient>
            </defs>
            <circle cx="20" cy="20" r="19.5" className="st-ring-plate" />
            <circle cx="20" cy="20" r="18.6" className="st-ring-ticks" pathLength={60} />
            <circle cx="20" cy="20" r={r} className="st-ring-bg" />
            <circle
              cx="20"
              cy="20"
              r={r}
              className="st-ring-fg"
              opacity={timer.progress > 0.005 ? 1 : 0}
              stroke={`url(#${id}-ring)`}
              strokeDasharray={`${c * timer.progress} ${c}`}
              transform="rotate(-90 20 20)"
            />
            <circle cx="20" cy="20" r="10.5" className="st-ring-hub" />
          </svg>
        </Art>
      </div>
      <div className="st-timer-btns">
        <button type="button" className="st-chip st-chip-main" onClick={timer.toggle}>
          {timer.running ? t('player.skinx.study.pause') : t('player.skinx.study.start')}
        </button>
        <button type="button" className="st-chip" onClick={timer.skip}>
          {timer.segment === 'focus'
            ? t('player.skinx.study.toBreak')
            : t('player.skinx.study.toFocus')}
        </button>
        <button
          type="button"
          className="st-chip st-chip-round"
          aria-label={t('player.skinx.study.reset')}
          title={t('player.skinx.study.reset')}
          onClick={timer.reset}
        >
          <svg viewBox="0 0 24 24" width="1em" height="1em" aria-hidden="true">
            <path d="M5.5 12 a6.5 6.5 0 1 0 2 -4.7 M5 4.5 v3.8 h3.8" className="st-reset-glyph" />
          </svg>
        </button>
      </div>
    </section>
  );
}

function Notes({ extras }: { extras: SkinExtras }) {
  const t = useT();
  const notes = extras.useNotes();
  const id = useId();
  if (notes.status === 'unavailable') return null;
  return (
    <section className="st-card st-notes" aria-labelledby={`${id}-h`}>
      <Art className="st-rings">
        {Array.from({ length: 14 }, (_, i) => (
          <i key={i} />
        ))}
      </Art>
      <h2 id={`${id}-h`} className="st-card-h st-notes-h">
        <label htmlFor={`${id}-t`}>{t('player.skinx.study.notes')}</label>
      </h2>
      <textarea
        id={`${id}-t`}
        className="st-notes-text"
        rows={3}
        value={notes.text}
        disabled={notes.status !== 'ready'}
        placeholder={t('player.skinx.study.notesHint')}
        aria-describedby={`${id}-d`}
        onChange={(e) => notes.setText(e.target.value)}
      />
      <p id={`${id}-d`} className="st-notes-where">
        {notes.persistent ? t('player.skinx.study.saved') : t('player.skinx.study.tabOnly')}
      </p>
      <Art className="st-stickers">
        <svg viewBox="0 0 60 24" className="st-pencil">
          <path d="M6 19 l36 -12 l4 5 l-36 12 z" className="st-pencil-body" />
          <path d="M42 7 l5 -1.7 l3 5 l-4 1.7 z" className="st-pencil-eraser" />
          <path d="M6 19 l4 5 l-8 1 z" className="st-pencil-tip" />
          <path d="M8 20.3 l36 -12" className="st-pencil-line" />
        </svg>
        <svg viewBox="0 0 24 22" className="st-heart">
          <path
            d="M12 20 C 2 13 1 7 5 4 C 8 2 11 4 12 6 C 13 4 16 2 19 4 C 23 7 22 13 12 20 z"
            className="st-heart-body"
          />
          <ellipse cx="7.5" cy="7.5" rx="2" ry="1.2" className="st-heart-shine" />
        </svg>
      </Art>
    </section>
  );
}

/**
 * The extras, keyed by the registration version so a new binding remounts their hooks cleanly. The timer and
 * the notes sit in different columns of the desk, so each reads the bound extras itself (same hooks, same keys).
 */
function TimerExtra() {
  const { extras, version } = useSkinExtras();
  return <FocusTimer key={`t${version}`} extras={extras} />;
}

function NotesExtra() {
  const { extras, version } = useSkinExtras();
  return <Notes key={`n${version}`} extras={extras} />;
}

function QueueCard() {
  const t = useT();
  const { total } = useQueueCount();
  return (
    <section className="st-card st-queue-card" aria-labelledby="st-queue-h">
      <h2 id="st-queue-h" className="st-card-h st-queue-h">
        <Art className="st-queue-leaf">
          <svg viewBox="0 0 20 20">
            <path d="M3 17 q1 -12 14 -14 q-1 13 -14 14 z" className="st-leaf" />
            <path d="M4 16 q4 -6 11 -11" className="st-leaf-vein" />
          </svg>
        </Art>
        <span className="st-queue-title">{t('player.queue.title')}</span>
        <span className="st-queue-count">{t('player.skinx.study.tracks', { n: total })}</span>
      </h2>
      <QueueButtons className="st-queue" />
      <div className="st-queue-foot">
        <span className="st-tile">
          <ShuffleButton className="st-tile-btn" />
          <Art className="st-tile-cap">{t('player.skinx.study.capShuffle')}</Art>
        </span>
        <span className="st-tile">
          <RepeatButton className="st-tile-btn" />
          <Art className="st-tile-cap">{t('player.skinx.study.capLoop')}</Art>
        </span>
        <span className="st-tile st-tile-vol">
          <VolumeControl className="st-tile-btn" />
        </span>
      </div>
    </section>
  );
}

export function StudySkin() {
  const t = useT();
  const text = useNowPlayingText();
  const { n, total } = useQueueCount();
  return (
    <BackdropFrame
      skin="study"
      scheme="light"
      art={
        <>
          <span className="st-dots" />
          <span className="st-grain" />
        </>
      }
    >
      <div className="st-title">
        <TitleArt />
        <span className="st-title-text">{t('player.skins.study')}</span>
        <Art className="st-badge">
          <svg viewBox="0 0 24 22" className="st-badge-mug">
            <path d="M3 8 h13 v6 a6.5 5.5 0 0 1 -13 0 z" className="st-thumb-cup" />
            <path d="M16 9.5 a3.2 3.2 0 0 1 0 6.4" className="st-thumb-handle" />
            <path d="M7.5 6 q-1.5 -2 0 -4.5 M11.5 6 q-1.5 -2 0 -4.5" className="st-badge-steam" />
          </svg>
          <span>{t('player.skinx.study.badge')}</span>
        </Art>
        <span className="st-win">
          <SkinButton className="st-dot st-dot-a" />
          <DockButton className="st-dot st-dot-b" />
          <CloseButton className="st-dot st-dot-c" />
        </span>
      </div>
      <div className="st-body">
        <div className="st-col st-col-a">
          <div className="st-deck">
            <Turntable />
          </div>
          <AmbienceMixer />
        </div>
        <div className="st-col st-col-b">
          <div className="st-screen">
            <div className="st-lcd">
              <PixelCat />
              <div className="st-lcd-text">
                <span className="st-now">
                  <Art className="st-note-glyph">♪</Art>
                  <Ticker text={text} />
                </span>
                <span className="st-sub">
                  {total > 0 ? t('player.skinx.study.trackOf', { n, total }) : ''}
                </span>
                <TimeReadout />
                <LedMeter className="st-leds" segments={10} />
              </div>
            </div>
          </div>
          <div className="st-play">
            <SeekRibbon />
            <Transport />
          </div>
          <NotesExtra />
        </div>
        <div className="st-col st-col-c">
          <TimerExtra />
          <QueueCard />
        </div>
      </div>
    </BackdropFrame>
  );
}

export default StudySkin;
