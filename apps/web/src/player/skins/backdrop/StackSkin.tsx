/**
 * MUS-SKN-001 "Steel Stack": the late-90s stacked desktop player, three docked brushed-steel windows (main,
 * equalizer, playlist) with bolt emblems and gold pinstripes in the title bars.
 *
 * Real: the green LCD time (the position) with the play-state glyph and a level meter from the analyser, the
 * "n. Title — Artist" ticker, the format readout (the file's container when the library knows it; the bitrate,
 * sample-rate and mono/stereo cells stay dark because nothing in the player knows them, rather than invent
 * numbers), volume, the EQ and PL toggles that show and hide those windows, seek, prev, play, pause, stop,
 * next, eject (opens the library), shuffle and repeat. The equalizer window drives the player's 10-band EQ: ON
 * is the equalizer switch, PRESETS the library's presets, each band a vertical slider; its curve is drawn from
 * the gains. The playlist window is the real queue: a click selects, a double-click or Enter plays, Delete or
 * REM removes, SEL selects the playing track, ADD opens the library; the time shows the position against the
 * queue's total length.
 *
 * Painted: the bolt emblems and pinstripes, AUTO (lit: the equalizer's preamp always takes the headroom
 * automatically, so the preamp column only shows that cut), MISC, and the screws.
 */
import { useEffect, useId, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import {
  CloseButton,
  DockButton,
  IconClose,
  IconNext,
  IconPause,
  IconPlay,
  IconPrev,
  IconStop,
  RepeatButton,
  SeekBar,
  SkinButton,
  Ticker,
  VolumeControl,
  formatTime,
  playIndex,
  useNowPlayingText,
} from '../../controls';
import {
  EQ_FREQS,
  EQ_MAX_DB,
  EQ_PRESETS,
  EQ_PRESET_IDS,
  matchPreset,
  normalizeGains,
  preampDb,
} from '../../../music/eq/presets';
import {
  currentTrack,
  next,
  pause,
  play,
  prev,
  removeAt,
  seek,
  setEq,
  usePlayerStore,
} from '../../store';
import { Art, BackdropFrame, LedMeter } from './frame';
import { EqSwitch, IconEject, LibraryButton, ShuffleButton, useQueueCount } from './parts';
import './stack.css';

const PRESET_LABELS: Record<(typeof EQ_PRESET_IDS)[number], MessageKey> = {
  flat: 'music.lib.eq.preset.flat',
  bassBoost: 'music.lib.eq.preset.bassBoost',
  trebleBoost: 'music.lib.eq.preset.trebleBoost',
  vocal: 'music.lib.eq.preset.vocal',
  rock: 'music.lib.eq.preset.rock',
  pop: 'music.lib.eq.preset.pop',
  jazz: 'music.lib.eq.preset.jazz',
  classical: 'music.lib.eq.preset.classical',
  electronic: 'music.lib.eq.preset.electronic',
  loudness: 'music.lib.eq.preset.loudness',
  acoustic: 'music.lib.eq.preset.acoustic',
  smallSpeakers: 'music.lib.eq.preset.smallSpeakers',
};

// ---- Which windows are showing: kept for the tab, so a skin change and back keeps them ----

const windows = { eq: true, pl: true };
const winSubs = new Set<() => void>();
let winSnap = { ...windows };
function setWindow(k: 'eq' | 'pl', open: boolean) {
  windows[k] = open;
  winSnap = { ...windows };
  winSubs.forEach((f) => f());
}
function useWindows() {
  return useSyncExternalStore(
    (f) => {
      winSubs.add(f);
      return () => winSubs.delete(f);
    },
    () => winSnap,
    () => winSnap,
  );
}
/** Test hook. */
export function __resetStackWindows(): void {
  setWindow('eq', true);
  setWindow('pl', true);
}

const hzLabel = (hz: number) => (hz >= 1000 ? `${hz / 1000}K` : String(hz));

function Bolt() {
  return (
    <svg viewBox="0 0 12 16" width="12" height="16">
      <path d="M7 0 1 9h4l-1 7 7-10H7z" className="ss-bolt" />
    </svg>
  );
}

function StackWindow({
  title,
  className,
  buttons,
  children,
}: {
  title: string;
  className: string;
  buttons: ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <section className={`ss-win ${className}`} aria-labelledby={id}>
      <div className="ss-bar">
        <Art className="ss-bar-art">
          <Bolt />
          <span className="ss-stripes" />
        </Art>
        <h2 id={id} className="ss-bar-title">
          {title}
        </h2>
        <Art className="ss-bar-art ss-bar-art-end">
          <span className="ss-stripes" />
        </Art>
        <span className="ss-bar-btns">{buttons}</span>
      </div>
      {children}
    </section>
  );
}

function StateGlyph() {
  const t = useT();
  const status = usePlayerStore((s) => s.status);
  return (
    <span className="ss-glyph" data-testid="player-state" data-state={status}>
      <span aria-hidden="true">
        {status === 'playing' || status === 'loading' ? (
          <IconPlay />
        ) : status === 'paused' ? (
          <IconPause />
        ) : (
          <IconStop />
        )}
      </span>
      <span className="sr-ply-bd-sr">{t(`player.state.${status}`)}</span>
    </span>
  );
}

function Readout({ value, unit }: { value: string | null; unit: string }) {
  return (
    // A dark cell is painted (its unlit segments are generated content), so it reads as nothing at all.
    value === null ? (
      <span className="ss-readout" data-lit="false" data-ghost={'--- ' + unit} aria-hidden="true" />
    ) : (
      <span className="ss-readout" data-lit="true">
        <span className="ss-readout-v">{value}</span> <span className="ss-readout-u">{unit}</span>
      </span>
    )
  );
}

function MainWindow() {
  const t = useT();
  const text = useNowPlayingText();
  const position = usePlayerStore((s) => s.position);
  const format = usePlayerStore((s) => currentTrack(s)?.format);
  const { n } = useQueueCount();
  const wins = useWindows();
  return (
    <StackWindow
      className="ss-main"
      title={t('player.skins.stack')}
      buttons={
        <>
          <SkinButton className="ss-tiny" />
          <DockButton className="ss-tiny" />
          <CloseButton className="ss-tiny" />
        </>
      }
    >
      <div className="ss-main-body">
        <div className="ss-lcd">
          <div className="ss-lcd-top">
            <StateGlyph />
            <span className="ss-lcd-time">{formatTime(position)}</span>
          </div>
          <LedMeter className="ss-leds" segments={16} />
        </div>
        <div className="ss-info">
          <div className="ss-ticker-box">
            {n > 0 && <span className="ss-n">{n}.</span>}
            <Ticker text={text} />
          </div>
          <div className="ss-readouts">
            <Readout value={null} unit={t('player.skinx.stack.kbps')} />
            <Readout value={null} unit={t('player.skinx.stack.khz')} />
            {format ? (
              <span className="ss-readout ss-fmt" data-lit="true">
                <span className="sr-ply-bd-sr">{t('player.skinx.stack.format')} </span>
                {format.toUpperCase()}
              </span>
            ) : (
              <span
                className="ss-readout ss-fmt"
                data-lit="false"
                data-ghost="---"
                aria-hidden="true"
              />
            )}
            <span
              className="ss-readout ss-ch"
              data-lit="false"
              data-ghost={t('player.skinx.stack.mono') + ' ' + t('player.skinx.stack.stereo')}
              aria-hidden="true"
            />
          </div>
          <div className="ss-mid">
            <VolumeControl className="ss-tiny ss-tiny-vol" />
            <button
              type="button"
              className="ss-toggle"
              aria-pressed={wins.eq}
              aria-label={t('player.skinx.stack.eqWindow')}
              onClick={() => setWindow('eq', !wins.eq)}
            >
              <span className="ss-led" aria-hidden="true" />
              {t('player.skinx.stack.eq')}
            </button>
            <button
              type="button"
              className="ss-toggle"
              aria-pressed={wins.pl}
              aria-label={t('player.skinx.stack.plWindow')}
              onClick={() => setWindow('pl', !wins.pl)}
            >
              <span className="ss-led" aria-hidden="true" />
              {t('player.skinx.stack.pl')}
            </button>
          </div>
        </div>
        <SeekBar className="ss-seek" />
        <div className="ss-transport" role="group" aria-label={t('player.controls')}>
          <button type="button" className="ss-key" aria-label={t('player.prev')} onClick={prev}>
            <IconPrev />
          </button>
          <button
            type="button"
            className="ss-key"
            aria-label={t('player.play')}
            onClick={() => play()}
          >
            <IconPlay />
          </button>
          <button type="button" className="ss-key" aria-label={t('player.pause')} onClick={pause}>
            <IconPause />
          </button>
          <button
            type="button"
            className="ss-key"
            aria-label={t('player.stop')}
            onClick={() => {
              pause();
              seek(0);
            }}
          >
            <IconStop />
          </button>
          <button
            type="button"
            className="ss-key"
            aria-label={t('player.next')}
            onClick={() => next()}
          >
            <IconNext />
          </button>
          <LibraryButton className="ss-key ss-key-gap" label={t('player.skinx.eject')}>
            <IconEject />
          </LibraryButton>
          <ShuffleButton className="ss-key ss-key-wide">
            <span className="ss-led" aria-hidden="true" />
            {t('player.skinx.shuffleShort')}
          </ShuffleButton>
          <RepeatButton className="ss-key" />
        </div>
      </div>
    </StackWindow>
  );
}

function Curve({ gains }: { gains: readonly number[] }) {
  const pts = gains
    .map((g, i) => `${4 + (i * 192) / (gains.length - 1)},${20 - (g / EQ_MAX_DB) * 16}`)
    .join(' ');
  return (
    <Art className="ss-curve">
      <svg viewBox="0 0 200 40" preserveAspectRatio="none">
        <path d="M0 20 H200" className="ss-curve-mid" />
        <polyline points={pts} className="ss-curve-line" />
      </svg>
    </Art>
  );
}

function EqWindow() {
  const t = useT();
  const eq = usePlayerStore((s) => s.eq);
  const enabled = usePlayerStore((s) => s.eqEnabled);
  const gains = normalizeGains(eq);
  const preset = matchPreset(gains);
  const pre = preampDb(enabled ? gains : []);
  return (
    <StackWindow
      className="ss-eqwin"
      title={t('player.skinx.stack.eqTitle')}
      buttons={
        <button
          type="button"
          className="ss-tiny"
          aria-label={t('player.skinx.stack.hideEq')}
          onClick={() => setWindow('eq', false)}
        >
          <IconClose />
        </button>
      }
    >
      <div className="ss-eq-top">
        <EqSwitch className="ss-toggle">
          <span className="ss-led" aria-hidden="true" />
          {t('player.skinx.stack.on')}
        </EqSwitch>
        <Art className="ss-toggle ss-auto">
          <span className="ss-led" data-lit="true" />
          {t('player.skinx.stack.auto')}
        </Art>
        <Curve gains={enabled ? gains : gains.map(() => 0)} />
        <select
          className="ss-presets"
          aria-label={t('music.lib.eq.presetLabel')}
          value={preset}
          onChange={(e) => {
            const id = e.target.value as (typeof EQ_PRESET_IDS)[number];
            if (id in EQ_PRESETS) setEq(EQ_PRESETS[id]);
          }}
        >
          {preset === 'custom' && <option value="custom">{t('music.lib.eq.preset.custom')}</option>}
          {EQ_PRESET_IDS.map((id) => (
            <option key={id} value={id}>
              {t(PRESET_LABELS[id])}
            </option>
          ))}
        </select>
      </div>
      <div className="ss-eq-body">
        <Art className="ss-preamp" style={{ '--pre': pre / EQ_MAX_DB } as React.CSSProperties}>
          <span className="ss-preamp-scale">
            <span>{t('music.lib.eq.db', { db: `+${EQ_MAX_DB}` })}</span>
            <span>{t('music.lib.eq.db', { db: '+0' })}</span>
            <span>{t('music.lib.eq.db', { db: -EQ_MAX_DB })}</span>
          </span>
          <span className="ss-preamp-groove">
            <span className="ss-preamp-thumb" />
          </span>
          <span className="ss-preamp-label">{t('player.skinx.stack.preamp')}</span>
        </Art>
        <div className="ss-bands" role="group" aria-label={t('music.lib.eq.bands')}>
          {EQ_FREQS.map((hz, i) => (
            <div key={hz} className="ss-band">
              <input
                type="range"
                min={-EQ_MAX_DB}
                max={EQ_MAX_DB}
                step={1}
                value={gains[i]}
                disabled={!enabled}
                aria-orientation="vertical"
                aria-label={t('music.lib.eq.band', {
                  hz: hz >= 1000 ? `${hz / 1000} kHz` : `${hz} Hz`,
                })}
                aria-valuetext={t('music.lib.eq.db', { db: gains[i] ?? 0 })}
                onChange={(e) => {
                  const nextGains = [...gains];
                  nextGains[i] = Number(e.target.value);
                  setEq(nextGains);
                }}
              />
              <span className="ss-band-hz" aria-hidden="true">
                {hzLabel(hz)}
              </span>
            </div>
          ))}
        </div>
      </div>
    </StackWindow>
  );
}

function PlaylistWindow() {
  const t = useT();
  const queue = usePlayerStore((s) => s.queue);
  const index = usePlayerStore((s) => s.index);
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const [sel, setSel] = useState(() => Math.max(0, index));
  const listId = useId();
  const listRef = useRef<HTMLUListElement>(null);
  const selected = Math.min(sel, queue.length - 1);
  // Keep the selected row in view as the arrow keys move it.
  useEffect(() => {
    if (selected < 0) return;
    document.getElementById(`${listId}-${selected}`)?.scrollIntoView?.({ block: 'nearest' });
  }, [selected, listId]);
  let total = 0;
  let known = 0;
  queue.forEach((tr, i) => {
    const len = i === index && duration > 0 ? duration : tr.durationSec;
    if (len && len > 0) {
      total += len;
      known++;
    }
  });
  const remove = () => {
    if (selected >= 0) removeAt(selected);
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (queue.length === 0) return;
    const go = (to: number) => {
      e.preventDefault();
      setSel(Math.max(0, Math.min(queue.length - 1, to)));
    };
    if (e.key === 'ArrowDown') go(selected + 1);
    else if (e.key === 'ArrowUp') go(selected - 1);
    else if (e.key === 'Home') go(0);
    else if (e.key === 'End') go(queue.length - 1);
    else if (e.key === 'Enter') {
      e.preventDefault();
      playIndex(selected);
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      remove();
    }
  };
  return (
    <StackWindow
      className="ss-plwin"
      title={t('player.skinx.stack.plTitle')}
      buttons={
        <button
          type="button"
          className="ss-tiny"
          aria-label={t('player.skinx.stack.hidePl')}
          onClick={() => setWindow('pl', false)}
        >
          <IconClose />
        </button>
      }
    >
      <div className="ss-pl-screen">
        {queue.length === 0 ? (
          <p className="sr-ply-empty">{t('player.queue.empty')}</p>
        ) : (
          <ul
            ref={listRef}
            role="listbox"
            tabIndex={0}
            className="ss-pl"
            aria-label={t('player.queue.title')}
            aria-describedby={`${listId}-hint`}
            aria-activedescendant={selected >= 0 ? `${listId}-${selected}` : undefined}
            onKeyDown={onKey}
          >
            {queue.map((tr, i) => {
              const len = i === index && duration > 0 ? duration : tr.durationSec;
              return (
                <li
                  key={`${i}:${tr.id}`}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === selected}
                  aria-current={i === index ? 'true' : undefined}
                  data-current={i === index}
                  onClick={() => {
                    setSel(i);
                    listRef.current?.focus();
                  }}
                  onDoubleClick={() => playIndex(i)}
                >
                  <span className="ss-pl-n">{i + 1}.</span>
                  <span className="ss-pl-title">
                    {tr.artist ? `${tr.title} — ${tr.artist}` : tr.title}
                  </span>
                  <span className="ss-pl-len">{len && len > 0 ? formatTime(len) : ''}</span>
                </li>
              );
            })}
          </ul>
        )}
        <span id={`${listId}-hint`} className="sr-ply-bd-sr">
          {t('player.skinx.stack.plHint')}
        </span>
      </div>
      <div className="ss-pl-foot">
        <LibraryButton className="ss-small" label={t('player.skinx.stack.addLabel')}>
          {t('player.skinx.stack.add')}
        </LibraryButton>
        <button
          type="button"
          className="ss-small"
          aria-label={t('player.skinx.stack.remLabel')}
          disabled={queue.length === 0}
          onClick={remove}
        >
          {t('player.skinx.stack.rem')}
        </button>
        <button
          type="button"
          className="ss-small"
          aria-label={t('player.skinx.stack.selLabel')}
          disabled={index < 0}
          onClick={() => setSel(index)}
        >
          {t('player.skinx.stack.sel')}
        </button>
        <Art className="ss-small ss-small-painted">{t('player.skinx.stack.misc')}</Art>
        <span className="ss-pl-time">
          {formatTime(position)}/{known > 0 ? formatTime(total) : '--:--'}
        </span>
      </div>
    </StackWindow>
  );
}

export function StackSkin() {
  const wins = useWindows();
  return (
    <BackdropFrame skin="stack" scheme="dark">
      <MainWindow />
      {wins.eq && <EqWindow />}
      {wins.pl && <PlaylistWindow />}
    </BackdropFrame>
  );
}

export default StackSkin;
