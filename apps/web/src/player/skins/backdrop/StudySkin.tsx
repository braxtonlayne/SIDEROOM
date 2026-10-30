/**
 * MUS-SKN-001 "Pastel Study Deck": a soft lo-fi study desk. Painted: the pastel shell, the turntable (the record
 * turns while a track plays), the cloud and the notebook rings. Live: the ticker, the time and a rainbow level
 * meter on the little screen, the ribbon seek bar, the round transport keys, the focus timer and the notes pad
 * (through `SkinExtras`: wave 32's shared tools once bound, a working in-memory default until then), the
 * queue, shuffle, repeat, volume, skin, dock and close.
 */
import { useId } from 'react';
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
  TimeReadout,
  VolumeControl,
  useNowPlayingText,
} from '../../controls';
import { useSkinExtras, type SkinExtras } from './extras';
import { Art, BackdropFrame, LedMeter } from './frame';
import { QueueButtons, ShuffleButton } from './parts';
import './study.css';

function Turntable() {
  return (
    <Art className="st-turntable">
      <svg viewBox="0 0 120 100">
        <rect x="2" y="2" width="116" height="96" rx="14" className="st-plinth" />
        <g className="st-record">
          <circle cx="54" cy="50" r="40" className="st-vinyl" />
          <circle cx="54" cy="50" r="31" className="st-groove" />
          <circle cx="54" cy="50" r="22" className="st-groove" />
          <circle cx="54" cy="50" r="14" className="st-label" />
          <path d="M50 46 q4 -6 8 0 q2 6 -4 8 q-6 -2 -4 -8z" className="st-berry" />
        </g>
        <circle cx="54" cy="50" r="2" className="st-spindle" />
        <path d="M104 14 L100 60 L86 74" className="st-arm" />
        <circle cx="104" cy="14" r="6" className="st-arm-base" />
      </svg>
    </Art>
  );
}

function FocusTimer({ extras }: { extras: SkinExtras }) {
  const t = useT();
  const timer = extras.useFocusTimer();
  const r = 15;
  const c = 2 * Math.PI * r;
  return (
    <section className="st-card st-timer" aria-labelledby="st-timer-h" data-segment={timer.segment}>
      <h2 id="st-timer-h" className="st-card-h">
        {t('player.skinx.study.timer')}
      </h2>
      <div className="st-timer-face">
        <span className="st-timer-clock" role="timer" aria-live="off">
          {timer.clock}
        </span>
        <Art className="st-ring">
          <svg viewBox="0 0 40 40">
            <circle cx="20" cy="20" r={r} className="st-ring-bg" />
            <circle
              cx="20"
              cy="20"
              r={r}
              className="st-ring-fg"
              strokeDasharray={`${c * timer.progress} ${c}`}
              transform="rotate(-90 20 20)"
            />
          </svg>
        </Art>
      </div>
      <p className="st-timer-seg">
        {timer.segment === 'focus' ? t('player.skinx.study.focus') : t('player.skinx.study.break')}
      </p>
      <div className="st-timer-btns">
        <button type="button" className="st-chip" onClick={timer.toggle}>
          {timer.running ? t('player.skinx.study.pause') : t('player.skinx.study.start')}
        </button>
        <button type="button" className="st-chip" onClick={timer.skip}>
          {timer.segment === 'focus'
            ? t('player.skinx.study.toBreak')
            : t('player.skinx.study.toFocus')}
        </button>
        <button type="button" className="st-chip" onClick={timer.reset}>
          {t('player.skinx.study.reset')}
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
      <Art className="st-rings" />
      <h2 id={`${id}-h`} className="st-card-h">
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
    </section>
  );
}

/** The extras, keyed by the registration version so a new binding remounts their hooks cleanly. */
function Extras() {
  const { extras, version } = useSkinExtras();
  return (
    <>
      <FocusTimer key={`t${version}`} extras={extras} />
      <Notes key={`n${version}`} extras={extras} />
    </>
  );
}

export function StudySkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <BackdropFrame
      skin="study"
      scheme="light"
      art={
        <>
          <span className="st-dots" />
          <span className="st-cloud" />
        </>
      }
    >
      <div className="st-title">
        <Art className="st-leaf" />
        <span className="st-title-text">{t('player.skins.study')}</span>
        <span className="st-win">
          <SkinButton className="st-dot st-dot-a" />
          <DockButton className="st-dot st-dot-b" />
          <CloseButton className="st-dot st-dot-c" />
        </span>
      </div>
      <div className="st-grid">
        <div className="st-deck">
          <Turntable />
          <div className="st-screen">
            <Ticker text={text} />
            <TimeReadout />
            <LedMeter className="st-leds" segments={10} />
          </div>
        </div>
        <div className="st-play">
          <SeekBar className="st-seek" />
          <div className="st-keys" role="group" aria-label={t('player.controls')}>
            <PrevButton className="st-key st-key-lilac" />
            <PlayPauseButton className="st-key st-key-sage st-key-main" />
            <StopButton className="st-key st-key-peach" />
            <NextButton className="st-key st-key-lilac" />
          </div>
          <div className="st-mods">
            <ShuffleButton className="st-mod" />
            <RepeatButton className="st-mod" />
            <VolumeControl className="st-mod" />
          </div>
        </div>
        <Extras />
        <section className="st-card st-queue-card" aria-labelledby="st-queue-h">
          <h2 id="st-queue-h" className="st-card-h">
            {t('player.queue.title')}
          </h2>
          <QueueButtons className="st-queue" />
        </section>
      </div>
    </BackdropFrame>
  );
}

export default StudySkin;
