/**
 * MUS-LIB-001: the content of the "Full Library Manager" skin's Library tab. player-shell owns the frame (the
 * transport, the tabs, the Queue tab) and renders this component in its slot (`registerLibraryPanel`); this
 * file owns everything inside: search, playlists, favorites, recently played and the equalizer.
 */
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import {
  EqSection,
  FavoritesSection,
  PlaylistsSection,
  RecentSection,
  SearchSection,
} from './sections';
import './panel.css';

const SECTIONS = ['search', 'playlists', 'favorites', 'recent', 'eq'] as const;
type Section = (typeof SECTIONS)[number];

const LABELS: Record<Section, MessageKey> = {
  search: 'music.lib.panel.tab.search',
  playlists: 'music.lib.panel.tab.playlists',
  favorites: 'music.lib.panel.tab.favorites',
  recent: 'music.lib.panel.tab.recent',
  eq: 'music.lib.panel.tab.eq',
};

export function LibraryManagerPanel() {
  const t = useT();
  const [section, setSection] = useState<Section>('search');
  const [shuffle, setShuffle] = useState(false);
  const [guildId, setGuildId] = useState<string | null>(null);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
    const i = SECTIONS.indexOf(section);
    const next =
      SECTIONS[(i + (e.key === 'ArrowRight' ? 1 : -1) + SECTIONS.length) % SECTIONS.length]!;
    setSection(next);
    document.getElementById(`sr-mlib-tab-${next}`)?.focus();
  };

  return (
    <div className="sr-mlib">
      <div className="sr-mlib-head">
        <div
          role="tablist"
          aria-label={t('music.lib.panel.sections')}
          className="sr-mlib-tabs"
          onKeyDown={onKey}
        >
          {SECTIONS.map((s) => (
            <button
              key={s}
              type="button"
              role="tab"
              id={`sr-mlib-tab-${s}`}
              aria-selected={section === s}
              aria-controls={`sr-mlib-pane-${s}`}
              tabIndex={section === s ? 0 : -1}
              onClick={() => setSection(s)}
            >
              {t(LABELS[s])}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="sr-mlib-btn"
          aria-pressed={shuffle}
          title={t('music.lib.panel.shuffleHelp')}
          onClick={() => setShuffle(!shuffle)}
        >
          {t('music.lib.panel.shuffle')}
        </button>
      </div>
      <div
        role="tabpanel"
        id={`sr-mlib-pane-${section}`}
        aria-labelledby={`sr-mlib-tab-${section}`}
        className="sr-mlib-pane"
      >
        {section === 'search' && <SearchSection shuffle={shuffle} guildId={guildId} />}
        {section === 'playlists' && (
          <PlaylistsSection shuffle={shuffle} guildId={guildId} onGuild={setGuildId} />
        )}
        {section === 'favorites' && <FavoritesSection shuffle={shuffle} guildId={guildId} />}
        {section === 'recent' && <RecentSection shuffle={shuffle} guildId={guildId} />}
        {section === 'eq' && <EqSection />}
      </div>
    </div>
  );
}
