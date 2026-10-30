import { useT } from '../../i18n';
import {
  CloseButton,
  DockButton,
  NextButton,
  PlayPauseButton,
  PrevButton,
  RepeatButton,
  SeekBar,
  SkinSelect,
  StopButton,
  Ticker,
  TimeReadout,
  VolumeControl,
  VuBar,
  useNowPlayingText,
} from '../controls';
import { usePlayerStore } from '../store';

/** "Retro MP3 Hardware" (the default): an LCD with a ticker and a VU bar over a row of hardware-style buttons. */
export function RetroSkin() {
  const t = useT();
  const text = useNowPlayingText();
  const status = usePlayerStore((s) => s.status);
  const n = usePlayerStore((s) => s.index + 1);
  const total = usePlayerStore((s) => s.queue.length);
  return (
    <section className="sr-ply sr-ply-retro" aria-label={t('player.region')} data-skin="retro">
      <div className="sr-ply-lcd">
        <div className="sr-ply-lcd-row">
          <span className="sr-ply-state" data-testid="player-state">
            {t(`player.state.${status}`)}
          </span>
          <span className="sr-ply-count">
            {n}/{total}
          </span>
        </div>
        <Ticker text={text} />
        <div className="sr-ply-lcd-row">
          <TimeReadout />
          <VuBar />
        </div>
      </div>
      <SeekBar className="sr-ply-range" />
      <div className="sr-ply-keys" role="group" aria-label={t('player.controls')}>
        <PrevButton className="sr-ply-key" />
        <PlayPauseButton className="sr-ply-key sr-ply-key-main" />
        <StopButton className="sr-ply-key" />
        <NextButton className="sr-ply-key" />
        <RepeatButton className="sr-ply-key" />
      </div>
      <div className="sr-ply-foot">
        <VolumeControl className="sr-ply-key sr-ply-key-small" />
        <SkinSelect className="sr-ply-select" />
        <DockButton className="sr-ply-key sr-ply-key-small" />
        <CloseButton className="sr-ply-key sr-ply-key-small" />
      </div>
    </section>
  );
}
