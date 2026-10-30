import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import { useGuilds } from '../../store/hooks';
import {
  EQ_FREQS,
  EQ_MAX_DB,
  EQ_PRESETS,
  EQ_PRESET_IDS,
  matchPreset,
  normalizeGains,
} from '../eq/presets';
import {
  clearRecent,
  createPlaylist,
  deletePlaylist,
  getPlaylist,
  listGuildPlaylists,
  listOwnPlaylists,
  listRecent,
  moveInPlaylist,
  removeFromPlaylist,
  renamePlaylist,
} from '../playlists/api';
import { getPlayerPort, usePortState } from '../queue/port';
import { fetchLibrary } from '../search/api';
import { TrackList } from './TrackList';
import {
  KEYS,
  useAddToPlaylist,
  useEditablePlaylists,
  useFavorites,
  useToggleFavorite,
} from './data';
import { useMusicActions } from './useMusicActions';
import type { LibraryTrack } from '../queue/types';

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

interface SectionProps {
  shuffle: boolean;
  /** The server whose playlists are offered for "add to playlist" (null for none). */
  guildId: string | null;
}

function Status({ loading, error }: { loading: boolean; error: unknown }) {
  const t = useT();
  if (loading) return <p className="sr-mlib-note">{t('music.lib.panel.loading')}</p>;
  if (error)
    return (
      <p className="sr-mlib-note" role="alert">
        {t('music.lib.panel.error')}
      </p>
    );
  return null;
}

/** Shared wiring of a track list: favorites, queue actions and the playlist picker. */
function useListProps(p: SectionProps) {
  const act = useMusicActions(p.shuffle);
  const fav = useFavorites();
  const toggle = useToggleFavorite();
  const pls = useEditablePlaylists(p.guildId);
  const add = useAddToPlaylist();
  return {
    favorites: fav.ids,
    playlists: pls.all.map((x) => ({ id: x.id, name: x.name })),
    onEnqueue: (tr: LibraryTrack, next: boolean) => act.enqueue([tr], next),
    onSmart: (k: Parameters<typeof act.playSmart>[0], tr: LibraryTrack) =>
      void act.playSmart(k, tr),
    onToggleFavorite: (tr: LibraryTrack) => toggle.mutate({ track: tr, on: !fav.ids.has(tr.id) }),
    onAddToPlaylist: (playlistId: string, track: LibraryTrack) => add.mutate({ playlistId, track }),
    act,
  };
}

// ------------------------------------------------------------------------------------------- search

export function SearchSection(p: SectionProps) {
  const t = useT();
  const [text, setText] = useState('');
  const [q, setQ] = useState('');
  useEffect(() => {
    const h = setTimeout(() => setQ(text), 250);
    return () => clearTimeout(h);
  }, [text]);
  const res = useQuery({
    queryKey: ['music', 'search', q],
    queryFn: () => fetchLibrary({ ...(q.trim() ? { q } : {}), limit: 50 }),
    staleTime: 30_000,
  });
  const lp = useListProps(p);
  const tracks = res.data?.items ?? [];
  return (
    <div role="search" className="sr-mlib-sect">
      <input
        type="search"
        className="sr-mlib-input"
        value={text}
        maxLength={100}
        placeholder={t('music.lib.search.placeholder')}
        aria-label={t('music.lib.search.label')}
        onChange={(e) => setText(e.target.value)}
      />
      <Status loading={res.isLoading} error={res.error} />
      {res.data && tracks.length === 0 && (
        <p className="sr-mlib-note">
          {q.trim() ? t('music.lib.search.none', { q }) : t('music.lib.search.empty')}
        </p>
      )}
      {tracks.length > 0 && (
        <>
          <p className="sr-mlib-note" aria-live="polite">
            {t('music.lib.search.count', { n: res.data?.total ?? tracks.length })}
          </p>
          <div className="sr-mlib-bar">
            <button type="button" className="sr-mlib-btn" onClick={() => lp.act.play(tracks, 0)}>
              {t('music.lib.panel.playAll')}
            </button>
          </div>
          <TrackList
            label={t('music.lib.search.results')}
            tracks={tracks}
            onPlay={(i) => lp.act.play(tracks, i)}
            {...lp}
          />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------- favorites

export function FavoritesSection(p: SectionProps) {
  const t = useT();
  const fav = useFavorites();
  const lp = useListProps(p);
  const tracks = useMemo(() => fav.data?.items.map((i) => i.track) ?? [], [fav.data]);
  return (
    <div className="sr-mlib-sect">
      <Status loading={fav.isLoading} error={fav.error} />
      {fav.data && tracks.length === 0 && (
        <p className="sr-mlib-note">{t('music.lib.favorites.empty')}</p>
      )}
      {(fav.data?.hidden ?? 0) > 0 && (
        <p className="sr-mlib-note">{t('music.lib.panel.hidden', { n: fav.data?.hidden ?? 0 })}</p>
      )}
      {tracks.length > 0 && (
        <>
          <div className="sr-mlib-bar">
            <button type="button" className="sr-mlib-btn" onClick={() => lp.act.play(tracks, 0)}>
              {t('music.lib.panel.playAll')}
            </button>
          </div>
          <TrackList
            label={t('music.lib.favorites.list')}
            tracks={tracks}
            onPlay={(i) => lp.act.play(tracks, i)}
            {...lp}
          />
        </>
      )}
    </div>
  );
}

// ------------------------------------------------------------------------------------------ recent

export function RecentSection(p: SectionProps) {
  const t = useT();
  const qc = useQueryClient();
  const rec = useQuery({ queryKey: KEYS.recent, queryFn: () => listRecent(50), staleTime: 10_000 });
  const clear = useMutation({
    mutationFn: clearRecent,
    onSettled: () => qc.invalidateQueries({ queryKey: KEYS.recent }),
  });
  const lp = useListProps(p);
  const tracks = useMemo(() => rec.data?.items.map((i) => i.track) ?? [], [rec.data]);
  return (
    <div className="sr-mlib-sect">
      <Status loading={rec.isLoading} error={rec.error} />
      {rec.data && tracks.length === 0 && (
        <p className="sr-mlib-note">{t('music.lib.recent.empty')}</p>
      )}
      {tracks.length > 0 && (
        <>
          <div className="sr-mlib-bar">
            <button type="button" className="sr-mlib-btn" onClick={() => lp.act.play(tracks, 0)}>
              {t('music.lib.panel.playAll')}
            </button>
            <button type="button" className="sr-mlib-btn" onClick={() => clear.mutate()}>
              {t('music.lib.recent.clear')}
            </button>
          </div>
          <TrackList
            label={t('music.lib.recent.list')}
            tracks={tracks}
            onPlay={(i) => lp.act.play(tracks, i)}
            {...lp}
          />
        </>
      )}
    </div>
  );
}

// ----------------------------------------------------------------------------------------- playlists

export function PlaylistsSection(p: SectionProps & { onGuild(id: string | null): void }) {
  const t = useT();
  const qc = useQueryClient();
  const guilds = useGuilds();
  const [openId, setOpenId] = useState<string | null>(null);
  const [name, setName] = useState('');
  const own = useQuery({ queryKey: KEYS.own, queryFn: listOwnPlaylists, staleTime: 30_000 });
  const guild = useQuery({
    queryKey: p.guildId ? KEYS.guild(p.guildId) : ['music', 'playlists', 'guild', 'none'],
    queryFn: () => listGuildPlaylists(p.guildId as string),
    enabled: p.guildId !== null,
    staleTime: 30_000,
  });
  const make = useMutation({
    mutationFn: (n: string) => createPlaylist(n, p.guildId ?? undefined),
    onSuccess: (pl) => {
      setName('');
      setOpenId(pl.id);
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ['music', 'playlists'] }),
  });
  const lists = p.guildId ? (guild.data ?? []) : (own.data ?? []);

  if (openId) {
    return (
      <PlaylistDetailView
        id={openId}
        shuffle={p.shuffle}
        guildId={p.guildId}
        onBack={() => setOpenId(null)}
      />
    );
  }
  return (
    <div className="sr-mlib-sect">
      <div className="sr-mlib-bar">
        <label className="sr-mlib-lbl">
          {t('music.lib.playlists.whose')}
          <select
            className="sr-mlib-select"
            value={p.guildId ?? ''}
            onChange={(e) => p.onGuild(e.target.value || null)}
          >
            <option value="">{t('music.lib.playlists.mine')}</option>
            {guilds.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      <Status loading={own.isLoading || guild.isLoading} error={own.error ?? guild.error} />
      <form
        className="sr-mlib-bar"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) make.mutate(name.trim());
        }}
      >
        <input
          className="sr-mlib-input"
          value={name}
          maxLength={100}
          onChange={(e) => setName(e.target.value)}
          aria-label={t('music.lib.playlists.newName')}
          placeholder={t('music.lib.playlists.newName')}
        />
        <button type="submit" className="sr-mlib-btn" disabled={!name.trim() || make.isPending}>
          {t('music.lib.playlists.create')}
        </button>
      </form>
      {make.isError && (
        <p className="sr-mlib-note" role="alert">
          {t('music.lib.playlists.createFailed')}
        </p>
      )}
      {lists.length === 0 && !own.isLoading && (
        <p className="sr-mlib-note">{t('music.lib.playlists.empty')}</p>
      )}
      <ul className="sr-mlib-list" aria-label={t('music.lib.playlists.list')}>
        {lists.map((pl) => (
          <li key={pl.id} className="sr-mlib-row">
            <button type="button" className="sr-mlib-main" onClick={() => setOpenId(pl.id)}>
              <span className="sr-mlib-title">{pl.name}</span>
              <span className="sr-mlib-sub">
                {t('music.lib.playlists.count', { n: pl.item_count })}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function PlaylistDetailView(p: {
  id: string;
  shuffle: boolean;
  guildId: string | null;
  onBack(): void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const d = useQuery({ queryKey: KEYS.playlist(p.id), queryFn: () => getPlaylist(p.id) });
  const lp = useListProps({ shuffle: p.shuffle, guildId: p.guildId });
  const [renaming, setRenaming] = useState<string | null>(null);
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: KEYS.playlist(p.id) });
    void qc.invalidateQueries({ queryKey: ['music', 'playlists'] });
  };
  const remove = useMutation({
    mutationFn: (itemId: string) => removeFromPlaylist(p.id, itemId),
    onSettled: refresh,
  });
  const move = useMutation({
    mutationFn: ({ itemId, to }: { itemId: string; to: number }) =>
      moveInPlaylist(p.id, itemId, to),
    onSettled: refresh,
  });
  const rename = useMutation({
    mutationFn: (n: string) => renamePlaylist(p.id, n),
    onSuccess: () => setRenaming(null),
    onSettled: refresh,
  });
  const drop = useMutation({
    mutationFn: () => deletePlaylist(p.id),
    onSuccess: p.onBack,
    onSettled: refresh,
  });
  const detail = d.data;
  const tracks = useMemo(() => detail?.items.map((i) => i.track) ?? [], [detail]);
  const can = detail?.playlist.can_edit ?? false;
  return (
    <div className="sr-mlib-sect">
      <div className="sr-mlib-bar">
        <button type="button" className="sr-mlib-btn" onClick={p.onBack}>
          {t('music.lib.playlists.back')}
        </button>
      </div>
      <Status loading={d.isLoading} error={d.error} />
      {detail && (
        <>
          {renaming === null ? (
            <h3 className="sr-mlib-h">{detail.playlist.name}</h3>
          ) : (
            <form
              className="sr-mlib-bar"
              onSubmit={(e) => {
                e.preventDefault();
                if (renaming.trim()) rename.mutate(renaming.trim());
              }}
            >
              <input
                className="sr-mlib-input"
                value={renaming}
                maxLength={100}
                aria-label={t('music.lib.playlists.renameLabel')}
                onChange={(e) => setRenaming(e.target.value)}
              />
              <button type="submit" className="sr-mlib-btn">
                {t('music.lib.playlists.save')}
              </button>
            </form>
          )}
          <div className="sr-mlib-bar">
            <button
              type="button"
              className="sr-mlib-btn"
              disabled={tracks.length === 0}
              onClick={() => lp.act.play(tracks, 0)}
            >
              {t('music.lib.panel.playAll')}
            </button>
            {can && renaming === null && (
              <button
                type="button"
                className="sr-mlib-btn"
                onClick={() => setRenaming(detail.playlist.name)}
              >
                {t('music.lib.playlists.rename')}
              </button>
            )}
            {can && (
              <button type="button" className="sr-mlib-btn" onClick={() => drop.mutate()}>
                {t('music.lib.playlists.delete')}
              </button>
            )}
          </div>
          {detail.hidden > 0 && (
            <p className="sr-mlib-note">{t('music.lib.panel.hidden', { n: detail.hidden })}</p>
          )}
          {tracks.length === 0 ? (
            <p className="sr-mlib-note">{t('music.lib.playlists.emptyList')}</p>
          ) : (
            <TrackList
              label={detail.playlist.name}
              tracks={tracks}
              onPlay={(i) => lp.act.play(tracks, i)}
              {...lp}
              rowExtra={
                can
                  ? (tr, i) => {
                      const item = detail.items[i];
                      if (!item) return null;
                      return (
                        <>
                          <button
                            type="button"
                            className="sr-mlib-btn"
                            disabled={i === 0}
                            aria-label={t('music.lib.playlists.moveUp', { title: tr.title })}
                            onClick={() =>
                              move.mutate({ itemId: item.itemId, to: item.position - 1 })
                            }
                          >
                            ↑
                          </button>
                          <button
                            type="button"
                            className="sr-mlib-btn"
                            disabled={i === tracks.length - 1}
                            aria-label={t('music.lib.playlists.moveDown', { title: tr.title })}
                            onClick={() =>
                              move.mutate({ itemId: item.itemId, to: item.position + 1 })
                            }
                          >
                            ↓
                          </button>
                          <button
                            type="button"
                            className="sr-mlib-btn"
                            aria-label={t('music.lib.playlists.removeItem', { title: tr.title })}
                            onClick={() => remove.mutate(item.itemId)}
                          >
                            ✕
                          </button>
                        </>
                      );
                    }
                  : undefined
              }
            />
          )}
        </>
      )}
    </div>
  );
}

// ------------------------------------------------------------------------------------------------ eq

export function EqSection() {
  const t = useT();
  const s = usePortState();
  const port = getPlayerPort();
  const gains = normalizeGains(s.eq);
  const preset = matchPreset(gains);
  return (
    <div className="sr-mlib-sect">
      <div className="sr-mlib-bar">
        <label className="sr-mlib-lbl">
          <input
            type="checkbox"
            checked={s.eqEnabled}
            onChange={(e) => port.setEqEnabled(e.target.checked)}
          />
          {t('music.lib.eq.enabled')}
        </label>
        <label className="sr-mlib-lbl">
          {t('music.lib.eq.presetLabel')}
          <select
            className="sr-mlib-select"
            value={preset}
            onChange={(e) => {
              const id = e.target.value as (typeof EQ_PRESET_IDS)[number];
              if (id in EQ_PRESETS) port.setEq(EQ_PRESETS[id]);
            }}
          >
            {preset === 'custom' && (
              <option value="custom">{t('music.lib.eq.preset.custom')}</option>
            )}
            {EQ_PRESET_IDS.map((id) => (
              <option key={id} value={id}>
                {t(PRESET_LABELS[id])}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="sr-mlib-eq" role="group" aria-label={t('music.lib.eq.bands')}>
        {EQ_FREQS.map((hz, i) => (
          <label key={hz} className="sr-mlib-band">
            <span className="sr-mlib-bandhz">{hz >= 1000 ? `${hz / 1000}k` : hz}</span>
            <input
              type="range"
              min={-EQ_MAX_DB}
              max={EQ_MAX_DB}
              step={1}
              value={gains[i]}
              disabled={!s.eqEnabled}
              aria-label={t('music.lib.eq.band', {
                hz: hz >= 1000 ? `${hz / 1000} kHz` : `${hz} Hz`,
              })}
              aria-valuetext={t('music.lib.eq.db', { db: gains[i] ?? 0 })}
              onChange={(e) => {
                const next = [...gains];
                next[i] = Number(e.target.value);
                port.setEq(next);
              }}
            />
            <span className="sr-mlib-banddb">{gains[i]}</span>
          </label>
        ))}
      </div>
      <label className="sr-mlib-lbl sr-mlib-xfade">
        {t('music.lib.eq.crossfade')}
        <input
          type="range"
          min={0}
          max={12}
          step={0.5}
          value={s.crossfadeSec}
          aria-label={t('music.lib.eq.crossfade')}
          aria-valuetext={t('music.lib.eq.seconds', { n: s.crossfadeSec })}
          onChange={(e) => port.setCrossfade(Number(e.target.value))}
        />
        <span>{t('music.lib.eq.seconds', { n: s.crossfadeSec })}</span>
      </label>
    </div>
  );
}
