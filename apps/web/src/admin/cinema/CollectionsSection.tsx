// Cinema collections editor (CIN-ADM-001): the curated rows and hero spotlights. Items are ordered; drag a row
// or use the move buttons, then save the order. Needs MANAGE_COLLECTIONS.
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  Select,
  Switch,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { SectionHeader, useAdminViewer } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, intOrNull, useLoad, useRun } from './hooks';

const slugify = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');

export function CollectionsSection() {
  const t = useT();
  const allowed = useAdminViewer().can('MANAGE_COLLECTIONS');
  const [open, setOpen] = useState<string | 'new' | null>(null);
  if (!allowed)
    return (
      <>
        <SectionHeader title={t('admin.collections.title')} />
        <EmptyState
          title={t('admin.collections.noAccess.title')}
          description={t('admin.collections.noAccess.description')}
        />
      </>
    );
  if (open)
    return <CollectionEditor id={open === 'new' ? null : open} onClose={() => setOpen(null)} />;
  return <CollectionList onOpen={setOpen} />;
}

function CollectionList({ onOpen }: { onOpen: (id: string | 'new') => void }) {
  const t = useT();
  const res = useLoad('cinema-collections', api.listCollections);
  return (
    <>
      <SectionHeader
        title={t('admin.collections.title')}
        description={t('admin.collections.description')}
        actions={<Button onClick={() => onOpen('new')}>{t('admin.collections.new')}</Button>}
      />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what={t('admin.collections.whatCollections')}
        />
      )}
      {res.data && res.data.items.length === 0 && (
        <EmptyState
          title={t('admin.collections.empty.title')}
          description={t('admin.collections.empty.description')}
        />
      )}
      {res.data && res.data.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.collections.list')}>
          {[...res.data.items]
            .sort((a, b) => a.position - b.position)
            .map((c) => (
              <li key={c.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {c.name} <Badge>{c.display}</Badge>{' '}
                    <Badge variant={c.published ? 'success' : 'neutral'}>
                      {c.published
                        ? t('admin.collections.statusPublished')
                        : t('admin.collections.statusDraft')}
                    </Badge>
                  </div>
                  <div className="ss-meta">
                    {t('admin.collections.titleCount', { n: c.item_count })} ·{' '}
                    {c.surfaces.join(', ') || t('admin.collections.noSurface')}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.collections.editAria', { name: c.name })}
                  onClick={() => onOpen(c.id)}
                >
                  {t('admin.collections.edit')}
                </Button>
              </li>
            ))}
        </ul>
      )}
    </>
  );
}

function CollectionEditor({ id: initialId, onClose }: { id: string | null; onClose: () => void }) {
  const t = useT();
  const [id, setId] = useState(initialId);
  const res = useLoad<api.CollectionDetail | null>(`cinema-collection:${id ?? 'new'}`, async () =>
    id ? api.getCollection(id) : null,
  );
  const [remove, setRemove] = useState(false);
  const del = useRun();
  const c = res.data?.collection;
  return (
    <>
      <SectionHeader
        title={c ? c.name : t('admin.collections.newTitle')}
        actions={
          <>
            <Button variant="ghost" onClick={onClose}>
              {t('admin.collections.back')}
            </Button>
            {c && (
              <Button variant="danger" onClick={() => setRemove(true)}>
                {t('admin.collections.delete')}
              </Button>
            )}
          </>
        }
      />
      <Banner error={del.error} />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what={t('admin.collections.whatThis')}
        />
      )}
      {(!id || res.data) && (
        <>
          <CollectionForm
            key={`form-${c?.id ?? 'new'}`}
            collection={c}
            onSave={async (b) => {
              if (c) await api.updateCollection(c.id, b);
              else setId((await api.createCollection(b)).id);
              await res.reload();
            }}
          />
          {c && res.data && (
            <ItemsEditor
              key={`items-${c.id}`}
              collectionId={c.id}
              initial={res.data.items}
              onSaved={() => res.reload()}
            />
          )}
        </>
      )}
      <ConfirmDialog
        open={remove}
        danger
        title={t('admin.collections.deleteTitle', {
          name: c?.name ?? t('admin.collections.collectionWord'),
        })}
        description={t('admin.collections.deleteDescription')}
        confirmLabel={t('admin.collections.delete')}
        loading={del.busy}
        onCancel={() => setRemove(false)}
        onConfirm={() => {
          if (!c) return;
          void del
            .run(async () => {
              await api.deleteCollection(c.id);
              return true;
            })
            .then((ok) => {
              setRemove(false);
              if (ok) onClose();
            });
        }}
      />
    </>
  );
}

function CollectionForm({
  collection,
  onSave,
}: {
  collection: api.Collection | undefined;
  onSave: (b: api.CollectionInput) => Promise<void>;
}) {
  const t = useT();
  const displays = [
    { value: 'row', label: t('admin.collections.display.row') },
    { value: 'hero', label: t('admin.collections.display.hero') },
    { value: 'banner', label: t('admin.collections.display.banner') },
  ];
  const [name, setName] = useState(collection?.name ?? '');
  const [slug, setSlug] = useState(collection?.slug ?? '');
  const [slugTouched, setSlugTouched] = useState(!!collection);
  const [eyebrow, setEyebrow] = useState(collection?.eyebrow ?? '');
  const [blurb, setBlurb] = useState(collection?.blurb ?? '');
  const [display, setDisplay] = useState<string>(collection?.display ?? 'row');
  const [surfaces, setSurfaces] = useState<api.Surface[]>(collection?.surfaces ?? ['discover']);
  const [position, setPosition] = useState(String(collection?.position ?? 0));
  const [published, setPublished] = useState(collection?.published ?? false);
  const { busy, error, notice, run } = useRun();
  return (
    <form
      className="adm-form adm-cin-form"
      aria-label={t('admin.collections.form.aria')}
      onSubmit={(e) => {
        e.preventDefault();
        void run(
          () =>
            onSave({
              name: name.trim(),
              slug: slug.trim(),
              eyebrow: eyebrow.trim() || null,
              blurb: blurb.trim() || null,
              display: display as api.CollectionDisplay,
              surfaces,
              position: intOrNull(position) ?? 0,
              published,
            }).then(() => true),
          collection ? t('admin.collections.form.saved') : t('admin.collections.form.created'),
        );
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label={t('admin.collections.form.name')}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (!slugTouched) setSlug(slugify(e.target.value));
          }}
        />
        <TextInput
          label={t('admin.collections.form.slug')}
          value={slug}
          onChange={(e) => {
            setSlugTouched(true);
            setSlug(e.target.value);
          }}
        />
        <TextInput
          label={t('admin.collections.form.eyebrow')}
          value={eyebrow}
          onChange={(e) => setEyebrow(e.target.value)}
        />
        <Select
          aria-label={t('admin.collections.form.display')}
          options={displays}
          value={display}
          onValueChange={setDisplay}
        />
        <TextInput
          label={t('admin.collections.form.position')}
          type="number"
          value={position}
          onChange={(e) => setPosition(e.target.value)}
        />
      </div>
      <TextArea
        label={t('admin.collections.form.blurb')}
        rows={2}
        value={blurb}
        onChange={(e) => setBlurb(e.target.value)}
      />
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">{t('admin.collections.form.shownOn')}</legend>
        {api.SURFACES.map((s) => (
          <Switch
            key={s}
            label={t('admin.collections.form.surface', { name: s })}
            checked={surfaces.includes(s)}
            onChange={(e) =>
              setSurfaces((cur) => (e.target.checked ? [...cur, s] : cur.filter((x) => x !== s)))
            }
          />
        ))}
      </fieldset>
      <Switch
        label={t('admin.collections.form.published')}
        checked={published}
        onChange={(e) => setPublished(e.target.checked)}
      />
      <Banner error={error} notice={notice} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!name.trim() || !slug.trim()}>
          {collection ? t('admin.collections.form.save') : t('admin.collections.form.create')}
        </Button>
      </div>
    </form>
  );
}

export function move<T>(list: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item!);
  return next;
}

function ItemsEditor({
  collectionId,
  initial,
  onSaved,
}: {
  collectionId: string;
  initial: api.Title[];
  onSaved: () => Promise<void>;
}) {
  const t = useT();
  const [items, setItems] = useState(initial);
  const [dirty, setDirty] = useState(false);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [q, setQ] = useState('');
  const [found, setFound] = useState<api.Title[]>([]);
  const { busy, error, notice, run } = useRun();

  useEffect(() => {
    if (!dirty) setItems(initial);
  }, [initial, dirty]);
  useEffect(() => {
    if (q.trim().length < 2) {
      setFound([]);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      api
        .listTitles({ title: q.trim(), limit: 8 })
        .then((p) => live && setFound(p.items))
        .catch(() => live && setFound([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [q]);

  const apply = (next: api.Title[]) => {
    setItems(next);
    setDirty(true);
  };
  const inList = new Set(items.map((i) => i.id));

  return (
    <section className="ss-card" aria-label={t('admin.collections.items.aria')}>
      <h2 className="ss-sub">{t('admin.collections.items.heading')}</h2>
      {items.length === 0 ? (
        <p className="ss-na">{t('admin.collections.items.none')}</p>
      ) : (
        <ol className="ss-list" aria-label={t('admin.collections.items.ordered')}>
          {items.map((ti, i) => (
            <li
              key={ti.id}
              className={`ss-row${dragFrom === i ? ' ss-dragging' : ''}${over === i && dragFrom !== i ? ' ss-drop-target' : ''}`}
              draggable
              onDragStart={() => setDragFrom(i)}
              onDragOver={(e) => {
                e.preventDefault();
                setOver(i);
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (dragFrom !== null) apply(move(items, dragFrom, i));
                setDragFrom(null);
                setOver(null);
              }}
              onDragEnd={() => (setDragFrom(null), setOver(null))}
            >
              <span className="ss-dnd-handle" aria-hidden="true">
                ⠿
              </span>
              <span className="adm-cin-pos">{i + 1}</span>
              <div className="ss-grow">
                <div className="ss-name">{ti.title}</div>
                <div className="ss-meta">
                  {ti.kind} · {ti.year ?? t('admin.collections.noYear')} ·{' '}
                  {ti.published
                    ? t('admin.collections.statusPublished')
                    : t('admin.collections.statusDraft')}
                </div>
              </div>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('admin.collections.items.moveUpAria', { title: ti.title })}
                disabled={i === 0}
                onClick={() => apply(move(items, i, i - 1))}
              >
                {t('admin.collections.items.up')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('admin.collections.items.moveDownAria', { title: ti.title })}
                disabled={i === items.length - 1}
                onClick={() => apply(move(items, i, i + 1))}
              >
                {t('admin.collections.items.down')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('admin.collections.items.removeAria', { title: ti.title })}
                onClick={() => apply(items.filter((x) => x.id !== ti.id))}
              >
                {t('admin.collections.items.remove')}
              </Button>
            </li>
          ))}
        </ol>
      )}
      <TextInput
        label={t('admin.collections.items.addLabel')}
        type="search"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {found.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.collections.items.results')}>
          {found.map((ti) => (
            <li key={ti.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">{ti.title}</div>
                <div className="ss-meta">
                  {ti.kind} · {ti.year ?? t('admin.collections.noYear')} ·{' '}
                  {ti.published
                    ? t('admin.collections.statusPublished')
                    : t('admin.collections.items.draftHidden')}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                disabled={inList.has(ti.id)}
                aria-label={t('admin.collections.items.addAria', { title: ti.title })}
                onClick={() => apply([...items, ti])}
              >
                {inList.has(ti.id)
                  ? t('admin.collections.items.added')
                  : t('admin.collections.items.add')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <Banner error={error} notice={notice} />
      <div className="adm-actions">
        <Button
          loading={busy}
          disabled={!dirty}
          onClick={() =>
            void run(async () => {
              await api.setCollectionItems(
                collectionId,
                items.map((i) => i.id),
              );
              setDirty(false);
              await onSaved();
              return true;
            }, t('admin.collections.items.saved'))
          }
        >
          {t('admin.collections.items.saveOrder')}
        </Button>
        <Button
          variant="ghost"
          disabled={!dirty || busy}
          onClick={() => {
            setItems(initial);
            setDirty(false);
          }}
        >
          {t('admin.collections.items.revert')}
        </Button>
      </div>
    </section>
  );
}
