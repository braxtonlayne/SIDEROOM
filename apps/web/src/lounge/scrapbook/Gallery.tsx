import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useChannels } from '../data';
import {
  getGallery,
  presignObject,
  removeEntry,
  setTags,
  type GalleryFilter,
  type ScrapbookEntry,
  type ScrapbookFacets,
  type ScrapbookObject,
} from './api';
import { ShelfPanel } from './ShelfPanel';
import { scrapbookOn, useGuildScrapbook } from './store';
import './scrapbook.css';

/** The tags of an input line: comma separated, `#` optional. The server cleans them again. */
export const parseTags = (line: string): string[] =>
  line
    .split(',')
    .map((s) => s.trim().replace(/^#/, ''))
    .filter(Boolean);

/**
 * One copied file. The signature is asked for when the tile appears and lives only in this component's state:
 * it is never stored, logged or sent anywhere else. A file the viewer's storage scopes do not reach (no view
 * grant for its kind on the server's shelf) shows "not shared with you" and the entry's text stays readable.
 */
export function ObjectMedia({
  guildId,
  entryId,
  object,
}: {
  guildId: string;
  entryId: string;
  object: ScrapbookObject;
}) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(!object.playable);
  useEffect(() => {
    if (!object.playable) return;
    let live = true;
    presignObject(guildId, entryId, object.id)
      .then((r) => live && setUrl(r.url))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [guildId, entryId, object.id, object.playable]);

  if (failed)
    return (
      <p className="lng-scr-locked" data-testid="scr-locked">
        {t('lounge.scrapbook.notShared', { name: object.filename })}
      </p>
    );
  if (!url) return <p className="lng-scr-loading">{t('lounge.scrapbook.loading')}</p>;
  if (object.kind === 'image')
    return (
      <img
        className="lng-scr-media"
        src={url}
        alt={object.filename}
        loading="lazy"
        onError={() => setFailed(true)}
      />
    );
  if (object.kind === 'video')
    return (
      <video
        className="lng-scr-media"
        src={url}
        controls
        preload="metadata"
        aria-label={object.filename}
        onError={() => setFailed(true)}
      />
    );
  if (object.kind === 'audio')
    return (
      <audio
        className="lng-scr-audio"
        src={url}
        controls
        preload="none"
        aria-label={object.filename}
        onError={() => setFailed(true)}
      />
    );
  return (
    <a className="lng-scr-file" href={url} target="_blank" rel="noopener noreferrer">
      {object.filename}
    </a>
  );
}

function dateLabel(ms: number, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: 'UTC' }).format(ms);
}

export function EntryTile({
  guildId,
  entry,
  channelName,
  onChanged,
  onRemoved,
}: {
  guildId: string;
  entry: ScrapbookEntry;
  channelName: string;
  onChanged: (e: ScrapbookEntry) => void;
  onRemoved: (id: string, copiesFailed: number) => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [line, setLine] = useState(entry.tags.join(', '));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);

  const saveTags = async () => {
    setBusy(true);
    setError(false);
    try {
      onChanged(await setTags(guildId, entry.id, parseTags(line)));
      setEditing(false);
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!window.confirm(t('lounge.scrapbook.removeConfirm'))) return;
    setBusy(true);
    setError(false);
    try {
      const r = await removeEntry(guildId, entry.id);
      onRemoved(entry.id, r.copies_failed);
    } catch {
      setError(true);
      setBusy(false);
    }
  };

  return (
    <article className="lng-scr-tile" data-testid="scr-entry" aria-label={entry.author_name}>
      {entry.objects.map((o) => (
        <ObjectMedia key={o.id} guildId={guildId} entryId={entry.id} object={o} />
      ))}
      {entry.content ? <p className="lng-scr-text">{entry.content}</p> : null}
      {entry.skipped > 0 ? (
        <p className="lng-scr-note">{t('lounge.scrapbook.skipped', { n: entry.skipped })}</p>
      ) : null}
      <p className="lng-scr-meta">
        <strong>{entry.author_name}</strong>
        <span>
          {dateLabel(entry.sent_at)} · #{channelName}
        </span>
      </p>
      {entry.tags.length > 0 ? (
        <ul className="lng-scr-tags" aria-label={t('lounge.scrapbook.tags')}>
          {entry.tags.map((tag) => (
            <li key={tag}>#{tag}</li>
          ))}
        </ul>
      ) : null}
      {entry.can_edit ? (
        <div className="lng-scr-actions">
          {editing ? (
            <>
              <input
                className="lng-scr-input"
                value={line}
                aria-label={t('lounge.scrapbook.tagsInput')}
                onChange={(e) => setLine(e.target.value)}
              />
              <Button onClick={() => void saveTags()} disabled={busy}>
                {t('lounge.scrapbook.saveTags')}
              </Button>
            </>
          ) : (
            <Button variant="ghost" onClick={() => setEditing(true)}>
              {t('lounge.scrapbook.editTags')}
            </Button>
          )}
          <Button variant="ghost" onClick={() => void remove()} disabled={busy}>
            {t('lounge.scrapbook.remove')}
          </Button>
        </div>
      ) : null}
      {error ? (
        <p className="lng-scr-note" role="alert">
          {t('lounge.scrapbook.actionFailed')}
        </p>
      ) : null}
    </article>
  );
}

/** The filter bar: date range, channel, tag and member. */
export function Filters({
  facets,
  channelNames,
  value,
  onChange,
}: {
  facets: ScrapbookFacets | undefined;
  channelNames: Record<string, string>;
  value: GalleryFilter;
  onChange: (f: GalleryFilter) => void;
}) {
  const t = useT();
  const set = (patch: Partial<GalleryFilter>) => {
    const next: GalleryFilter = { ...value, ...patch };
    for (const k of Object.keys(next) as (keyof GalleryFilter)[]) if (!next[k]) delete next[k];
    onChange(next);
  };
  return (
    <form className="lng-scr-filters" onSubmit={(e) => e.preventDefault()}>
      <label>
        {t('lounge.scrapbook.from')}
        <input
          type="date"
          className="lng-scr-input"
          value={value.from ?? ''}
          onChange={(e) => set({ from: e.target.value })}
        />
      </label>
      <label>
        {t('lounge.scrapbook.to')}
        <input
          type="date"
          className="lng-scr-input"
          value={value.to ?? ''}
          onChange={(e) => set({ to: e.target.value })}
        />
      </label>
      <label>
        {t('lounge.scrapbook.channel')}
        <select
          className="lng-scr-input"
          value={value.channelId ?? ''}
          onChange={(e) => set({ channelId: e.target.value })}
        >
          <option value="">{t('lounge.scrapbook.any')}</option>
          {(facets?.channels ?? []).map((c) => (
            <option key={c.id} value={c.id}>
              #{channelNames[c.id] ?? c.id} ({c.count})
            </option>
          ))}
        </select>
      </label>
      <label>
        {t('lounge.scrapbook.tag')}
        <select
          className="lng-scr-input"
          value={value.tag ?? ''}
          onChange={(e) => set({ tag: e.target.value })}
        >
          <option value="">{t('lounge.scrapbook.any')}</option>
          {(facets?.tags ?? []).map((x) => (
            <option key={x.tag} value={x.tag}>
              #{x.tag} ({x.count})
            </option>
          ))}
        </select>
      </label>
      <label>
        {t('lounge.scrapbook.member')}
        <select
          className="lng-scr-input"
          value={value.authorId ?? ''}
          onChange={(e) => set({ authorId: e.target.value })}
        >
          <option value="">{t('lounge.scrapbook.any')}</option>
          {(facets?.authors ?? []).map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} ({a.count})
            </option>
          ))}
        </select>
      </label>
    </form>
  );
}

/** The masonry gallery of a server's scrapbook (`/channels/:guildId/scrapbook`). */
export function ScrapbookGallery({ guildId }: { guildId: string }) {
  const t = useT();
  const status = useGuildScrapbook(guildId);
  const channels = useChannels(guildId);
  const channelNames = useMemo(
    () => Object.fromEntries(channels.map((c) => [c.id, c.name])),
    [channels],
  );
  const [filter, setFilter] = useState<GalleryFilter>({});
  const [entries, setEntries] = useState<ScrapbookEntry[]>([]);
  const [facets, setFacets] = useState<ScrapbookFacets | undefined>();
  const [next, setNext] = useState<string | undefined>();
  const [state, setState] = useState<'loading' | 'ready' | 'failed'>('loading');
  const [notice, setNotice] = useState(false);
  const on = scrapbookOn(status);

  const load = useCallback(
    async (f: GalleryFilter, before?: string) => {
      setState('loading');
      try {
        const page = await getGallery(guildId, f, before, !before);
        setEntries((old) => (before ? [...old, ...page.entries] : page.entries));
        setNext(page.next_before);
        if (page.facets) setFacets(page.facets);
        setState('ready');
      } catch {
        setState('failed');
      }
    },
    [guildId],
  );

  useEffect(() => {
    if (on) void load(filter);
  }, [on, filter, load]);

  if (status === undefined)
    return <p className="lng-scr-loading">{t('lounge.scrapbook.loading')}</p>;
  if (!on)
    return (
      <section className="lng-scr-page" aria-label={t('lounge.scrapbook.title')}>
        <h2>{t('lounge.scrapbook.title')}</h2>
        <p className="lng-scr-note" data-testid="scr-off">
          {t('lounge.scrapbook.off')}
        </p>
      </section>
    );

  return (
    <section className="lng-scr-page" aria-label={t('lounge.scrapbook.title')}>
      <header className="lng-scr-head">
        <h2>{t('lounge.scrapbook.title')}</h2>
        <p className="lng-scr-note">
          {t('lounge.scrapbook.howItWorks', {
            emoji: status.emojis.join(' '),
            n: status.threshold,
          })}
        </p>
      </header>
      {status.can_manage ? <ShelfPanel guildId={guildId} /> : null}
      <Filters
        facets={facets}
        channelNames={channelNames}
        value={filter}
        onChange={(f) => setFilter(f)}
      />
      {notice ? (
        <p className="lng-scr-note" role="status" data-testid="scr-notice">
          {t('lounge.scrapbook.copyPending')}
        </p>
      ) : null}
      {state === 'failed' ? (
        <p className="lng-scr-note" role="alert">
          {t('lounge.scrapbook.loadFailed')}
        </p>
      ) : null}
      {state === 'ready' && entries.length === 0 ? (
        <p className="lng-scr-note" data-testid="scr-empty">
          {t('lounge.scrapbook.empty')}
        </p>
      ) : null}
      <div className="lng-scr-grid" data-testid="scr-grid">
        {entries.map((e) => (
          <EntryTile
            key={e.id}
            guildId={guildId}
            entry={e}
            channelName={channelNames[e.channel_id] ?? e.channel_id}
            onChanged={(u) => setEntries((old) => old.map((x) => (x.id === u.id ? u : x)))}
            onRemoved={(id, failed) => {
              setEntries((old) => old.filter((x) => x.id !== id));
              if (failed > 0) setNotice(true);
            }}
          />
        ))}
      </div>
      {next ? (
        <Button onClick={() => void load(filter, next)} disabled={state === 'loading'}>
          {t('lounge.scrapbook.more')}
        </Button>
      ) : null}
    </section>
  );
}
