import { useState, type ReactNode } from 'react';
import { useT } from '../../i18n';
import { formatDuration } from './format';
import type { SmartKind } from '../queue/smart';
import type { LibraryTrack } from '../queue/types';

export interface TrackListProps {
  tracks: readonly LibraryTrack[];
  /** Ids of favorited tracks (for the star). */
  favorites: ReadonlySet<string>;
  onPlay(index: number): void;
  onEnqueue(track: LibraryTrack, next: boolean): void;
  onSmart(kind: SmartKind, track: LibraryTrack): void;
  onToggleFavorite(track: LibraryTrack): void;
  /** Playlists the member may add to; empty hides the picker. */
  playlists?: readonly { id: string; name: string }[];
  onAddToPlaylist?(playlistId: string, track: LibraryTrack): void;
  /** Extra buttons of a row (remove, move up and down in a playlist). */
  rowExtra?(track: LibraryTrack, index: number): ReactNode;
  label: string;
}

/** A list of tracks with the actions the library offers on every row. */
export function TrackList(p: TrackListProps) {
  const t = useT();
  const [open, setOpen] = useState<string | null>(null);
  return (
    <ol className="sr-mlib-list" aria-label={p.label}>
      {p.tracks.map((tr, i) => (
        <li key={`${tr.id}:${i}`} className="sr-mlib-row">
          <button
            type="button"
            className="sr-mlib-main"
            onClick={() => p.onPlay(i)}
            aria-label={t('music.lib.row.play', { title: tr.title })}
          >
            <span className="sr-mlib-title">{tr.title}</span>
            <span className="sr-mlib-sub">{[tr.artist, tr.album].filter(Boolean).join(' · ')}</span>
          </button>
          <span className="sr-mlib-dur">{tr.durationMs ? formatDuration(tr.durationMs) : ''}</span>
          <button
            type="button"
            className="sr-mlib-btn"
            aria-pressed={p.favorites.has(tr.id)}
            aria-label={t('music.lib.row.favorite', { title: tr.title })}
            onClick={() => p.onToggleFavorite(tr)}
          >
            {p.favorites.has(tr.id) ? '★' : '☆'}
          </button>
          <button
            type="button"
            className="sr-mlib-btn"
            aria-expanded={open === `${tr.id}:${i}`}
            aria-label={t('music.lib.row.more', { title: tr.title })}
            onClick={() => setOpen(open === `${tr.id}:${i}` ? null : `${tr.id}:${i}`)}
          >
            …
          </button>
          {p.rowExtra?.(tr, i)}
          {open === `${tr.id}:${i}` && (
            <div
              className="sr-mlib-more"
              role="group"
              aria-label={t('music.lib.row.moreGroup', { title: tr.title })}
            >
              <button type="button" className="sr-mlib-btn" onClick={() => p.onEnqueue(tr, true)}>
                {t('music.lib.row.playNext')}
              </button>
              <button type="button" className="sr-mlib-btn" onClick={() => p.onEnqueue(tr, false)}>
                {t('music.lib.row.addToQueue')}
              </button>
              <button type="button" className="sr-mlib-btn" onClick={() => p.onSmart('album', tr)}>
                {t('music.lib.row.playAlbum')}
              </button>
              <button type="button" className="sr-mlib-btn" onClick={() => p.onSmart('artist', tr)}>
                {t('music.lib.row.playArtist')}
              </button>
              <button type="button" className="sr-mlib-btn" onClick={() => p.onSmart('genre', tr)}>
                {t('music.lib.row.playGenre')}
              </button>
              <button
                type="button"
                className="sr-mlib-btn"
                onClick={() => p.onSmart('similar', tr)}
              >
                {t('music.lib.row.moreLikeThis')}
              </button>
              {p.playlists && p.playlists.length > 0 && p.onAddToPlaylist && (
                <select
                  className="sr-mlib-select"
                  aria-label={t('music.lib.row.addToPlaylist', { title: tr.title })}
                  value=""
                  onChange={(e) => {
                    if (e.target.value) p.onAddToPlaylist?.(e.target.value, tr);
                  }}
                >
                  <option value="">{t('music.lib.row.addToPlaylistPick')}</option>
                  {p.playlists.map((pl) => (
                    <option key={pl.id} value={pl.id}>
                      {pl.name}
                    </option>
                  ))}
                </select>
              )}
            </div>
          )}
        </li>
      ))}
    </ol>
  );
}
