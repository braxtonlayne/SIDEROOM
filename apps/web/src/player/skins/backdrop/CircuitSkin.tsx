/**
 * MUS-SKN-001 "Night Circuit": a neon DJ deck. Painted: the angled frame, the hazard stripes, the radar dish
 * (its sweep turns while a track plays), the frequency scale. Live: the ticker and the big clock, a real spectrum
 * from the player's analyser and a master level meter, four cue pads (jump to 0, 25, 50 and 75 percent), the
 * transport, repeat and shuffle, volume, the real queue table, skin, dock and close.
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
import { Art, BackdropFrame, LedMeter, SpectrumBars } from './frame';
import { CuePads, QueueButtons, ShuffleButton } from './parts';
import './circuit.css';

const SCALE = ['60', '250', '1k', '4k', '16k'];

function Clock() {
  const t = useT();
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const status = usePlayerStore((s) => s.status);
  return (
    <div className="nc-clock" data-testid="player-state" data-state={status}>
      <span className="nc-clock-big">{formatTime(position)}</span>
      <span className="nc-clock-sub">
        {t(`player.state.${status}`)} / {formatTime(duration)}
      </span>
    </div>
  );
}

function Radar() {
  return (
    <Art className="nc-radar">
      <svg viewBox="0 0 80 80">
        <circle cx="40" cy="40" r="36" className="nc-radar-ring" />
        <circle cx="40" cy="40" r="24" className="nc-radar-ring" />
        <circle cx="40" cy="40" r="12" className="nc-radar-ring" />
        <path d="M40 4 V76 M4 40 H76" className="nc-radar-ring" />
        <g className="nc-radar-sweep">
          <path d="M40 40 L40 4 A36 36 0 0 1 70 22 Z" />
        </g>
      </svg>
    </Art>
  );
}

export function CircuitSkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <BackdropFrame
      skin="circuit"
      scheme="dark"
      art={
        <>
          <span className="nc-scan" />
          <span className="nc-corner nc-corner-a" />
          <span className="nc-corner nc-corner-b" />
        </>
      }
    >
      <div className="nc-head">
        <span className="nc-tag">
          <Ticker text={text} />
        </span>
        <span className="nc-win">
          <SkinButton className="nc-winbtn" />
          <DockButton className="nc-winbtn" />
          <CloseButton className="nc-winbtn nc-winbtn-x" />
        </span>
      </div>
      <div className="nc-grid">
        <div className="nc-panel nc-vis">
          <SpectrumBars className="nc-spectrum" bands={24} />
          <Art className="nc-scale">
            {SCALE.map((s) => (
              <span key={s}>{s}</span>
            ))}
          </Art>
        </div>
        <div className="nc-panel nc-side">
          <Clock />
          <Radar />
        </div>
        <div className="nc-panel nc-seek">
          <SeekBar className="nc-range" />
          <LedMeter className="nc-leds" segments={20} />
        </div>
        <div className="nc-panel nc-deck">
          <CuePads className="nc-cues" />
          <div className="nc-transport" role="group" aria-label={t('player.controls')}>
            <PrevButton className="nc-btn nc-btn-y" />
            <PlayPauseButton className="nc-btn nc-btn-play" />
            <StopButton className="nc-btn nc-btn-r" />
            <NextButton className="nc-btn nc-btn-y" />
          </div>
          <div className="nc-mods">
            <RepeatButton className="nc-mod" />
            <ShuffleButton className="nc-mod" />
            <VolumeControl className="nc-mod" />
          </div>
        </div>
        <div className="nc-panel nc-list">
          <QueueButtons className="nc-queue" />
        </div>
        <Art className="nc-hazard" />
      </div>
    </BackdropFrame>
  );
}

export default CircuitSkin;
