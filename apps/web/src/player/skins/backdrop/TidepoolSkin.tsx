/**
 * MUS-SKN-002 "Tidepool Glass": a turn-of-the-millennium aqua media player. A molded pearl-and-silver body with
 * a glossy green gel wing on one side and a translucent aqua playlist wing on the other, a deep black glass
 * screen in a chrome bezel, and gel transport buttons around a big blue play orb.
 *
 * Real: the now-playing title, the format and "track n of total" line, the time, the glowing waveform (its
 * height follows the level), the liquid seek tube, prev, play/pause, next, the shuffle orb, the chrome mute ball
 * and the volume tube (with its percentage), the wing's Library, Equalizer and Skin pills, the playlist wing (the
 * real queue: a click plays the track), the status strip's Repeat and Stop chips and its level meter (from the
 * analyser), dock and close.
 *
 * Painted: the pearl shell and its iridescence, the chrome rims, water droplets and sparkles, the screen glare,
 * the title capsule, the LED dots and the engraved labels under the buttons (the LEDs follow the real state),
 * and the sea picture shown when a track has no cover art.
 */
import { useId, type ReactNode } from 'react';
import { useT } from '../../../i18n';
import {
  CloseButton,
  DockButton,
  IconSkin,
  IconStop,
  NextButton,
  PlayPauseButton,
  PrevButton,
  SeekBar,
  Ticker,
  TimeReadout,
  VolumeControl,
  formatTime,
  isBusy,
  useNowPlayingText,
} from '../../controls';
import { currentTrack, cycleRepeat, pause, seek, setSkin, usePlayerStore } from '../../store';
import { PLAYER_SKINS } from '../../types';
import { Art, BackdropFrame, LedMeter } from './frame';
import { EqSwitch, LibraryButton, QueueButtons, ShuffleButton, useQueueCount } from './parts';
import './tidepool.css';

/** A sine strand across 400 units that repeats every 200, so sliding it by half loops without a seam. */
function strand(a1: number, a2: number, p1: number, p2: number): string {
  let d = '';
  for (let x = 0; x <= 400; x += 4) {
    const y =
      40 +
      a1 * Math.sin((2 * Math.PI * x) / 200 + p1) +
      a2 * Math.sin((2 * Math.PI * x) / 100 + p2);
    d += `${x === 0 ? 'M' : 'L'}${x} ${y.toFixed(1)} `;
  }
  return d.trim();
}
const STRANDS = [
  { d: strand(16, 5, 0, 1.2), k: 'a' },
  { d: strand(13, 7, 0.7, 2.4), k: 'b' },
  { d: strand(18, 3, 1.6, 0.4), k: 'c' },
  { d: strand(9, 8, 2.6, 3.4), k: 'd' },
  { d: strand(20, 2, 3.6, 1.9), k: 'e' },
];

/** A filled water surface across 400 units that repeats every 200. */
function water(amp: number, phase: number, level: number): string {
  let d = 'M0 40 ';
  for (let x = 0; x <= 400; x += 8) {
    d += `L${x} ${(level + amp * Math.sin((2 * Math.PI * x) / 200 + phase)).toFixed(1)} `;
  }
  return `${d}L400 40Z`;
}
const WATER_BACK = water(4, 1.4, 12);
const WATER_FRONT = water(5, 0, 18);

/** Strip React's id punctuation so the id works inside `url(#...)`. */
const useSvgId = (name: string) => `tp-${useId().replace(/[^a-zA-Z0-9]/g, '')}-${name}`;

/** The flowing, glowing waveform on the screen. Its height follows the level; it flows while a track plays. */
function Wave() {
  const grad = useSvgId('wave');
  const glow = useSvgId('glow');
  return (
    <Art className="tp-wave">
      <span className="tp-aurora" />
      <span className="tp-rays" />
      <span className="tp-wave-amp">
        <svg className="tp-wave-svg" viewBox="0 0 400 80" preserveAspectRatio="none">
          <defs>
            <linearGradient id={grad} x1="0" x2="1" y1="0" y2="0">
              <stop offset="0" className="tp-stop-green" />
              <stop offset="0.25" className="tp-stop-aqua" />
              <stop offset="0.5" className="tp-stop-green" />
              <stop offset="0.75" className="tp-stop-aqua" />
              <stop offset="1" className="tp-stop-green" />
            </linearGradient>
            <filter id={glow} x="-5%" y="-40%" width="110%" height="180%">
              <feGaussianBlur stdDeviation="2.4" result="b" />
              <feMerge>
                <feMergeNode in="b" />
                <feMergeNode in="b" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>
          <g filter={`url(#${glow})`} stroke={`url(#${grad})`} fill="none">
            {STRANDS.map((s) => (
              <path key={s.k} className={`tp-strand tp-strand-${s.k}`} d={s.d} />
            ))}
          </g>
          <path className="tp-strand tp-strand-core" d={STRANDS[0]!.d} />
        </svg>
      </span>
    </Art>
  );
}

/** The cover, or a painted sea picture when the track has none. */
function Cover() {
  const art = usePlayerStore((s) => currentTrack(s)?.artworkUrl);
  const sky = useSvgId('sky');
  const sea = useSvgId('sea');
  return (
    <Art className="tp-cover">
      {art ? (
        <img src={art} alt="" />
      ) : (
        <svg viewBox="0 0 100 100" preserveAspectRatio="xMidYMid slice">
          <defs>
            <linearGradient id={sky} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" className="tp-c-sky-hi" />
              <stop offset="1" className="tp-c-sky" />
            </linearGradient>
            <linearGradient id={sea} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" className="tp-c-sea-hi" />
              <stop offset="1" className="tp-c-sea" />
            </linearGradient>
          </defs>
          <rect width="100" height="62" fill={`url(#${sky})`} />
          <circle className="tp-c-sun" cx="74" cy="22" r="9" />
          <path
            className="tp-c-cloud"
            d="M8 30c0-5 5-8 9-6 2-5 10-6 13-1 5-1 8 3 7 7H8z M50 18c1-4 6-5 8-2 3-2 7 0 7 4H50z"
          />
          <rect y="58" width="100" height="42" fill={`url(#${sea})`} />
          <path className="tp-c-glint" d="M56 66h16M62 72h22M70 80h18M8 84h12M48 90h20" />
          <ellipse className="tp-c-sand" cx="30" cy="70" rx="24" ry="5.5" />
          <path className="tp-c-foam" d="M7 72 C16 76 44 76 54 71" />
          <path className="tp-c-trunk" d="M29 68 C30 58 31 50 37 41" />
          <g className="tp-c-frond">
            <path d="M37 41 C30 35 21 36 15 42 C23 38 30 38 37 41z" />
            <path d="M37 41 C33 32 25 28 18 30 C26 31 32 35 37 41z" />
            <path d="M37 41 C40 31 48 28 54 30 C47 31 41 35 37 41z" />
            <path d="M37 41 C45 36 53 38 58 45 C51 40 44 39 37 41z" />
            <path d="M37 41 C37 47 34 52 30 55 C33 50 35 46 37 41z" />
          </g>
          <circle className="tp-c-nut" cx="36" cy="43" r="1.6" />
          <circle className="tp-c-spray" cx="80" cy="62" r="0.9" />
          <circle className="tp-c-spray" cx="86" cy="64" r="0.6" />
        </svg>
      )}
      <span className="tp-cover-glare" />
    </Art>
  );
}

/** The gel inside a wing: a gloss across the top, liquid sloshing at the bottom, bubbles rising. */
function WingArt({ drops }: { drops: [number, number] }) {
  return (
    <Art className="tp-wing-art">
      <span className="tp-water">
        <svg viewBox="0 0 400 40" preserveAspectRatio="none">
          <path className="tp-water-back" d={WATER_BACK} />
          <path className="tp-water-front" d={WATER_FRONT} />
        </svg>
      </span>
      <span className="tp-bub tp-bub-1" />
      <span className="tp-bub tp-bub-2" />
      <span className="tp-bub tp-bub-3" />
      <span className="tp-wing-gloss" />
      <span className="tp-wing-streak" />
      <Drop n={drops[0]} />
      <Drop n={drops[1]} />
    </Art>
  );
}

/** The queue's total length, at the foot of the playlist wing (unknown lengths count as nothing). */
function QueueTotal() {
  const t = useT();
  const total = usePlayerStore((s) =>
    s.queue.reduce(
      (sum, track, i) =>
        sum + (i === s.index && s.duration > 0 ? s.duration : (track.durationSec ?? 0)),
      0,
    ),
  );
  const count = usePlayerStore((s) => s.queue.length);
  if (count === 0) return null;
  return (
    <p className="tp-total">{t('player.skinx.tidepool.total', { time: formatTime(total) })}</p>
  );
}

/** The engraved label under a button, with an LED that follows the real state. */
function Etch({ children, on }: { children: ReactNode; on?: boolean }) {
  return (
    <Art className="tp-etch">
      <span className="tp-led" data-on={on ? 'true' : 'false'} />
      <span className="tp-etch-text">{children}</span>
    </Art>
  );
}

/** A water droplet sitting on the shell. */
const Drop = ({ n }: { n: number }) => <span className={`tp-drop tp-drop-${n}`} />;

/** Repeat as a chip in the status strip: its visible text is its name. */
function RepeatChip() {
  const t = useT();
  const repeat = usePlayerStore((s) => s.repeat);
  const label = t(`player.repeat.${repeat}`);
  return (
    <button
      type="button"
      className="tp-chip"
      aria-pressed={repeat !== 'off'}
      aria-label={label}
      data-repeat={repeat}
      onClick={cycleRepeat}
    >
      <span className="tp-chip-led" aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}

function StopChip() {
  const t = useT();
  return (
    <button
      type="button"
      className="tp-chip"
      aria-label={t('player.stop')}
      onClick={() => {
        pause();
        seek(0);
      }}
    >
      <IconStop />
      <span>{t('player.skinx.tidepool.stop')}</span>
    </button>
  );
}

/** The next-skin button as a gel pill with a visible word (the shared one is an icon only). */
function SkinPill() {
  const t = useT();
  const skin = usePlayerStore((s) => s.skin);
  const nextSkin = PLAYER_SKINS[(PLAYER_SKINS.indexOf(skin) + 1) % PLAYER_SKINS.length] ?? skin;
  return (
    <button
      type="button"
      className="tp-pill"
      aria-label={t('player.cycleSkin', { skin: t(`player.skins.${nextSkin}`) })}
      title={t(`player.skins.${nextSkin}`)}
      onClick={() => setSkin(nextSkin)}
    >
      <span className="tp-pill-led" aria-hidden="true" />
      <IconSkin />
      <span className="tp-pill-text">{t('player.skinx.tidepool.skin')}</span>
    </button>
  );
}

export function TidepoolSkin() {
  const t = useT();
  const text = useNowPlayingText();
  const { n, total } = useQueueCount();
  const format = usePlayerStore((s) => currentTrack(s)?.format);
  const busy = usePlayerStore((s) => isBusy(s.status));
  const shuffle = usePlayerStore((s) => s.shuffle);
  const eqOn = usePlayerStore((s) => s.eqEnabled);
  const volume = usePlayerStore((s) => (s.muted ? 0 : s.volume));
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const pct = duration > 0 ? Math.min(100, (position / duration) * 100) : 0;
  const volPct = Math.round(volume * 100);
  const grain = useSvgId('grain');
  return (
    <BackdropFrame
      skin="tidepool"
      scheme="light"
      art={
        <>
          <span className="tp-sparkle tp-sparkle-1" />
          <span className="tp-sparkle tp-sparkle-2" />
          <span className="tp-sparkle tp-sparkle-3" />
        </>
      }
    >
      <div className="tp-shell">
        <nav className="tp-wing tp-wing-start" aria-label={t('player.skinx.menu')}>
          <WingArt drops={[1, 2]} />
          <LibraryButton className="tp-pill">
            <span className="tp-pill-led" aria-hidden="true" />
            <span className="tp-pill-text">{t('player.skinx.library')}</span>
          </LibraryButton>
          <EqSwitch className="tp-pill">
            <span className="tp-pill-led" aria-hidden="true" />
            <span className="tp-pill-text">{t('player.skinx.tidepool.equalizer')}</span>
          </EqSwitch>
          <SkinPill />
        </nav>

        <div className="tp-body">
          <Art className="tp-body-art">
            <svg className="tp-grain" aria-hidden="true">
              <filter id={grain}>
                <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7" />
                <feColorMatrix type="saturate" values="0" />
              </filter>
              <rect width="100%" height="100%" filter={`url(#${grain})`} />
            </svg>
            <span className="tp-body-sheen" />
            <span className="tp-body-glare" />
            <span className="tp-body-rim tp-body-rim-start" />
            <span className="tp-body-rim tp-body-rim-end" />
            <Drop n={3} />
            <Drop n={4} />
            <Drop n={5} />
            <Drop n={6} />
          </Art>

          <div className="tp-crown">
            <Art className="tp-capsule">
              <span className="tp-capsule-drop" />
              <span className="tp-capsule-name">{t('player.skins.tidepool')}</span>
              <span className="tp-capsule-count">
                {n}/{total}
              </span>
            </Art>
            <span className="tp-gems">
              <DockButton className="tp-gem tp-gem-aqua" />
              <CloseButton className="tp-gem tp-gem-red" />
            </span>
          </div>

          <div className="tp-screen">
            <Art className="tp-screen-glare" />
            <div className="tp-screen-grid">
              <Cover />
              <div className="tp-time">
                <TimeReadout />
              </div>
              <div className="tp-title">
                <Ticker text={text} />
              </div>
              <p className="tp-info">
                {format && <span className="tp-info-fmt">{format.toUpperCase()}</span>}
                <span>{t('player.skinx.tidepool.track', { n, total })}</span>
                <span className="tp-info-hi">
                  {eqOn ? t('player.skinx.tidepool.eqOn') : t('player.skinx.tidepool.eqOff')}
                </span>
              </p>
              <Wave />
            </div>
            <div
              className="tp-seek-tube"
              style={{ '--tp-pct': `${pct.toFixed(2)}%` } as React.CSSProperties}
            >
              <SeekBar className="tp-seek" />
            </div>
          </div>

          <div className="tp-deck" role="group" aria-label={t('player.controls')}>
            <div className="tp-slot tp-slot-shuffle">
              <ShuffleButton className="tp-shuffle" />
              <Etch on={shuffle}>{t('player.skinx.shuffleShort')}</Etch>
            </div>
            <div className="tp-slot tp-slot-prev">
              <PrevButton className="tp-cap tp-cap-prev" />
              <Etch on={busy}>{t('player.skinx.tidepool.prev')}</Etch>
            </div>
            <div className="tp-slot tp-slot-play">
              <span className="tp-orb-well">
                <PlayPauseButton className="tp-orb" />
              </span>
              <Etch on={busy}>{t('player.skinx.tidepool.playPause')}</Etch>
            </div>
            <div className="tp-slot tp-slot-next">
              <NextButton className="tp-cap tp-cap-next" />
              <Etch on={busy}>{t('player.skinx.tidepool.next')}</Etch>
            </div>
            <div
              className="tp-slot tp-slot-vol"
              style={{ '--tp-vol': `${volPct}%` } as React.CSSProperties}
            >
              <VolumeControl className="tp-ball" />
              <Etch on={volPct > 0}>{t('player.skinx.tidepool.vol', { pct: volPct })}</Etch>
            </div>
          </div>

          <div className="tp-strip">
            <RepeatChip />
            <StopChip />
            <span className="tp-chip tp-chip-meter" aria-hidden="true">
              <span className="tp-chip-label">{t('player.skinx.tidepool.level')}</span>
              <LedMeter className="tp-meter" segments={12} />
            </span>
          </div>
        </div>

        <section className="tp-wing tp-wing-end" aria-labelledby="tp-queue-h">
          <WingArt drops={[7, 8]} />
          <h2 id="tp-queue-h" className="tp-wing-h">
            {t('player.queue.title')}
          </h2>
          <QueueButtons className="tp-queue" />
          <QueueTotal />
        </section>
      </div>
    </BackdropFrame>
  );
}

export default TidepoolSkin;
