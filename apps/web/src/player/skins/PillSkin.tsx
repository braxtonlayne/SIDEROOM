import { useT } from '../../i18n';
import {
  CloseButton,
  DockButton,
  NextButton,
  PlayPauseButton,
  PrevButton,
  SeekBar,
  SkinButton,
  Ticker,
  TimeReadout,
  VolumeControl,
  useNowPlayingText,
} from '../controls';

/** "Compact Pill / Dock": a slim bar. The title gets the free space; the controls wrap under it on a narrow screen. */
export function PillSkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <section className="sr-ply sr-ply-pill" aria-label={t('player.region')} data-skin="pill">
      <div className="sr-ply-pill-transport">
        <PrevButton className="sr-ply-btn" />
        <PlayPauseButton className="sr-ply-btn sr-ply-btn-main" />
        <NextButton className="sr-ply-btn" />
      </div>
      <div className="sr-ply-pill-mid">
        <Ticker text={text} />
        <SeekBar className="sr-ply-range" />
      </div>
      <div className="sr-ply-pill-end">
        <TimeReadout />
        <VolumeControl className="sr-ply-btn" />
        <SkinButton className="sr-ply-btn" />
        <DockButton className="sr-ply-btn" />
        <CloseButton className="sr-ply-btn" />
      </div>
    </section>
  );
}
