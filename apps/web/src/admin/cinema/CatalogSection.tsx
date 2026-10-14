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
import { useT } from '../../i18n';
import { SectionHeader, LoadMore, useAdminViewer, usePaged } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, useRun } from './hooks';
import { TitleEditor } from './TitleEditor';

const ANY = '__any';
const PAGE = 24;

export function CatalogSection() {
  const t = useT();
  const viewer = useAdminViewer();
  const canPublish = viewer.can('PUBLISH_MEDIA');
  const canDelete = viewer.can('DELETE_CONTENT');
  const [editing, setEditing] = useState<string | 'new' | null>(null);

  if (!canPublish && !canDelete)
    return (
      <>
        <SectionHeader title={t('admin.catalog.title')} />
        <EmptyState
          title={t('admin.catalog.noAccess.title')}
          description={t('admin.catalog.noAccess.description')}
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
  const t = useT();
  const kindOptions = [
    { value: ANY, label: t('admin.catalog.kind.all') },
    { value: 'movie', label: t('admin.titleEditor.kind.movie') },
    { value: 'series', label: t('admin.titleEditor.kind.series') },
    { value: 'anime', label: t('admin.titleEditor.kind.anime') },
  ];
  const statusOptions = [
    { value: ANY, label: t('admin.catalog.status.all') },
    { value: 'true', label: t('admin.catalog.status.published') },
    { value: 'false', label: t('admin.catalog.status.drafts') },
  ];
  const ageOptions = [
    { value: 'include', label: t('admin.catalog.age.include') },
    { value: 'only', label: t('admin.catalog.age.only') },
    { value: 'hide', label: t('admin.catalog.age.hide') },
  ];
  const sortOptions = [
    { value: 'updated', label: t('admin.catalog.sort.updated') },
    { value: 'newest', label: t('admin.catalog.sort.newest') },
    { value: 'title', label: t('admin.catalog.sort.title') },
    { value: 'year', label: t('admin.catalog.sort.year') },
  ];
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
    const timer = setTimeout(() => setDebounced(q.trim()), 250);
    return () => clearTimeout(timer);
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
  const view = paged.items.map((ti) => rows[ti.id] ?? ti);

  const toggle = (ti: api.Title) =>
    void run(
      async () => {
        const next = ti.published ? await api.unpublishTitle(ti.id) : await api.publishTitle(ti.id);
        setRows((r) => ({ ...r, [ti.id]: next }));
        return true;
      },
      ti.published
        ? t('admin.catalog.unpublished', { title: ti.title })
        : t('admin.catalog.published', { title: ti.title }),
    );

  return (
    <>
      <SectionHeader
        title={t('admin.catalog.title')}
        description={t('admin.catalog.description')}
        actions={
          canPublish ? (
            <Button onClick={() => onOpen('new')}>{t('admin.catalog.newTitle')}</Button>
          ) : undefined
        }
      />
      <div className="ss-toolbar">
        <TextInput
          label={t('admin.catalog.search')}
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="ss-search"
        />
        <Select
          aria-label={t('admin.catalog.filter.kind')}
          options={kindOptions}
          value={kind}
          onValueChange={setKind}
        />
        <Select
          aria-label={t('admin.catalog.filter.status')}
          options={statusOptions}
          value={status}
          onValueChange={setStatus}
        />
        <Select
          aria-label={t('admin.catalog.filter.audience')}
          options={ageOptions}
          value={age}
          onValueChange={setAge}
        />
        <Select
          aria-label={t('admin.catalog.filter.sort')}
          options={sortOptions}
          value={sort}
          onValueChange={setSort}
        />
      </div>
      <Banner error={error} notice={notice} />
      {paged.error && (
        <LoadProblem
          error={paged.error}
          status={failure}
          onRetry={paged.reload}
          what={t('admin.catalog.whatCatalog')}
        />
      )}
      {!paged.error && !paged.loading && view.length === 0 && (
        <EmptyState
          title={t('admin.catalog.empty.title')}
          description={
            debounced || kind !== ANY || status !== ANY
              ? t('admin.catalog.empty.filtered')
              : t('admin.catalog.empty.none')
          }
        />
      )}
      {view.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.catalog.table')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.catalog.col.poster')}</th>
                <th scope="col">{t('admin.catalog.col.title')}</th>
                <th scope="col">{t('admin.catalog.col.kind')}</th>
                <th scope="col">{t('admin.catalog.col.status')}</th>
                <th scope="col">{t('admin.catalog.col.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {view.map((ti) => (
                <tr key={ti.id}>
                  <td>
                    {ti.poster_url ? (
                      <img className="adm-cin-poster" src={ti.poster_url} alt="" loading="lazy" />
                    ) : (
                      <span className="adm-cin-poster adm-cin-poster-empty" aria-hidden="true" />
                    )}
                  </td>
                  <td>
                    <div className="ss-name">{ti.title}</div>
                    <div className="ss-meta">
                      {[ti.year, ti.content_rating].filter(Boolean).join(' · ') ||
                        t('admin.catalog.noYear')}
                      {ti.kind !== 'movie' &&
                        t('admin.catalog.counts', {
                          seasons: ti.season_count,
                          episodes: ti.episode_count,
                        })}
                    </div>
                  </td>
                  <td>
                    {ti.kind}{' '}
                    {ti.age_restricted && (
                      <Badge variant="danger">{t('admin.catalog.age18')}</Badge>
                    )}
                  </td>
                  <td>
                    <Badge variant={ti.published ? 'success' : 'neutral'}>
                      {ti.published
                        ? t('admin.catalog.statusPublished')
                        : t('admin.catalog.statusDraft')}
                    </Badge>{' '}
                    {!ti.playable && (
                      <Badge variant="warning">{t('admin.catalog.nothingPlayable')}</Badge>
                    )}
                  </td>
                  <td>
                    <div className="adm-actions">
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.catalog.editAria', { title: ti.title })}
                        onClick={() => onOpen(ti.id)}
                      >
                        {t('admin.catalog.edit')}
                      </Button>
                      {(canPublish || ti.published) && (
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={busy}
                          aria-label={
                            ti.published
                              ? t('admin.catalog.unpublishAria', { title: ti.title })
                              : t('admin.catalog.publishAria', { title: ti.title })
                          }
                          onClick={() => toggle(ti)}
                        >
                          {ti.published ? t('admin.catalog.unpublish') : t('admin.catalog.publish')}
                        </Button>
                      )}
                      {canPublish && canDelete && (
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={t('admin.catalog.mergeAria', { title: ti.title })}
                          onClick={() => (clear(), setMerge(ti))}
                        >
                          {t('admin.catalog.merge')}
                        </Button>
                      )}
                      {canDelete && (
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('admin.catalog.deleteAria', { title: ti.title })}
                          onClick={() => (clear(), setRemove(ti))}
                        >
                          {t('admin.catalog.delete')}
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
        title={t('admin.catalog.deleteTitle', {
          name: remove?.title ?? t('admin.catalog.titleWord'),
        })}
        description={t('admin.catalog.deleteDescription')}
        requireText={remove?.title}
        confirmLabel={t('admin.catalog.deleteConfirm')}
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const ti = remove;
          if (!ti) return;
          void run(
            async () => {
              await api.deleteTitle(ti.id);
              return true;
            },
            t('admin.catalog.deleted', { title: ti.title }),
          ).then((ok) => {
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
  const t = useT();
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
    const timer = setTimeout(() => {
      api
        .listTitles({ title: q.trim(), limit: 8 })
        .then((p) => live && setResults(p.items.filter((x) => x.id !== source.id)))
        .catch(() => live && setResults([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [q, source]);

  return (
    <Modal
      open={!!source}
      onClose={busy ? () => undefined : onCancel}
      title={t('admin.catalog.mergeDialog.title', { title: source?.title ?? '' })}
      description={t('admin.catalog.mergeDialog.description')}
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} disabled={busy}>
            {t('admin.catalog.cancel')}
          </Button>
          <Button
            variant="danger"
            disabled={!target}
            loading={busy}
            onClick={() => {
              if (!source || !target) return;
              void run(async () => {
                await api.mergeTitle(source.id, target.id);
                onDone(
                  t('admin.catalog.mergeDialog.done', {
                    source: source.title,
                    target: target.title,
                  }),
                );
              });
            }}
          >
            {t('admin.catalog.mergeDialog.confirm')}
          </Button>
        </>
      }
    >
      <div className="adm-form">
        <TextInput
          label={t('admin.catalog.mergeDialog.find')}
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        {results.length > 0 && (
          <ul className="ss-list" aria-label={t('admin.catalog.mergeDialog.targets')}>
            {results.map((r) => (
              <li key={r.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">{r.title}</div>
                  <div className="ss-meta">
                    {r.kind} · {r.year ?? t('admin.catalog.mergeDialog.noYear')} ·{' '}
                    {r.published
                      ? t('admin.catalog.statusPublished')
                      : t('admin.catalog.statusDraft')}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant={target?.id === r.id ? 'primary' : 'secondary'}
                  aria-pressed={target?.id === r.id}
                  aria-label={t('admin.catalog.mergeDialog.keepAria', { title: r.title })}
                  onClick={() => setTarget(r)}
                >
                  {target?.id === r.id
                    ? t('admin.catalog.mergeDialog.chosen')
                    : t('admin.catalog.mergeDialog.choose')}
                </Button>
              </li>
            ))}
          </ul>
        )}
        {target && (
          <Notice variant="warning">
            {t('admin.catalog.mergeDialog.willMerge', {
              source: source?.title ?? '',
              target: target.title,
            })}
          </Notice>
        )}
        <Banner error={error} />
      </div>
    </Modal>
  );
}
