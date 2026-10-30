/**
 * MUS-SKN-001 "Tidepool Glass": a glossy Y2K chrome-and-aqua body with gel wings. Painted: the pearl chrome
 * shell, the glass glare, the bubbles, the wave on the screen (its height follows the level). Live: the ticker
 * and the time on the screen, the glass seek tube, the gel transport (the big play orb pulses with the level),
 * volume, the left wing's Library, Shuffle and Skin buttons, the right wing's real queue, dock and close.
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
  TimeReadout,
  VolumeControl,
  useNowPlayingText,
} from '../../controls';
import { Art, BackdropFrame } from './frame';
import { LibraryButton, QueueButtons, ShuffleButton, useQueueCount } from './parts';
import './tidepool.css';

function Wave() {
  return (
    <Art className="tp-wave">
      <svg viewBox="0 0 200 60" preserveAspectRatio="none">
        <path className="tp-wave-a" d="M0 30 C25 5 50 5 75 30 S125 55 150 30 S190 8 200 22" />
        <path className="tp-wave-b" d="M0 34 C30 50 55 50 80 32 S130 10 160 30 S190 46 200 38" />
      </svg>
    </Art>
  );
}

export function TidepoolSkin() {
  const t = useT();
  const text = useNowPlayingText();
  const { n, total } = useQueueCount();
  return (
    <BackdropFrame
      skin="tidepool"
      scheme="light"
      art={
        <>
          <span className="tp-glare" />
          <span className="tp-bubble tp-bubble-1" />
          <span className="tp-bubble tp-bubble-2" />
          <span className="tp-bubble tp-bubble-3" />
        </>
      }
    >
      <div className="tp-title">
        <span className="tp-title-pill">
          <Art className="tp-drop" />
          <span className="tp-count">
            {n}/{total}
          </span>
        </span>
        <span className="tp-gems">
          <DockButton className="tp-gem tp-gem-a" />
          <CloseButton className="tp-gem tp-gem-c" />
        </span>
      </div>
      <div className="tp-body">
        <nav className="tp-wing tp-wing-start" aria-label={t('player.skinx.menu')}>
          <LibraryButton className="tp-gel">
            <span>{t('player.skinx.library')}</span>
          </LibraryButton>
          <ShuffleButton className="tp-gel">
            <span>{t('player.skinx.shuffleShort')}</span>
          </ShuffleButton>
          <SkinButton className="tp-gel tp-gel-icon" />
        </nav>
        <div className="tp-core">
          <div className="tp-screen">
            <Ticker text={text} />
            <div className="tp-screen-row">
              <TimeReadout />
              <Wave />
            </div>
            <SeekBar className="tp-seek" />
          </div>
          <div className="tp-transport" role="group" aria-label={t('player.controls')}>
            <RepeatButton className="tp-bubble-btn" />
            <PrevButton className="tp-bubble-btn" />
            <PlayPauseButton className="tp-orb" />
            <NextButton className="tp-bubble-btn" />
            <StopButton className="tp-bubble-btn" />
          </div>
          <div className="tp-vol">
            <VolumeControl className="tp-bubble-btn tp-bubble-sm" />
          </div>
        </div>
        <section className="tp-wing tp-wing-end" aria-labelledby="tp-queue-h">
          <h2 id="tp-queue-h" className="tp-wing-h">
            {t('player.queue.title')}
          </h2>
          <QueueButtons className="tp-queue" showLength={false} />
        </section>
      </div>
    </BackdropFrame>
  );
}

export default TidepoolSkin;
