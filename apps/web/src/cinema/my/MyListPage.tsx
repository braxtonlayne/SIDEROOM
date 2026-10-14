// My list (CIN-MY-001, spec cinema-media 2.4): saved titles, continue watching, custom lists and watch history
// with pause and clear. Posters link to the title page (`/cinema/t/:id`, owned by the browse UI) and straight
// into the player for continue watching.
import { Button, ConfirmDialog, EmptyState, PosterCard, Spinner, Switch, Tabs } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { t, useT } from '../../i18n';
import { watchHref } from '../player/model';
import '../player/player.css';
import {
  clearHistory,
  getContinue,
  getHistory,
  getHistorySettings,
  getList,
  getLists,
  getSaved,
  keys,
  removeSaved,
  setHistoryPaused,
} from './api';
import type { CinemaKind, EpisodeRef, ListVisibility, TitleRef } from './types';

const KIND_LABEL: Record<CinemaKind, string> = {
  movie: 'cinema.my.kindFilm',
  series: 'cinema.my.kindSeries',
  anime: 'cinema.my.kindAnime',
};
const titleLine = (ref: TitleRef) =>
  [ref.year, t(KIND_LABEL[ref.kind])].filter((x) => x !== null && x !== undefined).join(' · ');
const titleHref = (id: string) => `/cinema/t/${encodeURIComponent(id)}`;
const episodeLine = (e: EpisodeRef) =>
  `S${e.season} · E${e.number}${e.title ? ` · ${e.title}` : ''}`;
const poster = (t: TitleRef) => t.poster_url ?? null;

function Loading({ label }: { label: string }) {
  return (
    <div className="cpl-my-muted" style={{ padding: '32px 0' }}>
      <Spinner label={label} />
    </div>
  );
}
function Failed({ what, onRetry }: { what: string; onRetry: () => void }) {
  const tr = useT();
  return (
    <EmptyState
      title={tr('cinema.my.failedTitle', { what })}
      description={tr('cinema.my.failedBody')}
      action={<Button onClick={onRetry}>{tr('cinema.my.tryAgain')}</Button>}
    />
  );
}

function Saved() {
  const tr = useT();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.saved, queryFn: () => getSaved() });
  const remove = useMutation({
    mutationFn: removeSaved,
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.saved }),
  });
  if (q.isPending) return <Loading label={tr('cinema.my.loadingSaved')} />;
  if (q.isError)
    return <Failed what={tr('cinema.my.whatSaved')} onRetry={() => void q.refetch()} />;
  if (q.data.length === 0)
    return (
      <EmptyState
        title={tr('cinema.my.savedEmptyTitle')}
        description={tr('cinema.my.savedEmptyBody')}
        action={
          <a className="cpl-primary" href="/cinema">
            {tr('cinema.my.browseCinema')}
          </a>
        }
      />
    );
  return (
    <ul className="cpl-my-grid" aria-label={tr('cinema.my.savedTitles')}>
      {q.data.map((t) => (
        <li key={t.id}>
          <PosterCard
            title={t.title}
            subtitle={titleLine(t)}
            src={poster(t)}
            href={titleHref(t.id)}
            tag={t.age_restricted ? '18+' : undefined}
            actions={
              <Button
                size="sm"
                variant="secondary"
                iconOnly
                aria-label={tr('cinema.my.removeFromList', { title: t.title })}
                onClick={() => remove.mutate(t.id)}
              >
                ✕
              </Button>
            }
          />
        </li>
      ))}
    </ul>
  );
}

function Continue() {
  const tr = useT();
  const q = useQuery({ queryKey: keys.continue, queryFn: () => getContinue() });
  if (q.isPending) return <Loading label={tr('cinema.my.loadingContinue')} />;
  if (q.isError)
    return <Failed what={tr('cinema.my.whatContinue')} onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0)
    return (
      <EmptyState
        title={tr('cinema.my.continueEmptyTitle')}
        description={tr('cinema.my.continueEmptyBody')}
      />
    );
  return (
    <ul className="cpl-my-grid" aria-label={tr('cinema.my.continueWatching')}>
      {q.data.items.map((i) => {
        const upNext = i.reason === 'up_next';
        const sub = i.episode
          ? `${upNext ? tr('cinema.my.upNext') : ''}${episodeLine(i.episode)}`
          : titleLine(i.title);
        return (
          <li key={`${i.title.id}:${i.episode?.id ?? ''}`}>
            <PosterCard
              title={i.title.title}
              subtitle={sub}
              src={poster(i.title)}
              href={watchHref(i.title.id, i.episode?.id ?? null)}
              progress={
                i.progress && !upNext ? Math.min(100, Math.round(i.progress.percent)) : undefined
              }
            />
          </li>
        );
      })}
    </ul>
  );
}

function ListItems({ id }: { id: string }) {
  const tr = useT();
  const q = useQuery({ queryKey: keys.list(id), queryFn: () => getList(id) });
  if (q.isPending) return <Loading label={tr('cinema.my.loadingList')} />;
  if (q.isError) return <Failed what={tr('cinema.my.whatList')} onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0) return <p className="cpl-my-muted">{tr('cinema.my.listEmpty')}</p>;
  return (
    <ul className="cpl-my-grid">
      {q.data.items.map(({ title: t }) => (
        <li key={t.id}>
          <PosterCard
            title={t.title}
            subtitle={titleLine(t)}
            src={poster(t)}
            href={titleHref(t.id)}
          />
        </li>
      ))}
    </ul>
  );
}

const VISIBILITY: Record<ListVisibility, string> = {
  private: 'cinema.my.visPrivate',
  friends: 'cinema.my.visFriends',
  public: 'cinema.my.visPublic',
};

function Lists() {
  const tr = useT();
  const q = useQuery({ queryKey: keys.lists, queryFn: getLists });
  const [open, setOpen] = useState<string | null>(null);
  if (q.isPending) return <Loading label={tr('cinema.my.loadingLists')} />;
  if (q.isError)
    return <Failed what={tr('cinema.my.whatLists')} onRetry={() => void q.refetch()} />;
  if (q.data.length === 0)
    return (
      <EmptyState
        title={tr('cinema.my.listsEmptyTitle')}
        description={tr('cinema.my.listsEmptyBody')}
      />
    );
  return (
    <div>
      {q.data.map((l) => (
        <div key={l.id} className="cpl-my-list-row">
          <div>
            <h3>{l.name}</h3>
            <p className="cpl-my-muted">
              {tr('cinema.my.itemCount', { n: l.item_count })} · {tr(VISIBILITY[l.visibility])}
            </p>
          </div>
          <button
            type="button"
            className="cpl-my-btn"
            aria-expanded={open === l.id}
            onClick={() => setOpen(open === l.id ? null : l.id)}
          >
            {open === l.id
              ? tr('cinema.my.hideTitles')
              : tr('cinema.my.showTitlesIn', { name: l.name })}
          </button>
          {open === l.id && (
            <div style={{ gridColumn: '1 / -1' }}>
              <ListItems id={l.id} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function History() {
  const tr = useT();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.history, queryFn: () => getHistory() });
  const settings = useQuery({ queryKey: keys.historySettings, queryFn: getHistorySettings });
  const [confirm, setConfirm] = useState(false);
  const pause = useMutation({
    mutationFn: setHistoryPaused,
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.historySettings }),
  });
  const clear = useMutation({
    mutationFn: clearHistory,
    onSuccess: () => {
      setConfirm(false);
      return qc.invalidateQueries({ queryKey: keys.history });
    },
  });
  if (q.isPending || settings.isPending) return <Loading label={tr('cinema.my.loadingHistory')} />;
  if (q.isError || settings.isError)
    return (
      <Failed
        what={tr('cinema.my.whatHistory')}
        onRetry={() => {
          void q.refetch();
          void settings.refetch();
        }}
      />
    );
  const { items } = q.data;
  const paused = settings.data.paused;
  return (
    <div>
      <div className="cpl-my-toolbar">
        <Switch
          label={tr('cinema.my.pauseHistory')}
          description={tr('cinema.my.pauseHistoryBody')}
          checked={paused}
          disabled={pause.isPending}
          onChange={(e) => pause.mutate(e.target.checked)}
        />
        <Button variant="secondary" disabled={items.length === 0} onClick={() => setConfirm(true)}>
          {tr('cinema.my.clearHistory')}
        </Button>
      </div>
      {items.length === 0 ? (
        <EmptyState
          title={paused ? tr('cinema.my.historyPaused') : tr('cinema.my.historyEmpty')}
          description={
            paused ? tr('cinema.my.historyPausedBody') : tr('cinema.my.historyEmptyBody')
          }
        />
      ) : (
        <ul
          style={{ listStyle: 'none', margin: '12px 0 0', padding: 0 }}
          aria-label={tr('cinema.my.watchHistory')}
        >
          {items.map((h) => (
            <li key={`${h.title.id}:${h.episode?.id ?? ''}`} className="cpl-my-list-row">
              <div>
                <h3>
                  <a href={watchHref(h.title.id, h.episode?.id ?? null)}>{h.title.title}</a>
                </h3>
                <p className="cpl-my-muted">
                  {[
                    h.episode ? episodeLine(h.episode) : null,
                    h.progress.completed ? tr('cinema.my.watched') : tr('cinema.my.inProgress'),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </p>
              </div>
              <time
                className="cpl-my-muted"
                dateTime={new Date(h.progress.updated_at_ms).toISOString()}
              >
                {new Date(h.progress.updated_at_ms).toLocaleDateString()}
              </time>
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        open={confirm}
        title={tr('cinema.my.clearTitle')}
        description={tr('cinema.my.clearBody')}
        confirmLabel={tr('cinema.my.clearHistory')}
        danger
        loading={clear.isPending}
        onConfirm={() => clear.mutate()}
        onCancel={() => setConfirm(false)}
      />
    </div>
  );
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={title}>
      <h2 className="cpl-sr-only">{title}</h2>
      {children}
    </section>
  );
}

export function MyListPage() {
  const tr = useT();
  return (
    <div className="cpl-my">
      <p className="cpl-my-eyebrow">{tr('cinema.my.eyebrow')}</p>
      <h1 className="cpl-my-h1">{tr('cinema.my.heading')}</h1>
      <Tabs
        label={tr('cinema.my.sections')}
        items={[
          {
            id: 'saved',
            label: tr('cinema.my.tabSaved'),
            content: (
              <Panel title={tr('cinema.my.savedTitles')}>
                <Saved />
              </Panel>
            ),
          },
          {
            id: 'continue',
            label: tr('cinema.my.continueWatching'),
            content: (
              <Panel title={tr('cinema.my.continueWatching')}>
                <Continue />
              </Panel>
            ),
          },
          {
            id: 'lists',
            label: tr('cinema.my.tabLists'),
            content: (
              <Panel title={tr('cinema.my.yourLists')}>
                <Lists />
              </Panel>
            ),
          },
          {
            id: 'history',
            label: tr('cinema.my.tabHistory'),
            content: (
              <Panel title={tr('cinema.my.watchHistory')}>
                <History />
              </Panel>
            ),
          },
        ]}
      />
    </div>
  );
}
