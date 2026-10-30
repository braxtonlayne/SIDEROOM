/**
 * MUS-SKN-001 / MUS-SKN-002 "Valve & Walnut": a brass and walnut valve hi-fi cabinet, after the owner's tube
 * decoder reference (with a little of the gothic phonograph's parchment and wax).
 *
 * Live: the nameplate ticker; two VU meters (low and high band from the player's analyser); the Nixie position
 * tubes with the format and length strip; the reel window's green LCD (artist, album, track n of total) and its
 * tape packs, which wind from the left reel to the right as the track plays; the brass seek rail and its time
 * scale; the parchment queue ("next on the reel"), each line plays its track; five brass EQ knobs (62 Hz, 250 Hz,
 * 1 kHz, 4 kHz, 16 kHz of the player's ten-band equalizer) and the master gain knob (volume), each a real slider
 * that also turns by dragging; the chrome piano keys (previous, play or pause, stop, next); the toggle bank:
 * tube tone (the equalizer on or off), an A-B loop, shuffle, repeat and mute; skin, dock and close on the mains
 * plate.
 *
 * Painted: the walnut cheeks, the iron panel, screws, the glowing valve (it follows the level), the reels'
 * rotation, the sleeve card (the cover when the track has one), the phones jack and the tape-speed plate.
 */
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent,
  type ReactNode,
} from 'react';
import { useT } from '../../../i18n';
import { EQ_FREQS, EQ_MAX_DB } from '../../../music/eq/presets';
import {
  CloseButton,
  DockButton,
  NextButton,
  PlayPauseButton,
  PrevButton,
  SeekBar,
  SkinButton,
  StopButton,
  Ticker,
  formatTime,
  useNowPlayingText,
} from '../../controls';
import {
  currentTrack,
  cycleRepeat,
  seek,
  setEq,
  setVolume,
  toggleMute,
  usePlayerStore,
} from '../../store';
import { Art, BackdropFrame } from './frame';
import { EqSwitch, QueueButtons, ShuffleButton } from './parts';
import './valve.css';

/** The five bands of the ten-band equalizer the brass knobs turn. */
const KNOB_BANDS = [1, 3, 5, 7, 9] as const;
const hzLabel = (hz: number) => (hz >= 1000 ? `${hz / 1000}K` : String(hz));

/** Four slotted screw heads, one in each corner of the positioned parent. */
function Screws({ slots = [20, 75, 130, 160] }: { slots?: number[] }) {
  return (
    <>
      {['ss', 'se', 'es', 'ee'].map((corner, i) => (
        <span
          key={corner}
          className={`v-screw v-screw-${corner}`}
          aria-hidden="true"
          style={{ '--slot': `${slots[i] ?? 45}deg` } as CSSProperties}
        />
      ))}
    </>
  );
}

// ---- The VU meters ----

/** Classic VU scale: the dB mark and its needle angle (degrees from upright). */
const VU_MARKS: [string, number][] = [
  ['-20', -50],
  ['-10', -31],
  ['-7', -21],
  ['-5', -12],
  ['-3', -2],
  ['0', 22],
  ['+1', 32],
  ['+3', 50],
];
const PX = 100;
const PY = 128;
const polar = (deg: number, r: number) => {
  const a = (deg * Math.PI) / 180;
  return [PX + Math.sin(a) * r, PY - Math.cos(a) * r] as const;
};
const arc = (from: number, to: number, r: number) => {
  const [x1, y1] = polar(from, r);
  const [x2, y2] = polar(to, r);
  return `M${x1.toFixed(2)} ${y1.toFixed(2)} A${r} ${r} 0 0 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
};

function VuMeter({ band, label }: { band: 'low' | 'high'; label: string }) {
  const t = useT();
  return (
    <Art className="v-vu v-bezel" style={{ '--needle': `var(--bd-${band}, 0)` } as CSSProperties}>
      <Screws />
      <div className="v-vu-window">
        <svg viewBox="0 0 200 112" preserveAspectRatio="xMidYMid meet">
          <path d={arc(-50, 22, 84)} className="v-vu-arc" />
          <path d={arc(22, 50, 84)} className="v-vu-red" />
          {Array.from({ length: 21 }, (_, i) => {
            const deg = -50 + i * 5;
            const [x1, y1] = polar(deg, 84);
            const [x2, y2] = polar(deg, 79);
            return (
              <line
                key={i}
                x1={x1}
                y1={y1}
                x2={x2}
                y2={y2}
                className={deg > 22 ? 'v-vu-tick v-vu-tick-red' : 'v-vu-tick'}
              />
            );
          })}
          {VU_MARKS.map(([n, deg]) => {
            const [x1, y1] = polar(deg, 84);
            const [x2, y2] = polar(deg, 92);
            const [tx, ty] = polar(deg, 100);
            return (
              <g key={n} className={deg >= 22 ? 'v-vu-mark v-vu-mark-red' : 'v-vu-mark'}>
                <line x1={x1} y1={y1} x2={x2} y2={y2} />
                <text x={tx} y={ty + 3} textAnchor="middle">
                  {n}
                </text>
              </g>
            );
          })}
          <text x={PX} y="72" textAnchor="middle" className="v-vu-name">
            {t('player.skinx.valve.vu')}
          </text>
          <text x={PX} y="88" textAnchor="middle" className="v-vu-label">
            {label}
          </text>
          <g className="v-vu-needle">
            <line x1={PX} y1={PY} x2={PX} y2={PY - 96} />
          </g>
          <path d="M62 113 A38 20 0 0 1 138 113 Z" className="v-vu-hump" />
        </svg>
      </div>
    </Art>
  );
}

// ---- The valve in its alcove ----

function Valve() {
  const t = useT();
  return (
    <Art className="v-tube">
      <div className="v-alcove">
        <svg viewBox="0 0 64 132" preserveAspectRatio="xMidYMid meet">
          <defs>
            <radialGradient id="v-tube-glow" cx="50%" cy="58%" r="50%">
              <stop offset="0" className="v-glow-core" />
              <stop offset="0.45" className="v-glow-mid" />
              <stop offset="1" className="v-glow-edge" />
            </radialGradient>
            <linearGradient id="v-glass" x1="0" x2="1">
              <stop offset="0" className="v-glass-edge" />
              <stop offset="0.2" className="v-glass-hi" />
              <stop offset="0.36" className="v-glass-clear" />
              <stop offset="0.82" className="v-glass-clear" />
              <stop offset="1" className="v-glass-edge" />
            </linearGradient>
          </defs>
          <ellipse
            cx="32"
            cy="70"
            rx="31"
            ry="48"
            fill="url(#v-tube-glow)"
            className="v-tube-halo"
          />
          {/* Envelope: a dome on a straight glass, the getter flash silvering the top. */}
          <path d="M14 108 V36 Q14 10 32 8 Q50 10 50 36 V108 Z" className="v-tube-env" />
          <path d="M17 30 Q20 13 32 11 Q44 13 47 30 Z" className="v-tube-getter" />
          {/* Inside: mica spacers, the anode plates, the grid and the filament. */}
          <rect x="18" y="38" width="28" height="3" rx="1" className="v-tube-mica" />
          <rect x="18" y="92" width="28" height="3" rx="1" className="v-tube-mica" />
          <rect x="20" y="44" width="9" height="45" rx="1" className="v-tube-plate" />
          <rect x="35" y="44" width="9" height="45" rx="1" className="v-tube-plate" />
          <path d="M25 46 V88 M39 46 V88" className="v-tube-grid" />
          <path d="M30 50 Q32 46 34 50 V86 Q32 90 30 86 Z" className="v-tube-filament" />
          <path d="M32 95 V108 M24 95 V108 M40 95 V108" className="v-tube-leads" />
          <path
            d="M14 108 V36 Q14 10 32 8 Q50 10 50 36 V108 Z"
            fill="url(#v-glass)"
            className="v-tube-shine"
          />
          <rect x="11" y="106" width="42" height="16" rx="3" className="v-tube-base" />
          <path d="M11 111 H53 M11 117 H53" className="v-tube-base-ring" />
          <path d="M20 122 v6 M28 122 v7 M36 122 v7 M44 122 v6" className="v-tube-pins" />
        </svg>
      </div>
      <span className="v-engrave">{t('player.skinx.valve.triode')}</span>
    </Art>
  );
}

// ---- Nixie tubes: the position ----

function Nixie() {
  const t = useT();
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const status = usePlayerStore((s) => s.status);
  const format = usePlayerStore((s) => currentTrack(s)?.format);
  const face = formatTime(position).padStart(5, '0');
  return (
    <div className="v-nixie v-bezel" data-testid="player-state" data-state={status}>
      <Screws slots={[40, 110, 10, 150]} />
      <div className="v-nixie-tubes">
        <span className="sr-ply-bd-sr">
          {t(`player.state.${status}`)}, {t('player.skinx.valve.position')}{' '}
        </span>
        {Array.from(face).map((ch, i) =>
          ch === ':' ? (
            <span key={i} className="v-nixie-colon">
              {ch}
            </span>
          ) : (
            <span key={i} className="v-nixie-tube">
              <span className="v-nixie-digit">{ch}</span>
            </span>
          ),
        )}
      </div>
      <div className="v-nixie-strip">
        {format && <span className="v-nixie-fmt">{format.toUpperCase()}</span>}
        <span>/ {formatTime(duration)}</span>
      </div>
    </div>
  );
}

// ---- The reel-to-reel window ----

function Reel({ cx, pack }: { cx: number; pack: number }) {
  const r = 21 + pack * 24;
  return (
    <g transform={`translate(${cx} 56)`}>
      <circle r="50" className="v-reel-shadow" />
      <circle r={r} className="v-reel-tape" />
      <circle r={r} className="v-reel-tape-sheen" />
      <g className="v-reel-spin">
        <circle r="48" fill="url(#v-flange)" className="v-reel-flange" />
        <circle r="48" className="v-reel-rings" />
        {[0, 120, 240].map((a) => (
          <path
            key={a}
            d="M-11 -19 A22 22 0 0 1 11 -19 L22 -37 A43 43 0 0 0 -22 -37 Z"
            transform={`rotate(${a})`}
            className="v-reel-window"
          />
        ))}
        <circle r="13" className="v-reel-hubring" />
        <circle r="7.5" className="v-reel-hub" />
        {[0, 120, 240].map((a) => (
          <rect
            key={a}
            x="-1.6"
            y="-12.5"
            width="3.2"
            height="5"
            transform={`rotate(${a})`}
            className="v-reel-key"
          />
        ))}
      </g>
    </g>
  );
}

function Deck() {
  const t = useT();
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const track = usePlayerStore((s) => currentTrack(s));
  const n = usePlayerStore((s) => s.index + 1);
  const total = usePlayerStore((s) => s.queue.length);
  const done = duration > 0 ? Math.min(1, position / duration) : 0;
  return (
    <div className="v-deck v-bezel">
      <Screws slots={[60, 15, 100, 140]} />
      <Art className="v-deck-art">
        <svg viewBox="0 0 360 120" preserveAspectRatio="xMidYMid meet">
          <defs>
            <radialGradient id="v-flange" r="50%">
              <stop offset="0.25" className="v-flange-a" />
              <stop offset="0.62" className="v-flange-b" />
              <stop offset="0.9" className="v-flange-c" />
              <stop offset="1" className="v-flange-d" />
            </radialGradient>
          </defs>
          <path d="M58 104 L120 113 H240 L302 104" className="v-tape" />
          <Reel cx={62} pack={1 - done} />
          <Reel cx={298} pack={done} />
          {/* Head block: guide rollers, the chrome head cover and the capstan. */}
          <rect x="146" y="98" width="68" height="20" rx="4" className="v-headcover" />
          <path d="M156 104 h48" className="v-headcover-line" />
          <circle cx="120" cy="110" r="6" className="v-roller" />
          <circle cx="240" cy="110" r="6" className="v-roller" />
          <circle cx="160" cy="110" r="3.4" className="v-roller" />
          <circle cx="200" cy="110" r="3.4" className="v-roller" />
        </svg>
      </Art>
      <div className="v-lcd">
        <span className="v-lcd-row">
          <span className="v-lcd-k">{t('player.skinx.valve.artist')}:</span>{' '}
          <span className="v-lcd-v">{track?.artist ?? '—'}</span>
        </span>
        <span className="v-lcd-row">
          <span className="v-lcd-k">{t('player.skinx.valve.album')}:</span>{' '}
          <span className="v-lcd-v">{track?.album ?? '—'}</span>
        </span>
        <span className="v-lcd-row">
          <span className="v-lcd-k">{t('player.skinx.valve.track')}:</span>{' '}
          <span className="v-lcd-v">
            {total > 0
              ? t('player.skinx.valve.trackOf', { n: String(n).padStart(2, '0'), total })
              : '—'}
          </span>
        </span>
      </div>
    </div>
  );
}

// ---- The seek rail and its scale ----

function Rail() {
  const duration = usePlayerStore((s) => s.duration);
  const position = usePlayerStore((s) => s.position);
  const done = duration > 0 ? Math.min(1, position / duration) : 0;
  const steps = 6;
  return (
    <div className="v-rail-wrap">
      <div className="v-rail" style={{ '--done': done } as CSSProperties}>
        <Art className="v-rail-slot" />
        <SeekBar className="v-seek" />
      </div>
      <Art className="v-scale">
        {Array.from({ length: steps + 1 }, (_, i) => (
          <span key={i}>{formatTime((duration * i) / steps)}</span>
        ))}
      </Art>
    </div>
  );
}

// ---- Brass knobs: real sliders that also turn by dragging ----

function Knob({
  label,
  legend,
  min,
  max,
  step,
  value,
  valueText,
  onValue,
  big = false,
}: {
  label: string;
  legend: string;
  min: number;
  max: number;
  step: number;
  value: number;
  valueText: string;
  onValue: (v: number) => void;
  big?: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
  const drag = useRef<{ x: number; y: number; v: number } | null>(null);
  const frac = (value - min) / (max - min);
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const moved = (e.clientX - d.x - (e.clientY - d.y)) / 140;
    const snapped = Math.round((d.v + moved * (max - min)) / step) * step;
    const next = Math.min(max, Math.max(min, snapped));
    if (next !== value) onValue(next);
  };
  return (
    <div
      className={big ? 'v-knob v-knob-big' : 'v-knob'}
      style={{ '--turn': frac } as CSSProperties}
      onPointerDown={(e) => {
        input.current?.focus();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        drag.current = { x: e.clientX, y: e.clientY, v: value };
        e.preventDefault();
      }}
      onPointerMove={move}
      onPointerUp={() => {
        drag.current = null;
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
    >
      <input
        ref={input}
        type="range"
        className="v-knob-input"
        aria-label={label}
        aria-valuetext={valueText}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onValue(Number(e.target.value))}
      />
      <Art className="v-knob-art">
        <span className="v-knob-ticks" />
        <span className="v-knob-skirt" />
        <span className="v-knob-cap">
          <span className="v-knob-pointer" />
        </span>
      </Art>
      <span className="v-knob-legend" aria-hidden="true">
        {legend}
      </span>
    </div>
  );
}

function Knobs() {
  const t = useT();
  const eq = usePlayerStore((s) => s.eq);
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  const vol = Math.round((muted ? 0 : volume) * 100);
  return (
    <div className="v-knobs" role="group" aria-label={t('player.skinx.valve.knobs')}>
      {KNOB_BANDS.map((b) => {
        const hz = EQ_FREQS[b]!;
        const db = eq[b] ?? 0;
        return (
          <Knob
            key={b}
            label={t('music.lib.eq.band', { hz: `${hzLabel(hz)}Hz` })}
            legend={`${hzLabel(hz)}HZ`}
            min={-EQ_MAX_DB}
            max={EQ_MAX_DB}
            step={1}
            value={db}
            valueText={t('music.lib.eq.db', { db: db > 0 ? `+${db}` : String(db) })}
            onValue={(v) => {
              const next = [...usePlayerStore.getState().eq];
              next[b] = v;
              setEq(next);
            }}
          />
        );
      })}
      <Knob
        big
        label={t('player.volume')}
        legend={t('player.skinx.valve.master')}
        min={0}
        max={100}
        step={1}
        value={vol}
        valueText={`${vol}%`}
        onValue={(v) => setVolume(v / 100)}
      />
    </div>
  );
}

// ---- The toggle bank: two lamps, a chrome bat lever and an engraved legend ----

function Toggle({
  legend,
  state,
  on,
  children,
}: {
  legend: string;
  state: string;
  on: boolean;
  children: ReactNode;
}) {
  return (
    <div className="v-tg" data-on={on}>
      <Art className="v-tg-lamps">
        <span className="v-led v-led-red" />
        <span className="v-led v-led-green" />
      </Art>
      {children}
      <span className="v-tg-legend" aria-hidden="true">
        {legend} <b>{state}</b>
      </span>
    </div>
  );
}
const Lever = () => (
  <span className="v-lever" aria-hidden="true">
    <span className="v-lever-bat" />
  </span>
);

/** A-B loop: the first press marks A, the second marks B and loops, the third stops. Forgotten on a new track. */
function useAbLoop() {
  const [loop, setLoop] = useState<{ a: number | null; b: number | null }>({ a: null, b: null });
  const position = usePlayerStore((s) => s.position);
  const trackId = usePlayerStore((s) => currentTrack(s)?.id);
  useEffect(() => setLoop({ a: null, b: null }), [trackId]);
  useEffect(() => {
    if (loop.a !== null && loop.b !== null && position >= loop.b) seek(loop.a);
  }, [loop, position]);
  const press = () => {
    const at = usePlayerStore.getState().position;
    setLoop((l) =>
      l.a === null
        ? { a: at, b: null }
        : l.b === null
          ? { a: l.a, b: Math.max(at, l.a + 1) }
          : { a: null, b: null },
    );
  };
  return { loop, press };
}

function ToggleBank() {
  const t = useT();
  const eqOn = usePlayerStore((s) => s.eqEnabled);
  const shuffle = usePlayerStore((s) => s.shuffle);
  const repeat = usePlayerStore((s) => s.repeat);
  const muted = usePlayerStore((s) => s.muted);
  const { loop, press } = useAbLoop();
  const onOff = (v: boolean) => t(v ? 'player.skinx.valve.on' : 'player.skinx.valve.off');
  const looping = loop.b !== null;
  return (
    <div className="v-bank">
      <Toggle legend={t('player.skinx.valve.warmth')} state={onOff(eqOn)} on={eqOn}>
        <EqSwitch className="v-lever-btn" label={t('player.skinx.valve.toneLabel')}>
          <Lever />
        </EqSwitch>
      </Toggle>
      <Toggle
        legend={t('player.skinx.valve.loop')}
        state={
          looping
            ? t('player.skinx.valve.loopAB')
            : loop.a !== null
              ? t('player.skinx.valve.loopA')
              : onOff(false)
        }
        on={loop.a !== null}
      >
        <button
          type="button"
          className="v-lever-btn"
          aria-pressed={looping}
          data-on={loop.a !== null}
          aria-label={t(
            loop.a === null
              ? 'player.skinx.valve.loopSetA'
              : looping
                ? 'player.skinx.valve.loopClear'
                : 'player.skinx.valve.loopSetB',
          )}
          onClick={press}
        >
          <Lever />
        </button>
      </Toggle>
      <Toggle legend={t('player.skinx.shuffle')} state={onOff(shuffle)} on={shuffle}>
        <ShuffleButton className="v-lever-btn">
          <Lever />
        </ShuffleButton>
      </Toggle>
      <Toggle
        legend={t('player.skinx.valve.repeat')}
        state={
          repeat === 'one'
            ? t('player.skinx.valve.repeatOne')
            : repeat === 'all'
              ? t('player.skinx.valve.on')
              : onOff(false)
        }
        on={repeat !== 'off'}
      >
        <button
          type="button"
          className="v-lever-btn"
          aria-pressed={repeat !== 'off'}
          data-on={repeat !== 'off'}
          aria-label={t(`player.repeat.${repeat}`)}
          onClick={cycleRepeat}
        >
          <Lever />
        </button>
      </Toggle>
      <Toggle legend={t('player.skinx.valve.mute')} state={onOff(muted)} on={muted}>
        <button
          type="button"
          className="v-lever-btn"
          aria-pressed={muted}
          data-on={muted}
          aria-label={muted ? t('player.unmute') : t('player.mute')}
          onClick={toggleMute}
        >
          <Lever />
        </button>
      </Toggle>
    </div>
  );
}

// ---- Chrome piano keys ----

function Keys() {
  const t = useT();
  const busy = usePlayerStore((s) => s.status === 'playing' || s.status === 'loading');
  return (
    <div className="v-keys-wrap">
      <div className="v-keys" role="group" aria-label={t('player.controls')} data-busy={busy}>
        <PrevButton className="v-key" />
        <PlayPauseButton className="v-key" />
        <StopButton className="v-key" />
        <NextButton className="v-key" />
      </div>
      <Art className="v-keys-legend">
        <span>{t('player.skinx.valve.rew')}</span>
        <span data-lit={busy}>{t('player.skinx.valve.playKey')}</span>
        <span>{t('player.skinx.valve.stopKey')}</span>
        <span>{t('player.skinx.valve.ff')}</span>
      </Art>
    </div>
  );
}

// ---- Parchment queue ----

function Folio() {
  const t = useT();
  const track = usePlayerStore((s) => currentTrack(s));
  const title = track?.title ?? '';
  return (
    <section className="v-folio" aria-labelledby="v-folio-title">
      <Art className="v-folio-art">
        <span className="v-seal" />
      </Art>
      {track && (
        <Art className="v-folio-now">
          <span className="v-dropcap">{title.slice(0, 1)}</span>
          <span className="v-now-title">{title.slice(1)}</span>
          <span className="v-now-by">
            {[track.artist, track.album].filter(Boolean).join(' · ')}
          </span>
        </Art>
      )}
      <h2 id="v-folio-title" className="v-folio-title">
        {t('player.skinx.valve.reel')}
      </h2>
      <QueueButtons className="v-queue" />
    </section>
  );
}

function Sleeve() {
  const t = useT();
  const art = usePlayerStore((s) => currentTrack(s)?.artworkUrl);
  return (
    <Art className="v-sleeve">
      <div className="v-sleeve-card">
        {art ? (
          <img src={art} alt="" className="v-sleeve-img" />
        ) : (
          <svg viewBox="0 0 60 112" className="v-sleeve-etch" preserveAspectRatio="xMidYMax meet">
            {/* A record half out of its sleeve; the sleeve carries an etched gramophone. */}
            <circle cx="30" cy="27" r="24" className="v-disc" />
            <circle cx="30" cy="27" r="20" className="v-disc-groove" />
            <circle cx="30" cy="27" r="15" className="v-disc-groove" />
            <circle cx="30" cy="27" r="7.5" className="v-disc-label" />
            <circle cx="30" cy="27" r="1.2" className="v-disc-hole" />
            <rect x="2" y="36" width="56" height="76" rx="1.5" className="v-sleeve-paper" />
            <rect x="5" y="39" width="50" height="70" className="v-sleeve-rule" />
            <rect x="7" y="41" width="46" height="66" className="v-sleeve-rule" />
            <g className="v-sleeve-ink">
              <ellipse cx="26" cy="92" rx="15" ry="3.6" />
              <ellipse cx="26" cy="91.4" rx="9" ry="2" />
              <path d="M26 90 V82 Q26 77 30 75 L39 57 Q43 54 48 59 Q46 65 40 67 L33 77" />
              <path d="M12 99 h28 M16 102 h20" />
              <path d="M39 57 Q37 52 42 51 Q48 52 48 59" />
            </g>
          </svg>
        )}
      </div>
      <span className="v-engrave">{t('player.skinx.valve.sleeve')}</span>
    </Art>
  );
}

function Mains() {
  const t = useT();
  return (
    <div className="v-mains v-brass">
      <Screws slots={[30, 120, 70, 10]} />
      <Art className="v-mains-lamp">
        <span className="v-pilot" />
        <span className="v-engrave-ink">{t('player.skinx.valve.mains')}</span>
      </Art>
      <span className="v-mains-btns">
        <SkinButton className="v-cap" />
        <DockButton className="v-cap" />
        <CloseButton className="v-cap" />
      </span>
    </div>
  );
}

/** The iron panel and the filters for its noise and the walnut's grain, drawn once behind everything. */
function CabinetArt() {
  return (
    <svg className="v-cabinet">
      <defs>
        <filter id="v-noise" x="0" y="0" width="100%" height="100%">
          <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="3" seed="7" />
          <feColorMatrix type="saturate" values="0" />
        </filter>
        <filter id="v-grain" x="0" y="0" width="100%" height="100%">
          <feTurbulence
            type="fractalNoise"
            baseFrequency="0.22 0.012"
            numOctaves="3"
            seed="3"
            result="n"
          />
          <feColorMatrix
            in="n"
            type="matrix"
            values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  3 0 0 0 -1.3"
            result="m"
          />
          <feComposite in="SourceGraphic" in2="m" operator="in" />
        </filter>
        <filter id="v-burl" x="0" y="0" width="100%" height="100%">
          <feTurbulence
            type="turbulence"
            baseFrequency="0.05 0.035"
            numOctaves="4"
            seed="11"
            result="n"
          />
          <feColorMatrix
            in="n"
            type="matrix"
            values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  3.2 0 0 0 -0.55"
            result="m"
          />
          <feComposite in="SourceGraphic" in2="m" operator="in" />
        </filter>
      </defs>
      <rect width="100%" height="100%" className="v-iron" />
      <rect width="100%" height="100%" filter="url(#v-noise)" className="v-iron-noise" />
    </svg>
  );
}

function Cheek({ side }: { side: 'start' | 'end' }) {
  return (
    <Art className={`v-cheek v-cheek-${side}`}>
      <svg preserveAspectRatio="none">
        <rect width="100%" height="100%" className="v-wood" />
        <rect width="100%" height="100%" filter="url(#v-burl)" className="v-wood-burl" />
        <rect width="100%" height="100%" filter="url(#v-grain)" className="v-wood-grain" />
      </svg>
    </Art>
  );
}

export function ValveSkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <BackdropFrame skin="valve" scheme="dark" art={<CabinetArt />}>
      <Cheek side="start" />
      <Cheek side="end" />
      <div className="v-body">
        <div className="v-plate v-brass">
          <Screws slots={[20, 140, 80, 50]} />
          <span className="v-plate-brand" aria-hidden="true">
            {t('player.skins.valve')}
          </span>
          <span className="v-plate-sep" aria-hidden="true" />
          <Ticker text={text} />
        </div>
        <Mains />
        <Valve />
        <div className="v-vus">
          <VuMeter band="low" label={t('player.skinx.valve.low')} />
          <VuMeter band="high" label={t('player.skinx.valve.high')} />
        </div>
        <Nixie />
        <Sleeve />
        <Deck />
        <Rail />
        <Folio />
        <Knobs />
        <Keys />
        <ToggleBank />
        <Art className="v-jackplate">
          <span className="v-jack" />
          <span className="v-engrave">{t('player.skinx.valve.phones')}</span>
          <span className="v-speed">{t('player.skinx.valve.speed')}</span>
        </Art>
      </div>
    </BackdropFrame>
  );
}

export default ValveSkin;
