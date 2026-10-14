// Cinema catalog (CIN-ADM-001): the titles list with search, filters, publish/unpublish, merge and delete,
// opening the title editor in place. Permissions (doc 07): PUBLISH_MEDIA creates, edits and publishes;
// DELETE_CONTENT deletes; merging takes both; unpublishing takes either.
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  Notice,
  Select,
  TextInput,
} from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { SectionHeader, LoadMore, useAdminViewer, usePaged } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, useRun } from './hooks';
import { TitleEditor } from './TitleEditor';

const ANY = '__any';
const KIND_OPTIONS = [
  { value: ANY, label: 'All kinds' },
  ...api.TITLE_KINDS.map((k) => ({ value: k, label: k[0]!.toUpperCase() + k.slice(1) })),
];
const STATUS_OPTIONS = [
  { value: ANY, label: 'Published and drafts' },
  { value: 'true', label: 'Published' },
  { value: 'false', label: 'Drafts' },
];
const AGE_OPTIONS = [
  { value: 'include', label: 'Any audience' },
  { value: 'only', label: 'Age-restricted only' },
  { value: 'hide', label: 'Hide age-restricted' },
];
const SORT_OPTIONS = [
  { value: 'updated', label: 'Recently updated' },
  { value: 'newest', label: 'Newest first' },
  { value: 'title', label: 'Title A to Z' },
  { value: 'year', label: 'Year' },
];
const PAGE = 24;

export function CatalogSection() {
  const viewer = useAdminViewer();
  const canPublish = viewer.can('PUBLISH_MEDIA');
  const canDelete = viewer.can('DELETE_CONTENT');
  const [editing, setEditing] = useState<string | 'new' | null>(null);

  if (!canPublish && !canDelete)
    return (
      <>
        <SectionHeader title="Cinema catalog" />
        <EmptyState
          title="You do not have access"
          description="Editing the catalog takes the Publish media permission (the Media Curator role)."
        />
      </>
    );
  if (editing)
    return (
      <TitleEditor titleId={editing === 'new' ? null : editing} onClose={() => setEditing(null)} />
    );
  return <TitleList onOpen={setEditing} />;
}

function TitleList({ onOpen }: { onOpen: (id: string | 'new') => void }) {
  const viewer = useAdminViewer();
  const canPublish = viewer.can('PUBLISH_MEDIA');
  const canDelete = viewer.can('DELETE_CONTENT');
  const [q, setQ] = useState('');
  const [debounced, setDebounced] = useState('');
  const [kind, setKind] = useState(ANY);
  const [status, setStatus] = useState(ANY);
  const [age, setAge] = useState('include');
  const [sort, setSort] = useState('updated');
  const [merge, setMerge] = useState<api.Title | null>(null);
  const [remove, setRemove] = useState<api.Title | null>(null);
  const { busy, error, notice, run, flash, clear } = useRun();

  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 250);
    return () => clearTimeout(t);
  }, [q]);

  const key = `cinema-titles:${debounced}:${kind}:${status}:${age}:${sort}`;
  const [failure, setFailure] = useState<number | null>(null);
  const paged = usePaged<api.Title, number>(key, async (offset) => {
    try {
      const p = await api.listTitles({
        title: debounced,
        kind: kind === ANY ? undefined : kind,
        published: status === ANY ? undefined : status,
        age,
        sort,
        limit: PAGE,
        offset: offset ?? 0,
      });
      setFailure(null);
      const used = (offset ?? 0) + p.items.length;
      return { items: p.items, next: used < p.total ? used : null };
    } catch (e) {
      setFailure((e as { status?: number }).status ?? null);
      throw e;
    }
  });
  const [rows, setRows] = useState<Record<string, api.Title>>({});
  const view = paged.items.map((t) => rows[t.id] ?? t);

  const toggle = (t: api.Title) =>
    void run(
      async () => {
        const next = t.published ? await api.unpublishTitle(t.id) : await api.publishTitle(t.id);
        setRows((r) => ({ ...r, [t.id]: next }));
        return true;
      },
      t.published ? `${t.title} unpublished.` : `${t.title} published.`,
    );

  return (
    <>
      <SectionHeader
        title="Cinema catalog"
        description="Titles, seasons, episodes, artwork and skip markers. Drafts stay hidden from members until you publish them."
        actions={canPublish ? <Button onClick={() => onOpen('new')}>New title</Button> : undefined}
      />
      <div className="ss-toolbar">
        <TextInput
          label="Search titles"
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="ss-search"
        />
        <Select aria-label="Kind" options={KIND_OPTIONS} value={kind} onValueChange={setKind} />
        <Select
          aria-label="Status"
          options={STATUS_OPTIONS}
          value={status}
          onValueChange={setStatus}
        />
        <Select aria-label="Audience" options={AGE_OPTIONS} value={age} onValueChange={setAge} />
        <Select aria-label="Sort" options={SORT_OPTIONS} value={sort} onValueChange={setSort} />
      </div>
      <Banner error={error} notice={notice} />
      {paged.error && (
        <LoadProblem
          error={paged.error}
          status={failure}
          onRetry={paged.reload}
          what="the catalog"
        />
      )}
      {!paged.error && !paged.loading && view.length === 0 && (
        <EmptyState
          title="No titles"
          description={
            debounced || kind !== ANY || status !== ANY
              ? 'Nothing matches these filters.'
              : 'The catalog is empty. Create the first title.'
          }
        />
      )}
      {view.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Titles">
            <thead>
              <tr>
                <th scope="col">Poster</th>
                <th scope="col">Title</th>
                <th scope="col">Kind</th>
                <th scope="col">Status</th>
                <th scope="col">Actions</th>
              </tr>
            </thead>
            <tbody>
              {view.map((t) => (
                <tr key={t.id}>
                  <td>
                    {t.poster_url ? (
                      <img className="adm-cin-poster" src={t.poster_url} alt="" loading="lazy" />
                    ) : (
                      <span className="adm-cin-poster adm-cin-poster-empty" aria-hidden="true" />
                    )}
                  </td>
                  <td>
                    <div className="ss-name">{t.title}</div>
                    <div className="ss-meta">
                      {[t.year, t.content_rating].filter(Boolean).join(' · ') || 'No year'}
                      {t.kind !== 'movie' &&
                        ` · ${t.season_count} season${t.season_count === 1 ? '' : 's'}, ${t.episode_count} episode${t.episode_count === 1 ? '' : 's'}`}
                    </div>
                  </td>
                  <td>
                    {t.kind} {t.age_restricted && <Badge variant="danger">18+</Badge>}
                  </td>
                  <td>
                    <Badge variant={t.published ? 'success' : 'neutral'}>
                      {t.published ? 'published' : 'draft'}
                    </Badge>{' '}
                    {!t.playable && <Badge variant="warning">nothing playable</Badge>}
                  </td>
                  <td>
                    <div className="adm-actions">
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={`Edit ${t.title}`}
                        onClick={() => onOpen(t.id)}
                      >
                        Edit
                      </Button>
                      {(canPublish || t.published) && (
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={busy}
                          aria-label={`${t.published ? 'Unpublish' : 'Publish'} ${t.title}`}
                          onClick={() => toggle(t)}
                        >
                          {t.published ? 'Unpublish' : 'Publish'}
                        </Button>
                      )}
                      {canPublish && canDelete && (
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={`Merge ${t.title} into another title`}
                          onClick={() => (clear(), setMerge(t))}
                        >
                          Merge
                        </Button>
                      )}
                      {canDelete && (
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={`Delete ${t.title}`}
                          onClick={() => (clear(), setRemove(t))}
                        >
                          Delete
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      <MergeDialog
        source={merge}
        onCancel={() => setMerge(null)}
        onDone={(msg) => {
          setMerge(null);
          paged.reload();
          flash(msg);
        }}
      />
      <ConfirmDialog
        open={!!remove}
        danger
        title={`Delete ${remove?.title ?? 'title'}`}
        description="This removes the title with its seasons, episodes, variants, trailers and markers, and members lose their lists and progress for it. It cannot be undone. Type the title to confirm."
        requireText={remove?.title}
        confirmLabel="Delete title"
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const t = remove;
          if (!t) return;
          void run(async () => {
            await api.deleteTitle(t.id);
            return true;
          }, `${t.title} deleted.`).then((ok) => {
            setRemove(null);
            if (ok) paged.reload();
          });
        }}
      />
    </>
  );
}

/** Pick a target title and merge the source into it (seasons, episodes and member data move across). */
function MergeDialog({
  source,
  onCancel,
  onDone,
}: {
  source: api.Title | null;
  onCancel: () => void;
  onDone: (msg: string) => void;
}) {
  const [q, setQ] = useState('');
  const [results, setResults] = useState<api.Title[]>([]);
  const [target, setTarget] = useState<api.Title | null>(null);
  const { busy, error, run, clear } = useRun();

  useEffect(() => {
    if (!source) {
      setQ('');
      setResults([]);
      setTarget(null);
      clear();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source]);
  useEffect(() => {
    if (!source || q.trim().length < 2) {
      setResults([]);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      api
        .listTitles({ title: q.trim(), limit: 8 })
        .then((p) => live && setResults(p.items.filter((x) => x.id !== source.id)))
        .catch(() => live && setResults([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [q, source]);

  return (
    <Modal
      open={!!source}
      onClose={busy ? () => undefined : onCancel}
      title={`Merge ${source?.title ?? ''}`}
      description="Seasons, episodes, variants and member data move to the title you pick. This title is then removed and its address redirects to the target."
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant="danger"
            disabled={!target}
            loading={busy}
            onClick={() => {
              if (!source || !target) return;
              void run(async () => {
                await api.mergeTitle(source.id, target.id);
                onDone(`${source.title} merged into ${target.title}.`);
              });
            }}
          >
            Merge titles
          </Button>
        </>
      }
    >
      <div className="adm-form">
        <TextInput
          label="Find the title to keep"
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        {results.length > 0 && (
          <ul className="ss-list" aria-label="Merge targets">
            {results.map((r) => (
              <li key={r.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">{r.title}</div>
                  <div className="ss-meta">
                    {r.kind} · {r.year ?? 'no year'} · {r.published ? 'published' : 'draft'}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant={target?.id === r.id ? 'primary' : 'secondary'}
                  aria-pressed={target?.id === r.id}
                  aria-label={`Keep ${r.title}`}
                  onClick={() => setTarget(r)}
                >
                  {target?.id === r.id ? 'Chosen' : 'Choose'}
                </Button>
              </li>
            ))}
          </ul>
        )}
        {target && (
          <Notice variant="warning">
            {source?.title} will be merged into {target.title}.
          </Notice>
        )}
        <Banner error={error} />
      </div>
    </Modal>
  );
}
