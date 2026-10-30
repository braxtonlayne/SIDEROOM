import { useState } from 'react';
import { useT } from '../../i18n';
import {
  CloseButton,
  DockButton,
  IconClose,
  NextButton,
  PlayPauseButton,
  PrevButton,
  RepeatButton,
  SeekBar,
  SkinButton,
  Ticker,
  TimeReadout,
  VolumeControl,
  playIndex,
  useNowPlayingText,
} from '../controls';
import { removeAt, usePlayerStore } from '../store';

type Tab = 'library' | 'queue';

function QueueList() {
  const t = useT();
  const queue = usePlayerStore((s) => s.queue);
  const index = usePlayerStore((s) => s.index);
  if (queue.length === 0) return <p className="sr-ply-empty">{t('player.queue.empty')}</p>;
  return (
    <ol className="sr-ply-queue" aria-label={t('player.queue.title')}>
      {queue.map((track, i) => (
        <li key={`${i}:${track.id}`} data-current={i === index}>
          <button
            type="button"
            className="sr-ply-queue-play"
            aria-current={i === index ? 'true' : undefined}
            onClick={() => playIndex(i)}
          >
            <span className="sr-ply-queue-n">{i + 1}</span>
            <span className="sr-ply-queue-title">{track.title}</span>
            {track.artist && <span className="sr-ply-queue-artist">{track.artist}</span>}
          </button>
          <button
            type="button"
            className="sr-ply-btn"
            aria-label={t('player.queue.remove', { title: track.title })}
            onClick={() => removeAt(i)}
          >
            <IconClose />
          </button>
        </li>
      ))}
    </ol>
  );
}

/**
 * "Full Library Manager": the frame of a larger panel with a Library tab and a Queue tab. The Library tab's
 * content (library, playlists, search) is the slot music-lib fills through `registerLibraryPanel`.
 */
export function LibrarySkin() {
  const t = useT();
  const text = useNowPlayingText();
  const Panel = usePlayerStore((s) => s.libraryPanel);
  const [tab, setTab] = useState<Tab>('library');
  const tabs: Tab[] = ['library', 'queue'];
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
    const next =
      tabs[(tabs.indexOf(tab) + (e.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length]!;
    setTab(next);
    document.getElementById(`sr-ply-tab-${next}`)?.focus();
  };
  return (
    <section className="sr-ply sr-ply-library" aria-label={t('player.region')} data-skin="library">
      <header className="sr-ply-lib-head">
        <Ticker text={text} />
        <TimeReadout />
      </header>
      <div className="sr-ply-lib-transport">
        <PrevButton className="sr-ply-btn" />
        <PlayPauseButton className="sr-ply-btn sr-ply-btn-main" />
        <NextButton className="sr-ply-btn" />
        <RepeatButton className="sr-ply-btn" />
        <SeekBar className="sr-ply-range" />
        <VolumeControl className="sr-ply-btn" />
        <span className="sr-ply-lib-actions">
          <SkinButton className="sr-ply-btn" />
          <DockButton className="sr-ply-btn" />
          <CloseButton className="sr-ply-btn" />
        </span>
      </div>
      <div role="tablist" aria-label={t('player.panel')} className="sr-ply-tabs" onKeyDown={onKey}>
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`sr-ply-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`sr-ply-pane-${k}`}
            tabIndex={tab === k ? 0 : -1}
            onClick={() => setTab(k)}
          >
            {t(`player.tab.${k}`)}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`sr-ply-pane-${tab}`}
        aria-labelledby={`sr-ply-tab-${tab}`}
        className="sr-ply-pane"
      >
        {tab === 'queue' ? (
          <QueueList />
        ) : Panel ? (
          <Panel />
        ) : (
          <p className="sr-ply-empty">{t('player.library.unavailable')}</p>
        )}
      </div>
    </section>
  );
}
