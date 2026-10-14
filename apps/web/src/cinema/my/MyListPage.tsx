// My list (CIN-MY-001, spec cinema-media 2.4): saved titles, continue watching, custom lists and watch history
// with pause and clear. Posters link to the title page (`/cinema/t/:id`, owned by the browse UI) and straight
// into the player for continue watching.
import { Button, ConfirmDialog, EmptyState, PosterCard, Spinner, Switch, Tabs } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { watchHref } from '../player/model';
import '../player/cinema-player.css';
import {
  clearHistory,
  getContinue,
  getHistory,
  getList,
  getLists,
  getSaved,
  keys,
  removeSaved,
  setHistoryPaused,
} from './api';
import type { CinemaKind, TitleCard } from './types';

const KIND_LABEL: Record<CinemaKind, string> = { movie: 'Film', series: 'Series', anime: 'Anime' };
const titleLine = (t: TitleCard) =>
  [t.year, KIND_LABEL[t.kind]].filter((x) => x !== null && x !== undefined).join(' · ');
const titleHref = (id: string) => `/cinema/t/${encodeURIComponent(id)}`;

function Loading({ label }: { label: string }) {
  return (
    <div className="cin-my-muted" style={{ padding: '32px 0' }}>
      <Spinner label={label} />
    </div>
  );
}
function Failed({ what, onRetry }: { what: string; onRetry: () => void }) {
  return (
    <EmptyState
      title={`We could not load ${what}`}
      description="Check your connection and try again."
      action={<Button onClick={onRetry}>Try again</Button>}
    />
  );
}

function Saved() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.saved, queryFn: getSaved });
  const remove = useMutation({
    mutationFn: removeSaved,
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.saved }),
  });
  if (q.isPending) return <Loading label="Loading your list" />;
  if (q.isError) return <Failed what="your list" onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0)
    return (
      <EmptyState
        title="Nothing saved yet"
        description="Press + on a poster to keep a title here."
        action={
          <a className="cin-primary" href="/cinema">
            Browse the cinema
          </a>
        }
      />
    );
  return (
    <ul className="cin-my-grid" aria-label="Saved titles">
      {q.data.items.map((t) => (
        <li key={t.id}>
          <PosterCard
            title={t.title}
            subtitle={titleLine(t)}
            src={t.poster_url}
            href={titleHref(t.id)}
            tag={t.age_restricted ? '18+' : undefined}
            actions={
              <Button
                size="sm"
                variant="secondary"
                iconOnly
                aria-label={`Remove ${t.title} from My list`}
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
  const q = useQuery({ queryKey: keys.continue, queryFn: () => getContinue() });
  if (q.isPending) return <Loading label="Loading continue watching" />;
  if (q.isError) return <Failed what="continue watching" onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0)
    return (
      <EmptyState
        title="Nothing in progress"
        description="Start something and it will wait for you here."
      />
    );
  return (
    <ul className="cin-my-grid" aria-label="Continue watching">
      {q.data.items.map((i) => (
        <li key={`${i.title.id}:${i.episode_id ?? ''}`}>
          <PosterCard
            title={i.title.title}
            subtitle={i.episode_label ?? titleLine(i.title)}
            src={i.title.poster_url}
            href={watchHref(i.title.id, i.episode_id)}
            progress={
              i.duration_s > 0 ? Math.min(100, Math.round((i.position_s / i.duration_s) * 100)) : 0
            }
          />
        </li>
      ))}
    </ul>
  );
}

function ListItems({ id }: { id: string }) {
  const q = useQuery({ queryKey: keys.list(id), queryFn: () => getList(id) });
  if (q.isPending) return <Loading label="Loading list" />;
  if (q.isError) return <Failed what="this list" onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0) return <p className="cin-my-muted">This list is empty.</p>;
  return (
    <ul className="cin-my-grid">
      {q.data.items.map((t) => (
        <li key={t.id}>
          <PosterCard
            title={t.title}
            subtitle={titleLine(t)}
            src={t.poster_url}
            href={titleHref(t.id)}
          />
        </li>
      ))}
    </ul>
  );
}

const VISIBILITY = { private: 'Only you', friends: 'Friends', everyone: 'Everyone' } as const;

function Lists() {
  const q = useQuery({ queryKey: keys.lists, queryFn: getLists });
  const [open, setOpen] = useState<string | null>(null);
  if (q.isPending) return <Loading label="Loading your lists" />;
  if (q.isError) return <Failed what="your lists" onRetry={() => void q.refetch()} />;
  if (q.data.items.length === 0)
    return (
      <EmptyState
        title="No lists yet"
        description="Group titles into lists you can keep private or share."
      />
    );
  return (
    <div>
      {q.data.items.map((l) => (
        <div key={l.id} className="cin-my-list-row">
          <div>
            <h3>{l.name}</h3>
            <p className="cin-my-muted">
              {l.item_count} {l.item_count === 1 ? 'title' : 'titles'} · {VISIBILITY[l.visibility]}
            </p>
          </div>
          <button
            type="button"
            className="cin-my-btn"
            aria-expanded={open === l.id}
            onClick={() => setOpen(open === l.id ? null : l.id)}
          >
            {open === l.id ? 'Hide titles' : `Show titles in ${l.name}`}
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
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.history, queryFn: () => getHistory() });
  const [confirm, setConfirm] = useState(false);
  const pause = useMutation({
    mutationFn: setHistoryPaused,
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.history }),
  });
  const clear = useMutation({
    mutationFn: clearHistory,
    onSuccess: () => {
      setConfirm(false);
      return qc.invalidateQueries({ queryKey: keys.history });
    },
  });
  if (q.isPending) return <Loading label="Loading watch history" />;
  if (q.isError) return <Failed what="your history" onRetry={() => void q.refetch()} />;
  const { items, paused } = q.data;
  return (
    <div>
      <div className="cin-my-toolbar">
        <Switch
          label="Pause watch history"
          description="While paused, new viewing is not added to your history."
          checked={paused}
          disabled={pause.isPending}
          onChange={(e) => pause.mutate(e.target.checked)}
        />
        <Button variant="secondary" disabled={items.length === 0} onClick={() => setConfirm(true)}>
          Clear history
        </Button>
      </div>
      {items.length === 0 ? (
        <EmptyState
          title={paused ? 'History is paused' : 'No history yet'}
          description={
            paused
              ? 'Turn it back on to remember what you watch.'
              : 'What you watch will show here.'
          }
        />
      ) : (
        <ul
          style={{ listStyle: 'none', margin: '12px 0 0', padding: 0 }}
          aria-label="Watch history"
        >
          {items.map((h) => (
            <li key={h.id} className="cin-my-list-row">
              <div>
                <h3>
                  <a href={watchHref(h.title.id, h.episode_id)}>{h.title.title}</a>
                </h3>
                <p className="cin-my-muted">
                  {[h.episode_label, h.completed ? 'Watched' : 'In progress']
                    .filter(Boolean)
                    .join(' · ')}
                </p>
              </div>
              <time className="cin-my-muted" dateTime={new Date(h.watched_at_ms).toISOString()}>
                {new Date(h.watched_at_ms).toLocaleDateString()}
              </time>
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        open={confirm}
        title="Clear your watch history?"
        description="This removes the list of what you watched. Your saved titles and where you stopped are kept."
        confirmLabel="Clear history"
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
      <h2 className="cin-sr-only">{title}</h2>
      {children}
    </section>
  );
}

export function MyListPage() {
  return (
    <main className="cin-my">
      <p className="cin-my-eyebrow">— Your space</p>
      <h1 className="cin-my-h1">My list.</h1>
      <Tabs
        label="My list sections"
        items={[
          {
            id: 'saved',
            label: 'Saved',
            content: (
              <Panel title="Saved titles">
                <Saved />
              </Panel>
            ),
          },
          {
            id: 'continue',
            label: 'Continue watching',
            content: (
              <Panel title="Continue watching">
                <Continue />
              </Panel>
            ),
          },
          {
            id: 'lists',
            label: 'Lists',
            content: (
              <Panel title="Your lists">
                <Lists />
              </Panel>
            ),
          },
          {
            id: 'history',
            label: 'History',
            content: (
              <Panel title="Watch history">
                <History />
              </Panel>
            ),
          },
        ]}
      />
    </main>
  );
}
