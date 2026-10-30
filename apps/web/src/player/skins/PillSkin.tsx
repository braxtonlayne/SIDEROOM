import { useT } from '../../i18n';
import {
  CloseButton,
  DockButton,
  NextButton,
  PlayPauseButton,
  PrevButton,
  SeekBar,
  SkinSelect,
  Ticker,
  TimeReadout,
  VolumeControl,
  useNowPlayingText,
} from '../controls';

/** "Compact Pill / Dock": one slim row that stays out of the way. */
export function PillSkin() {
  const t = useT();
  const text = useNowPlayingText();
  return (
    <section className="sr-ply sr-ply-pill" aria-label={t('player.region')} data-skin="pill">
      <PrevButton className="sr-ply-btn" />
      <PlayPauseButton className="sr-ply-btn sr-ply-btn-main" />
      <NextButton className="sr-ply-btn" />
      <div className="sr-ply-pill-mid">
        <Ticker text={text} />
        <SeekBar className="sr-ply-range" />
      </div>
      <TimeReadout />
      <VolumeControl className="sr-ply-btn" />
      <SkinSelect className="sr-ply-select" />
      <DockButton className="sr-ply-btn" />
      <CloseButton className="sr-ply-btn" />
    </section>
  );
}
