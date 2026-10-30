/**
 * MUS-SKN-001 "Valve & Walnut": a brass and walnut tube hi-fi cabinet. Painted: the walnut cheeks, the riveted
 * iron panel, the glowing valve, the tape reels, the brass knobs. Live: the nameplate ticker, two real VU meters
 * (low and high band from the player's analyser), the Nixie-style position readout, the brass seek rail, the
 * chrome piano keys, the tone switch (the equalizer on or off), shuffle, volume, skin, dock and close.
 */
import { useT } from '../../../i18n';
import {
  CloseButton,
  DockButton,
  NextButton,
  PlayPauseButton,
  PrevButton,
  RepeatButton,
  SeekBar,
  SkinButton,
  StopButton,
  Ticker,
  VolumeControl,
  formatTime,
  useNowPlayingText,
} from '../../controls';
import { usePlayerStore } from '../../store';
import { Art, BackdropFrame } from './frame';
import { EqSwitch, ShuffleButton } from './parts';
import './valve.css';

function Screw({ x, y }: { x: number; y: number }) {
  return (
    <g transform={`translate(${x} ${y})`}>
      <circle r="5" className="v-screw" />
      <path d="M-3.5 1 3.5 -1" className="v-screw-slot" />
    </g>
  );
}

/** One VU meter: a cream face, a scale and a needle driven by `--bd-low` or `--bd-high`. */
function VuMeter({ band, label }: { band: 'low' | 'high'; label: string }) {
  return (
    <Art className="v-vu" style={{ '--needle': `var(--bd-${band}, 0)` } as React.CSSProperties}>
      <svg viewBox="0 0 120 70" preserveAspectRatio="xMidYMid meet">
        <rect x="1" y="1" width="118" height="68" rx="6" className="v-vu-face" />
        <path d="M18 52 A44 44 0 0 1 102 52" className="v-vu-arc" />
        <path d="M86 26 A44 44 0 0 1 102 52" className="v-vu-red" />
        {Array.from({ length: 11 }, (_, i) => {
          const a = (-60 + i * 12) * (Math.PI / 180);
          const x1 = 60 + Math.sin(a) * 42;
          const y1 = 62 - Math.cos(a) * 42;
          const x2 = 60 + Math.sin(a) * (i % 5 === 0 ? 34 : 37);
          const y2 = 62 - Math.cos(a) * (i % 5 === 0 ? 34 : 37);
          return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} className="v-vu-tick" />;
        })}
        <text x="60" y="52" textAnchor="middle" className="v-vu-label">
          {label}
        </text>
        <g className="v-vu-needle">
          <line x1="60" y1="64" x2="60" y2="20" />
        </g>
        <circle cx="60" cy="64" r="5" className="v-vu-pivot" />
      </svg>
    </Art>
  );
}

function Valve() {
  return (
    <Art className="v-tube">
      <svg viewBox="0 0 60 120">
        <defs>
          <radialGradient id="v-glow" cx="50%" cy="55%" r="50%">
            <stop offset="0" className="v-glow-core" />
            <stop offset="1" className="v-glow-edge" />
          </radialGradient>
        </defs>
        <rect x="6" y="2" width="48" height="116" rx="22" className="v-tube-niche" />
        <ellipse cx="30" cy="62" rx="22" ry="34" fill="url(#v-glow)" className="v-tube-halo" />
        <path d="M16 96 V38 a14 14 0 0 1 28 0 V96 Z" className="v-tube-glass" />
        <rect x="21" y="44" width="18" height="40" rx="2" className="v-tube-plate" />
        <path d="M24 84 V50 M30 84 V48 M36 84 V50" className="v-tube-filament" />
        <rect x="14" y="94" width="32" height="14" rx="3" className="v-tube-base" />
        <path d="M22 108 v6 M30 108 v6 M38 108 v6" className="v-tube-pins" />
      </svg>
    </Art>
  );
}

function Reels() {
  const reel = (cx: number) => (
    <g transform={`translate(${cx} 34)`}>
      <circle r="27" className="v-reel-tape" />
      <g className="v-reel-spin">
        <circle r="22" className="v-reel" />
        {[0, 120, 240].map((a) => (
          <path
            key={a}
            d="M0 -6 L-9 -18 A20 20 0 0 1 9 -18 Z"
            transform={`rotate(${a})`}
            className="v-reel-hole"
          />
        ))}
        <circle r="5" className="v-reel-hub" />
      </g>
    </g>
  );
  return (
    <Art className="v-deck">
      <svg viewBox="0 0 220 70" preserveAspectRatio="xMidYMid meet">
        <rect x="1" y="1" width="218" height="68" rx="8" className="v-deck-glass" />
        <path d="M44 61 C80 70 140 70 176 61" className="v-tape" />
        {reel(48)}
        {reel(172)}
        <rect x="92" y="46" width="36" height="16" rx="3" className="v-head" />
      </svg>
    </Art>
  );
}

function Nixie() {
  const t = useT();
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const status = usePlayerStore((s) => s.status);
  return (
    <div className="v-nixie" data-testid="player-state" data-state={status}>
      <span className="v-nixie-digits">{formatTime(position)}</span>
      <span className="v-nixie-total">
        <span className="sr-ply-bd-sr">{t(`player.state.${status}`)} </span>/ {formatTime(duration)}
      </span>
    </div>
  );
}

export function ValveSkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <BackdropFrame
      skin="valve"
      scheme="dark"
      art={
        <svg className="v-cabinet" viewBox="0 0 100 100" preserveAspectRatio="none">
          <rect x="0" y="0" width="100" height="100" className="v-iron" />
        </svg>
      }
    >
      <Art className="v-cheek v-cheek-start" />
      <Art className="v-cheek v-cheek-end" />
      <div className="v-body">
        <div className="v-plate">
          <Art className="v-rivets">
            <svg viewBox="0 0 20 20" width="16" height="16">
              <Screw x={10} y={10} />
            </svg>
          </Art>
          <Ticker text={text} />
          <Art className="v-lamp" />
        </div>
        <div className="v-meters">
          <Valve />
          <VuMeter band="low" label={t('player.skinx.valve.low')} />
          <VuMeter band="high" label={t('player.skinx.valve.high')} />
          <Nixie />
        </div>
        <Reels />
        <div className="v-rail">
          <SeekBar className="v-seek" />
        </div>
        <div className="v-keys" role="group" aria-label={t('player.controls')}>
          <PrevButton className="v-key" />
          <PlayPauseButton className="v-key v-key-main" />
          <StopButton className="v-key" />
          <NextButton className="v-key" />
          <RepeatButton className="v-key" />
          <ShuffleButton className="v-key" />
        </div>
        <div className="v-foot">
          <EqSwitch className="v-toggle">
            <span className="v-toggle-lever" aria-hidden="true" />
            <span className="v-toggle-label">{t('player.skinx.valve.tone')}</span>
          </EqSwitch>
          <Art className="v-knobs">
            {[0, 1, 2].map((i) => (
              <span key={i} className="v-knob" />
            ))}
          </Art>
          <VolumeControl className="v-small" />
          <span className="v-actions">
            <SkinButton className="v-small" />
            <DockButton className="v-small" />
            <CloseButton className="v-small" />
          </span>
        </div>
      </div>
    </BackdropFrame>
  );
}

export default ValveSkin;
