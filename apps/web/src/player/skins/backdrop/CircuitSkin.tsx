/**
 * MUS-SKN-001 / MUS-SKN-002 "Night Circuit": a black, angular neon DJ console. Painted (aria-hidden, inert): the
 * chamfered console body with its hazard trim, the system title and DSP readouts, the cover frame and its city
 * illustration (the real cover when the track has one), the waveform, the radar scope (its sweep turns while a
 * track plays), the frequency scale, the four offline pads and the hazard plates.
 *
 * Live: skin, dock and close in the title bar; the ticker and the big clock; the waveform is the seek bar (its
 * playhead is the range thumb); a real spectrum and two master meters from the player's analyser; four cue pads
 * (0, 25, 50 and 75 percent); SKIP / EXECUTE PLAY / HALT / SKIP; the gain fader (volume) and mute; three tone
 * knobs that set the equalizer's low, mid and high bands, a crossfade knob and the EQ switch; repeat, shuffle and
 * library; and the queue table, where each row plays its track.
 */
import { useMemo, type CSSProperties, type ReactNode } from 'react';
import { useT } from '../../../i18n';
import { EQ_FREQS, EQ_MAX_DB } from '../../../music/eq/presets';
import {
  CloseButton,
  DockButton,
  IconNext,
  IconPause,
  IconPlay,
  IconPrev,
  IconStop,
  IconVolume,
  SkinButton,
  Ticker,
  formatTime,
  isBusy,
  playIndex,
  useNowPlayingText,
} from '../../controls';
import {
  currentTrack,
  cycleRepeat,
  next,
  pause,
  prev,
  seek,
  setCrossfade,
  setEq,
  setEqEnabled,
  setVolume,
  toggle,
  toggleMute,
  toggleShuffle,
  usePlayerStore,
} from '../../store';
import { CROSSFADE_MAX_SEC } from '../../types';
import { Art, BackdropFrame, LedMeter, SpectrumBars } from './frame';
import { EqSwitch, LibraryButton } from './parts';
import './circuit.css';

/* Painted readouts: made-up console jargon printed on the artwork, the way the old skins did. */
const PAINT = {
  sys: '// NC_AUDIO_OS',
  ver: ' v3.41',
  dsp: 'DSP',
  readouts: ['BUF 128 SPL', 'LAT 2.4MS', 'OUT HW_64'],
  phase: 'PHASE CORR +0.94 [MONO_OK]',
  left: 'MASTER L',
  right: 'MASTER R',
  limiter: '[LIMITER: ARMED]',
  pads: 'CUE_MATRIX // A',
};
const SCALE = [
  '20HZ',
  '50HZ',
  '100HZ',
  '250HZ',
  '500HZ',
  '1KHZ',
  '2.5KHZ',
  '5KHZ',
  '10KHZ',
  '20KHZ',
];
const PEAKS = ['-3', '-6', '-12', '-18', '-24'];
const DB = ['-48', '-24', '-12', '-6', '-3', '0'];
const OFFLINE_PADS = [
  ['LP', '1/2'],
  ['LP', '1/4'],
  ['STUTR', ''],
  ['REVRS', ''],
];

type Tone = 'cyan' | 'red' | 'yellow' | 'steel';

/** A console panel: a chamfered neon outline (painted) around live content. */
function Panel({
  className,
  tone = 'steel',
  children,
}: {
  className: string;
  tone?: Tone;
  children?: ReactNode;
}) {
  return (
    <div className={`nc-panel ${className}`} data-tone={tone}>
      <Art className="nc-edge">
        <i />
      </Art>
      {children}
    </div>
  );
}

/** Two-digit minutes, seconds and hundredths: 02:48.94. */
function clockFace(sec: number): { main: string; frac: string } {
  const s = Number.isFinite(sec) && sec > 0 ? sec : 0;
  const m = Math.floor(s / 60);
  const whole = Math.floor(s % 60);
  return {
    main: `${String(m).padStart(2, '0')}:${String(whole).padStart(2, '0')}`,
    frac: `.${String(Math.floor((s % 1) * 100)).padStart(2, '0')}`,
  };
}

/** A small seeded generator so a track always paints the same waveform. */
function seeded(seed: string) {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
  return () => {
    h = Math.imul(h ^ (h >>> 15), 2246822507);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    h ^= h >>> 16;
    return (h >>> 0) / 4294967296;
  };
}

const WAVE_N = 210;

/** Two mirrored waveforms as dense vertical strokes (cyan above, magenta below), drawn from the track id. */
function wavePaths(seed: string): { a: string; b: string } {
  const r = seeded(seed || 'night-circuit');
  const phase = r() * 6;
  const sections = [r(), r(), r(), r(), r()];
  let a = '';
  let b = '';
  for (let i = 0; i < WAVE_N; i++) {
    const x = (i / (WAVE_N - 1)) * 300;
    const t = i / WAVE_N;
    const part = sections[Math.min(4, Math.floor(t * 5))]!;
    const body = 0.35 + part * 0.45 + 0.2 * Math.sin(t * 9 + phase);
    const fade = Math.min(1, t * 14, (1 - t) * 14);
    const hit = i % 6 === 0 ? 0.35 : 0;
    const ha = Math.max(0.6, (body + hit) * fade * (0.55 + r() * 0.6) * 15);
    const hb = Math.max(0.5, (body * 0.8 + hit * 0.5) * fade * (0.5 + r() * 0.6) * 9);
    a += `M${x.toFixed(1)} ${(22 - ha).toFixed(1)}V${(22 + ha).toFixed(1)}`;
    b += `M${x.toFixed(1)} ${(50 - hb).toFixed(1)}V${(50 + hb).toFixed(1)}`;
  }
  return { a, b };
}

function Waveform({ seed }: { seed: string }) {
  const { a, b } = useMemo(() => wavePaths(seed), [seed]);
  const draw = (cls: string) => (
    <svg viewBox="0 0 300 64" preserveAspectRatio="none" className={cls}>
      <path d="M0 22H300M0 50H300" className="nc-wave-axis" />
      <path d={a} className="nc-wave-a" />
      <path d={b} className="nc-wave-b" />
    </svg>
  );
  return (
    <Art className="nc-wave">
      <span className="nc-wave-grid" />
      {draw('nc-wave-dim')}
      <span className="nc-wave-done">{draw('nc-wave-lit')}</span>
    </Art>
  );
}

function Header() {
  const t = useT();
  const status = usePlayerStore((s) => s.status);
  return (
    <div className="nc-head">
      <Art className="nc-title">
        <span className="nc-title-sys">
          {PAINT.sys}
          <span className="nc-title-ver">{PAINT.ver}</span>
        </span>
        <span className="nc-title-state">[{t(`player.state.${status}`)}]</span>
      </Art>
      <Art className="nc-readouts">
        <span>
          {PAINT.dsp} <b className="nc-dsp" />
        </span>
        {PAINT.readouts.map((r) => (
          <span key={r}>{r}</span>
        ))}
      </Art>
      <span className="nc-win">
        <SkinButton className="nc-winbtn" />
        <DockButton className="nc-winbtn" />
        <CloseButton className="nc-winbtn nc-winbtn-x" />
      </span>
    </div>
  );
}

/** The city at night the cover frame shows when the track has no cover of its own. */
function CityArt() {
  return (
    <svg viewBox="0 0 120 120" preserveAspectRatio="xMidYMid slice" className="nc-city">
      <defs>
        <linearGradient id="nc-sky" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="nc-stop-sky0" />
          <stop offset="1" className="nc-stop-sky1" />
        </linearGradient>
        <linearGradient id="nc-sun" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="nc-stop-sun0" />
          <stop offset="1" className="nc-stop-sun1" />
        </linearGradient>
      </defs>
      <rect width="120" height="120" fill="url(#nc-sky)" />
      <path
        d="M12 14h.1M30 8h.1M88 12h.1M104 26h.1M70 6h.1M20 34h.1M98 40h.1M46 18h.1M112 8h.1"
        className="nc-stars"
      />
      <circle cx="60" cy="62" r="38" className="nc-sun-halo" />
      <circle cx="60" cy="62" r="30" fill="url(#nc-sun)" />
      <path d="M28 58h64M27 64h66M28 70h64M30 76h60" className="nc-sun-cut" />
      <path
        d="M0 120V70h10v-8h8v14h7V52h12v24h6V40h4v-6h6v6h4v40h7V60h10v18h6V46h9v30h7V66h8v12h6v42z"
        className="nc-skyline"
      />
      <path
        d="M14 82h2M14 88h2M41 58h3M41 64h3M41 70h3M57 46h2M57 52h2M57 60h2M57 68h2M76 66h3M76 72h3M92 52h3M92 58h3M92 66h3"
        className="nc-windows"
      />
      <path
        d="M0 98H120M0 106H120M0 115H120M60 92L4 120M60 92L116 120M60 92V120M60 92L34 120M60 92L86 120"
        className="nc-floor"
      />
    </svg>
  );
}

function Cover() {
  const track = usePlayerStore((s) => currentTrack(s));
  const index = usePlayerStore((s) => s.index);
  const tag = (track?.format ?? '---').toUpperCase();
  const caption = `ID: NC-${String(Math.max(0, index + 1)).padStart(4, '0')} // ${tag}`;
  return (
    <Panel className="nc-cover" tone="cyan">
      <Art className="nc-cover-art">
        <span className="nc-cover-img">
          {track?.artworkUrl ? <img src={track.artworkUrl} alt="" /> : <CityArt />}
          <span className="nc-bracket" />
        </span>
        <span className="nc-cover-cap">{caption}</span>
      </Art>
    </Panel>
  );
}

function Deck() {
  const t = useT();
  const text = useNowPlayingText();
  const track = usePlayerStore((s) => currentTrack(s));
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const status = usePlayerStore((s) => s.status);
  const index = usePlayerStore((s) => s.index);
  const total = usePlayerStore((s) => s.queue.length);
  const hasTrack = index >= 0;
  const face = clockFace(position);
  const pos = duration > 0 ? Math.min(1, position / duration) : 0;
  const counter = `${String(Math.max(0, index + 1)).padStart(2, '0')}/${String(total).padStart(2, '0')}`;
  return (
    <Panel className="nc-deck" tone="steel">
      <div className="nc-deck-top">
        <span className="nc-deck-title">
          <Ticker text={text} />
        </span>
        <div className="nc-clock" data-testid="player-state" data-state={status}>
          <span className="nc-clock-big">
            {face.main}
            <small>{face.frac}</small>
          </span>
          <span className="nc-clock-sub">
            {t(`player.state.${status}`)} {'//'} {formatTime(duration)} {'//'} {counter}
          </span>
        </div>
      </div>
      <div className="nc-scrub" style={{ '--nc-pos': pos } as CSSProperties}>
        <Waveform seed={track?.id ?? ''} />
        <input
          type="range"
          className="nc-seek"
          aria-label={t('player.seek')}
          aria-valuetext={`${formatTime(position)} / ${formatTime(duration)}`}
          min={0}
          max={duration > 0 ? Math.floor(duration) : 0}
          step={1}
          value={Math.min(Math.floor(position), Math.floor(duration))}
          disabled={!hasTrack || duration <= 0}
          onChange={(e) => seek(Number(e.target.value))}
        />
      </div>
    </Panel>
  );
}

function Radar() {
  return (
    <Panel className="nc-radar" tone="steel">
      <Art className="nc-radar-art">
        <span className="nc-radar-scope">
          <svg viewBox="0 0 100 100">
            <defs>
              <radialGradient id="nc-scope" cx="0.5" cy="0.5" r="0.5">
                <stop offset="0" className="nc-stop-scope0" />
                <stop offset="1" className="nc-stop-scope1" />
              </radialGradient>
              <linearGradient id="nc-sweep" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0" className="nc-stop-sweep0" />
                <stop offset="1" className="nc-stop-sweep1" />
              </linearGradient>
            </defs>
            <circle cx="50" cy="50" r="46" fill="url(#nc-scope)" className="nc-radar-rim" />
            {[36, 26, 16, 7].map((r) => (
              <circle key={r} cx="50" cy="50" r={r} className="nc-radar-ring" />
            ))}
            <path
              d="M50 4V96M4 50H96M17 17L83 83M83 17L17 83"
              className="nc-radar-ring nc-radar-fine"
            />
            <path
              d="M50 4v3M50 93v3M4 50h3M93 50h3M27 10l1.5 2.6M73 10l-1.5 2.6M10 27l2.6 1.5M90 27l-2.6 1.5M10 73l2.6-1.5M90 73l-2.6-1.5M27 90l1.5-2.6M73 90l-1.5-2.6"
              className="nc-radar-tick"
            />
            <g className="nc-radar-sweep">
              <path d="M50 50L50 4A46 46 0 0 1 89.8 27Z" fill="url(#nc-sweep)" />
              <path d="M50 50L89.8 27" className="nc-radar-beam" />
            </g>
            <path d="M34 58q8-14 20-6t14 18" className="nc-radar-trace" />
            <circle cx="66" cy="34" r="1.8" className="nc-radar-blip" />
            <circle cx="38" cy="62" r="1.4" className="nc-radar-blip nc-radar-blip-b" />
            <circle cx="58" cy="70" r="1.1" className="nc-radar-blip" />
            <circle cx="50" cy="50" r="1.6" className="nc-radar-core" />
          </svg>
        </span>
        <span className="nc-radar-cap">{PAINT.phase}</span>
      </Art>
    </Panel>
  );
}

function Spectrum() {
  return (
    <Panel className="nc-spec" tone="cyan">
      <div className="nc-spec-screen">
        <Art className="nc-spec-grid">
          {PEAKS.map((p) => (
            <span key={p}>{p}</span>
          ))}
        </Art>
        <SpectrumBars className="nc-spectrum" bands={48} />
      </div>
      <Art className="nc-scale">
        {SCALE.map((s) => (
          <span key={s}>{s}</span>
        ))}
      </Art>
    </Panel>
  );
}

/** The four cue pads (copied from the shared ones so each can wear its own neon glow) and four offline pads. */
function Pads() {
  const t = useT();
  const duration = usePlayerStore((s) => s.duration);
  return (
    <Panel className="nc-pads" tone="steel">
      <Art className="nc-pads-head">
        <span>{PAINT.pads}</span>
        <i />
        <i />
        <i />
      </Art>
      <div className="nc-cues" role="group" aria-label={t('player.skinx.cues')}>
        {[0, 25, 50, 75].map((pct, i) => (
          <span key={pct} className="nc-hex" data-cue={i + 1}>
            <button
              type="button"
              className="sr-ply-cue"
              data-cue={i + 1}
              aria-label={t('player.skinx.cue', { n: i + 1, pct })}
              disabled={duration <= 0}
              onClick={() => seek((duration * pct) / 100)}
            >
              <span aria-hidden="true">{t('player.skinx.cueShort', { n: i + 1 })}</span>
            </button>
          </span>
        ))}
      </div>
      <Art className="nc-offline">
        {OFFLINE_PADS.map(([a, b]) => (
          <span key={`${a}${b}`} className="nc-hex nc-hex-off">
            <span>
              {a}
              {b && <br />}
              {b}
            </span>
          </span>
        ))}
      </Art>
      <Art className="nc-steps">
        {Array.from({ length: 16 }, (_, i) => (
          <i key={i} style={{ '--s': i } as CSSProperties} />
        ))}
      </Art>
    </Panel>
  );
}

function Transport() {
  const t = useT();
  const status = usePlayerStore((s) => s.status);
  const busy = isBusy(status);
  return (
    <Panel className="nc-transport" tone="steel">
      <div className="nc-tx" role="group" aria-label={t('player.controls')}>
        {/* "Prev" is painted over the button rather than in it: it is not a word of "Previous track". */}
        <span className="nc-tx-slot">
          <button
            type="button"
            className="nc-tx-btn nc-tx-prev"
            aria-label={t('player.prev')}
            onClick={prev}
          >
            <span className="nc-tx-face" aria-hidden="true">
              <IconPrev />
              <span className="nc-tx-gap" />
            </span>
          </button>
          <Art className="nc-tx-word">{t('player.skinx.circuit.prev')}</Art>
        </span>
        <button
          type="button"
          className="nc-tx-btn nc-tx-play"
          aria-label={busy ? t('player.pause') : t('player.play')}
          data-busy={busy}
          onClick={toggle}
        >
          <span className="nc-tx-face" aria-hidden="true">
            {busy ? <IconPause /> : <IconPlay />}
            <span>{busy ? t('player.pause') : t('player.play')}</span>
          </span>
        </button>
        <button
          type="button"
          className="nc-tx-btn nc-tx-stop"
          aria-label={t('player.stop')}
          onClick={() => {
            pause();
            seek(0);
          }}
        >
          <span className="nc-tx-face" aria-hidden="true">
            <IconStop />
            <span>{t('player.stop')}</span>
          </span>
        </button>
        <button
          type="button"
          className="nc-tx-btn nc-tx-next"
          aria-label={t('player.next')}
          onClick={() => next()}
        >
          <span className="nc-tx-face" aria-hidden="true">
            <span>{t('player.skinx.circuit.next')}</span>
            <IconNext />
          </span>
        </button>
      </div>
    </Panel>
  );
}

/** The gain fader: the player's volume as a vertical fader, with mute under it. */
function Fader() {
  const t = useT();
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  const value = Math.round((muted ? 0 : volume) * 100);
  return (
    <Panel className="nc-fader" tone="steel">
      <Art className="nc-fader-label">{t('player.skinx.circuit.gain')}</Art>
      <div className="nc-fader-slot" style={{ '--nc-vol': value / 100 } as CSSProperties}>
        <Art className="nc-fader-art">
          <span className="nc-fader-fill" />
          <span className="nc-fader-cap" />
        </Art>
        <input
          type="range"
          className="nc-fader-input"
          aria-label={t('player.volume')}
          aria-orientation="vertical"
          min={0}
          max={100}
          step={1}
          value={value}
          onChange={(e) => setVolume(Number(e.target.value) / 100)}
        />
      </div>
      <Art className="nc-fader-read">{`${value}%`}</Art>
      <button
        type="button"
        className="nc-mute"
        aria-pressed={muted}
        aria-label={muted ? t('player.unmute') : t('player.mute')}
        onClick={toggleMute}
      >
        <IconVolume muted={muted || volume === 0} />
      </button>
    </Panel>
  );
}

/** Which equalizer bands each tone knob moves. */
const KNOB_BANDS: Record<'sub' | 'mid' | 'air', readonly number[]> = {
  sub: [0, 1, 2],
  mid: [4, 5, 6],
  air: [8, 9],
};
type KnobId = keyof typeof KNOB_BANDS;
const KNOB_TAGS: Record<KnobId | 'fade', string> = {
  sub: 'SUB_BASS',
  mid: 'MID_PUNCH',
  air: 'AIR_TREB',
  fade: 'X_FADE',
};

/** A painted rotary knob with a live range input laid over it (arrow keys or a sideways drag turn it). */
function Knob({
  tag,
  label,
  value,
  min,
  max,
  valueText,
  onChange,
}: {
  tag: string;
  label: string;
  value: number;
  min: number;
  max: number;
  valueText: string;
  onChange: (v: number) => void;
}) {
  const turn = (value - min) / (max - min);
  return (
    <div className="nc-knob" style={{ '--nc-turn': turn } as CSSProperties}>
      <span className="nc-knob-well">
        <Art className="nc-knob-art">
          <span className="nc-knob-ring" />
          <span className="nc-knob-cap" />
        </Art>
        <input
          type="range"
          className="nc-knob-input"
          aria-label={label}
          aria-valuetext={valueText}
          min={min}
          max={max}
          step={1}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
        />
      </span>
      <Art className="nc-knob-tag">{tag}</Art>
    </div>
  );
}

const hz = (f: number) => (f >= 1000 ? `${f / 1000} kHz` : `${f} Hz`);

function Knobs() {
  const t = useT();
  const eq = usePlayerStore((s) => s.eq);
  const fade = usePlayerStore((s) => s.crossfadeSec);
  const tone = (id: KnobId) => {
    const bands = KNOB_BANDS[id];
    const avg = bands.reduce((sum, b) => sum + (eq[b] ?? 0), 0) / bands.length;
    return Math.round(avg);
  };
  const setTone = (id: KnobId, db: number) => {
    const nextGains = EQ_FREQS.map((_, i) => eq[i] ?? 0);
    for (const b of KNOB_BANDS[id]) nextGains[b] = db;
    setEq(nextGains);
    if (!usePlayerStore.getState().eqEnabled) setEqEnabled(true);
  };
  const range = (id: KnobId) => {
    const bands = KNOB_BANDS[id];
    return `${hz(EQ_FREQS[bands[0] ?? 0] ?? 0)}–${hz(EQ_FREQS[bands[bands.length - 1] ?? 0] ?? 0)}`;
  };
  return (
    <Panel className="nc-knobs" tone="steel">
      <div className="nc-knob-row" role="group" aria-label={t('player.skinx.circuit.tone')}>
        {(Object.keys(KNOB_BANDS) as KnobId[]).map((id) => (
          <Knob
            key={id}
            tag={KNOB_TAGS[id]}
            label={t(`player.skinx.circuit.${id}`, { range: range(id) })}
            value={tone(id)}
            min={-EQ_MAX_DB}
            max={EQ_MAX_DB}
            valueText={t('music.lib.eq.db', { db: tone(id) })}
            onChange={(v) => setTone(id, v)}
          />
        ))}
        <Knob
          tag={KNOB_TAGS.fade}
          label={t('music.lib.eq.crossfade')}
          value={Math.round(fade)}
          min={0}
          max={CROSSFADE_MAX_SEC}
          valueText={t('music.lib.eq.seconds', { n: Math.round(fade) })}
          onChange={setCrossfade}
        />
      </div>
      <EqSwitch className="nc-eq" label={t('player.skinx.circuit.eqLabel')}>
        <span aria-hidden="true">{t('player.skinx.circuit.eq')}</span>
      </EqSwitch>
    </Panel>
  );
}

/** A second LED row on the same master level (the analyser hears a mono sum, so both rows read alike). */
function MeterRow({ segments }: { segments: number }) {
  return (
    <div
      className="sr-ply-bd-leds nc-led"
      style={{ '--src': 'var(--bd-level, 0)', '--n': segments } as CSSProperties}
    >
      {Array.from({ length: segments }, (_, i) => (
        <span
          key={i}
          data-zone={i >= segments - 4 ? 'hot' : i >= segments - 10 ? 'warm' : 'cool'}
          style={{ '--i': i } as CSSProperties}
        />
      ))}
    </div>
  );
}

function Meters() {
  const t = useT();
  const repeat = usePlayerStore((s) => s.repeat);
  const shuffle = usePlayerStore((s) => s.shuffle);
  return (
    <Panel className="nc-meters" tone="steel">
      <Art className="nc-meter-tag">{PAINT.left}</Art>
      <div className="nc-meter-l">
        <LedMeter className="nc-led" segments={32} />
      </div>
      <Art className="nc-meter-tag">{PAINT.right}</Art>
      <Art className="nc-meter-r">
        <MeterRow segments={32} />
      </Art>
      <Art className="nc-db">
        {DB.map((d) => (
          <span key={d}>{d}</span>
        ))}
      </Art>
      <div className="nc-flags">
        <Art className="nc-limiter">{PAINT.limiter}</Art>
        <button
          type="button"
          className="nc-flag"
          aria-pressed={repeat !== 'off'}
          aria-label={t(`player.repeat.${repeat}`)}
          data-repeat={repeat}
          onClick={cycleRepeat}
        >
          <span aria-hidden="true">[{t(`player.repeat.${repeat}`)}]</span>
        </button>
        <button
          type="button"
          className="nc-flag"
          aria-pressed={shuffle}
          aria-label={t('player.skinx.shuffle')}
          onClick={toggleShuffle}
        >
          <span aria-hidden="true">[{t('player.skinx.shuffle')}]</span>
        </button>
        <LibraryButton className="nc-flag nc-flag-lib" label={t('player.skinx.circuit.libLabel')}>
          <span aria-hidden="true">[{t('player.skinx.circuit.lib')}]</span>
        </LibraryButton>
      </div>
    </Panel>
  );
}

/** The queue as a table of buttons: each row plays its track. */
function TrackTable() {
  const t = useT();
  const queue = usePlayerStore((s) => s.queue);
  const index = usePlayerStore((s) => s.index);
  const duration = usePlayerStore((s) => s.duration);
  return (
    <Panel className="nc-table" tone="steel">
      <Art className="nc-thead">
        <span>{t('player.skinx.circuit.idx')}</span>
        <span>{t('player.skinx.circuit.track')}</span>
        <span className="nc-c-artist">{t('player.skinx.circuit.artist')}</span>
        <span>{t('player.skinx.circuit.len')}</span>
      </Art>
      {queue.length === 0 ? (
        <p className="sr-ply-empty">{t('player.queue.empty')}</p>
      ) : (
        <ol className="nc-rows" aria-label={t('player.queue.title')}>
          {queue.map((track, i) => {
            const len = i === index && duration > 0 ? duration : track.durationSec;
            return (
              <li key={`${i}:${track.id}`} data-current={i === index}>
                <button
                  type="button"
                  aria-current={i === index ? 'true' : undefined}
                  onClick={() => playIndex(i)}
                >
                  <span className="nc-c-idx">{String(i + 1).padStart(2, '0')}</span>
                  <span className="nc-c-title">{track.title}</span>
                  <span className="nc-c-artist">{track.artist ?? '—'}</span>
                  <span className="nc-c-len">{len && len > 0 ? formatTime(len) : '--:--'}</span>
                </button>
              </li>
            );
          })}
        </ol>
      )}
    </Panel>
  );
}

export function CircuitSkin() {
  return (
    <BackdropFrame skin="circuit" scheme="dark">
      <div className="nc-body">
        <Art className="nc-shell">
          <span className="nc-shell-plate" />
          <span className="nc-trim nc-trim-top" />
          <span className="nc-trim nc-trim-bottom" />
          <span className="nc-scan" />
        </Art>
        <Header />
        <div className="nc-grid">
          <Cover />
          <Deck />
          <Radar />
          <Pads />
          <Spectrum />
          <Art className="nc-hazard nc-hazard-a" />
          <Transport />
          <Fader />
          <Knobs />
          <TrackTable />
          <Meters />
        </div>
      </div>
    </BackdropFrame>
  );
}

export default CircuitSkin;
