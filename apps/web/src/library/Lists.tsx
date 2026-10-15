// Reading lists (LIB-SOC-010): mine, featured, a list's detail and its editor.
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Icon,
  Notice,
  Skeleton,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import {
  createList,
  deleteList,
  getFeaturedLists,
  getList,
  getMyLists,
  removeFromList,
  reorderList,
  socialKeys,
  updateList,
  type ListVisibility,
  type ReadingList,
} from './social';
import type { Series } from './types';
import { AgeBadge, KIND_LABEL, label } from './ui';

const VISIBILITY: { value: ListVisibility; labelKey: MessageKey }[] = [
  { value: 'private', labelKey: 'library.lists.visibility.private' },
  { value: 'friends', labelKey: 'library.lists.visibility.friends' },
  { value: 'public', labelKey: 'library.lists.visibility.public' },
];

const refreshLists = (qc: ReturnType<typeof useQueryClient>) =>
  qc.invalidateQueries({ queryKey: ['library', 'lists'] });

function VisibilitySelect({
  value,
  onChange,
}: {
  value: ListVisibility;
  onChange: (v: ListVisibility) => void;
}) {
  const t = useT();
  return (
    <label className="lib-select">
      <span>{t('library.lists.visibility.label')}</span>
      <select value={value} onChange={(e) => onChange(e.target.value as ListVisibility)}>
        {VISIBILITY.map((v) => (
          <option key={v.value} value={v.value}>
            {t(v.labelKey)}
          </option>
        ))}
      </select>
    </label>
  );
}

/* ------------------------------------------------------------ the tiles */

function ListTile({ list }: { list: ReadingList }) {
  const t = useT();
  const visibility = VISIBILITY.find((v) => v.value === list.visibility)!;
  return (
    <li className="lib-list-tile">
      <Link to="/library/lists/$listId" params={{ listId: list.id }} className="lib-list-tile-link">
        <div className="lib-fan lib-fan-tile" aria-hidden="true">
          {list.covers.slice(0, 4).map((src, i) => (
            <img key={src + i} src={src} alt="" style={{ ['--i' as string]: i }} />
          ))}
        </div>
        <h3 className="lib-list-title">{list.name}</h3>
        <p className="lib-muted">
          {t('library.lists.tileMeta', {
            n: list.item_count,
            visibility: t(visibility.labelKey),
          })}
          {!list.mine && ` · ${t('library.lists.byOwner', { name: list.owner_name })}`}
        </p>
        {list.featured && (
          <span className="lib-chip-tag lib-featured-tag">{t('library.lists.featured')}</span>
        )}
      </Link>
    </li>
  );
}

function NewListForm({ onDone }: { onDone: () => void }) {
  const t = useT();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [visibility, setVisibility] = useState<ListVisibility>('private');
  const create = useMutation({
    mutationFn: () =>
      createList({ name: name.trim(), description: description.trim(), visibility }),
    onSuccess: async (list) => {
      await refreshLists(qc);
      onDone();
      void navigate({ to: '/library/lists/$listId', params: { listId: list.id } });
    },
  });
  return (
    <form
      className="lib-review-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) create.mutate();
      }}
    >
      <TextInput
        label={t('library.lists.name')}
        value={name}
        maxLength={100}
        onChange={(e) => setName(e.target.value)}
      />
      <TextArea
        label={t('library.lists.description')}
        value={description}
        rows={3}
        maxLength={500}
        onChange={(e) => setDescription(e.target.value)}
      />
      <VisibilitySelect value={visibility} onChange={setVisibility} />
      {create.isError && <Notice variant="danger">{t('library.lists.createError')}</Notice>}
      <div className="lib-actions">
        <Button type="submit" disabled={!name.trim() || create.isPending}>
          {t('library.lists.create')}
        </Button>
        <Button variant="ghost" type="button" onClick={onDone}>
          {t('library.lists.cancel')}
        </Button>
      </div>
    </form>
  );
}

/* ----------------------------------------------------------- lists page */

export function ListsPage() {
  const t = useT();
  const [creating, setCreating] = useState(false);
  const mine = useQuery({ queryKey: socialKeys.myLists(), queryFn: () => getMyLists() });
  const featured = useQuery({ queryKey: socialKeys.featured(), queryFn: () => getFeaturedLists() });
  return (
    <div className="lib-lists">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.lists.title')}</h1>
        {!creating && (
          <Button onClick={() => setCreating(true)}>
            <Icon name="plus" size={16} />
            {t('library.lists.new')}
          </Button>
        )}
      </div>
      {creating && <NewListForm onDone={() => setCreating(false)} />}
      <section className="lib-section" aria-labelledby="lib-lists-mine">
        <h2 id="lib-lists-mine" className="lib-h2">
          {t('library.lists.mine')}
        </h2>
        {mine.isPending ? (
          <Skeleton height={140} />
        ) : mine.isError ? (
          <Notice variant="danger" title={t('library.lists.loadError')} />
        ) : mine.data.items.length === 0 ? (
          <EmptyState
            title={t('library.lists.emptyTitle')}
            description={t('library.lists.emptyDescription')}
          />
        ) : (
          <ul className="lib-list-tiles">
            {mine.data.items.map((l) => (
              <ListTile key={l.id} list={l} />
            ))}
          </ul>
        )}
      </section>
      {(featured.data?.items.length ?? 0) > 0 && (
        <section className="lib-section" aria-labelledby="lib-lists-featured">
          <h2 id="lib-lists-featured" className="lib-h2">
            {t('library.lists.featuredTitle')}
          </h2>
          <ul className="lib-list-tiles">
            {featured.data!.items.map((l) => (
              <ListTile key={l.id} list={l} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/* ------------------------------------------------------------ list page */

export function ListPage({ listId }: { listId: string }) {
  const t = useT();
  const q = useQuery({ queryKey: socialKeys.list(listId), queryFn: () => getList(listId) });
  if (q.isPending) return <Skeleton height={240} />;
  if (q.isError) {
    const gone = q.error instanceof ApiError && q.error.status === 404;
    return (
      <EmptyState
        title={gone ? t('library.lists.goneTitle') : t('library.lists.errorTitle')}
        description={
          gone ? t('library.lists.goneDescription') : t('library.lists.errorDescription')
        }
        action={
          <Link to="/library/lists" className="lib-btn">
            {t('library.lists.back')}
          </Link>
        }
      />
    );
  }
  return <ListView listId={listId} data={q.data} />;
}

function ListView({
  listId,
  data,
}: {
  listId: string;
  data: { list: ReadingList; items: { series: Series; note: string }[] };
}) {
  const t = useT();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const { list, items } = data;
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [name, setName] = useState(list.name);
  const [description, setDescription] = useState(list.description);
  const [visibility, setVisibility] = useState<ListVisibility>(list.visibility);
  const refresh = () => refreshLists(qc);
  const save = useMutation({
    mutationFn: () =>
      updateList(listId, { name: name.trim(), description: description.trim(), visibility }),
    onSuccess: async () => {
      await refresh();
      setEditing(false);
    },
  });
  const remove = useMutation({
    mutationFn: () => deleteList(listId),
    onSuccess: async () => {
      await refresh();
      void navigate({ to: '/library/lists' });
    },
  });
  const drop = useMutation({
    mutationFn: (seriesId: string) => removeFromList(listId, seriesId),
    onSuccess: refresh,
  });
  const move = useMutation({
    mutationFn: ({ from, to }: { from: number; to: number }) => {
      const ids = items.map((i) => i.series.id);
      const [m] = ids.splice(from, 1);
      ids.splice(to, 0, m!);
      return reorderList(listId, ids);
    },
    onSuccess: refresh,
  });
  const visibilityKey = VISIBILITY.find((v) => v.value === list.visibility)!.labelKey;
  return (
    <article className="lib-list-page">
      <header className="lib-page-head">
        <div>
          <p className="lib-eyebrow">
            {list.featured ? t('library.lists.featured') : t(visibilityKey)}
            {!list.mine && ` · ${t('library.lists.byOwner', { name: list.owner_name })}`}
          </p>
          <h1 className="lib-h1">{list.name}</h1>
          {list.description && <p className="lib-muted">{list.description}</p>}
        </div>
        {list.mine && !editing && (
          <div className="lib-actions">
            <Button variant="secondary" onClick={() => setEditing(true)}>
              <Icon name="edit" size={16} />
              {t('library.lists.edit')}
            </Button>
            <Button variant="danger" onClick={() => setConfirmDelete(true)}>
              <Icon name="trash" size={16} />
              {t('library.lists.delete')}
            </Button>
          </div>
        )}
      </header>
      {editing && (
        <form
          className="lib-review-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) save.mutate();
          }}
        >
          <TextInput
            label={t('library.lists.name')}
            value={name}
            maxLength={100}
            onChange={(e) => setName(e.target.value)}
          />
          <TextArea
            label={t('library.lists.description')}
            value={description}
            rows={3}
            maxLength={500}
            onChange={(e) => setDescription(e.target.value)}
          />
          <VisibilitySelect value={visibility} onChange={setVisibility} />
          {save.isError && <Notice variant="danger">{t('library.lists.saveError')}</Notice>}
          <div className="lib-actions">
            <Button type="submit" disabled={!name.trim() || save.isPending}>
              {t('library.lists.save')}
            </Button>
            <Button variant="ghost" type="button" onClick={() => setEditing(false)}>
              {t('library.lists.cancel')}
            </Button>
          </div>
        </form>
      )}
      {items.length === 0 ? (
        <EmptyState
          title={t('library.lists.itemsEmptyTitle')}
          description={
            list.mine ? t('library.lists.itemsEmptyMine') : t('library.lists.itemsEmptyOthers')
          }
          action={
            list.mine ? (
              <Link to="/library/browse" search={{}} className="lib-btn">
                {t('library.lists.browse')}
              </Link>
            ) : undefined
          }
        />
      ) : (
        <ol className="lib-update-list lib-update-list-wide" aria-label={t('library.lists.items')}>
          {items.map((it, i) => (
            <li key={it.series.id} className="lib-update-row">
              <Link
                to="/library/s/$seriesId"
                params={{ seriesId: it.series.id }}
                className="lib-update-cover"
                aria-hidden
                tabIndex={-1}
              >
                {it.series.cover_url ? (
                  <img src={it.series.cover_url} alt="" loading="lazy" />
                ) : (
                  <span>{it.series.title.charAt(0)}</span>
                )}
              </Link>
              <div className="lib-update-text">
                <Link
                  to="/library/s/$seriesId"
                  params={{ seriesId: it.series.id }}
                  className="lib-update-title"
                >
                  {it.series.title}
                </Link>
                {it.series.age_restricted && <AgeBadge />}
                <span className="lib-update-line">
                  {KIND_LABEL[it.series.kind]}
                  {it.series.genres.length > 0
                    ? ` · ${it.series.genres.slice(0, 2).map(label).join(' · ')}`
                    : ''}
                  {it.note ? ` · ${it.note}` : ''}
                </span>
              </div>
              {list.mine && (
                <span className="lib-list-controls">
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={i === 0 || move.isPending}
                    aria-label={t('library.lists.moveUp', { title: it.series.title })}
                    onClick={() => move.mutate({ from: i, to: i - 1 })}
                  >
                    ↑
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={i === items.length - 1 || move.isPending}
                    aria-label={t('library.lists.moveDown', { title: it.series.title })}
                    onClick={() => move.mutate({ from: i, to: i + 1 })}
                  >
                    ↓
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={drop.isPending}
                    aria-label={t('library.lists.removeItem', { title: it.series.title })}
                    onClick={() => drop.mutate(it.series.id)}
                  >
                    <Icon name="close" size={16} />
                  </Button>
                </span>
              )}
            </li>
          ))}
        </ol>
      )}
      <ConfirmDialog
        open={confirmDelete}
        title={t('library.lists.deleteTitle', { name: list.name })}
        description={t('library.lists.deleteDescription')}
        confirmLabel={t('library.lists.delete')}
        cancelLabel={t('library.lists.cancel')}
        danger
        loading={remove.isPending}
        onConfirm={() => remove.mutate()}
        onCancel={() => setConfirmDelete(false)}
      />
    </article>
  );
}
