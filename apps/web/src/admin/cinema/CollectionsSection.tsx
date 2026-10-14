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
import { SectionHeader, useAdminViewer } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, intOrNull, useLoad, useRun } from './hooks';

const DISPLAYS = [
  { value: 'row', label: 'Row' },
  { value: 'hero', label: 'Hero spotlight' },
  { value: 'banner', label: 'Banner' },
];
const slugify = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');

export function CollectionsSection() {
  const allowed = useAdminViewer().can('MANAGE_COLLECTIONS');
  const [open, setOpen] = useState<string | 'new' | null>(null);
  if (!allowed)
    return (
      <>
        <SectionHeader title="Cinema collections" />
        <EmptyState
          title="You do not have access"
          description="Collections take the Manage collections permission (Media Curator)."
        />
      </>
    );
  if (open)
    return <CollectionEditor id={open === 'new' ? null : open} onClose={() => setOpen(null)} />;
  return <CollectionList onOpen={setOpen} />;
}

function CollectionList({ onOpen }: { onOpen: (id: string | 'new') => void }) {
  const res = useLoad('cinema-collections', api.listCollections);
  return (
    <>
      <SectionHeader
        title="Cinema collections"
        description="Curated rows and spotlights that appear on Discover and the browse pages, in position order."
        actions={<Button onClick={() => onOpen('new')}>New collection</Button>}
      />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what="collections"
        />
      )}
      {res.data && res.data.items.length === 0 && (
        <EmptyState title="No collections" description="Create the first curated row." />
      )}
      {res.data && res.data.items.length > 0 && (
        <ul className="ss-list" aria-label="Collections">
          {[...res.data.items]
            .sort((a, b) => a.position - b.position)
            .map((c) => (
              <li key={c.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {c.name} <Badge>{c.display}</Badge>{' '}
                    <Badge variant={c.published ? 'success' : 'neutral'}>
                      {c.published ? 'published' : 'draft'}
                    </Badge>
                  </div>
                  <div className="ss-meta">
                    {c.item_count} title{c.item_count === 1 ? '' : 's'} ·{' '}
                    {c.surfaces.join(', ') || 'no surface'}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Edit collection ${c.name}`}
                  onClick={() => onOpen(c.id)}
                >
                  Edit
                </Button>
              </li>
            ))}
        </ul>
      )}
    </>
  );
}

function CollectionEditor({ id: initialId, onClose }: { id: string | null; onClose: () => void }) {
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
        title={c ? c.name : 'New collection'}
        actions={
          <>
            <Button variant="ghost" onClick={onClose}>
              Back to collections
            </Button>
            {c && (
              <Button variant="danger" onClick={() => setRemove(true)}>
                Delete collection
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
          what="this collection"
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
        title={`Delete ${c?.name ?? 'collection'}`}
        description="The row disappears from Discover. The titles themselves are not touched."
        confirmLabel="Delete collection"
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
      aria-label="Collection details"
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
          collection ? 'Saved.' : 'Collection created.',
        );
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label="Collection name"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (!slugTouched) setSlug(slugify(e.target.value));
          }}
        />
        <TextInput
          label="Slug"
          value={slug}
          onChange={(e) => {
            setSlugTouched(true);
            setSlug(e.target.value);
          }}
        />
        <TextInput label="Eyebrow" value={eyebrow} onChange={(e) => setEyebrow(e.target.value)} />
        <Select
          aria-label="Display"
          options={DISPLAYS}
          value={display}
          onValueChange={setDisplay}
        />
        <TextInput
          label="Position"
          type="number"
          value={position}
          onChange={(e) => setPosition(e.target.value)}
        />
      </div>
      <TextArea label="Blurb" rows={2} value={blurb} onChange={(e) => setBlurb(e.target.value)} />
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">Shown on</legend>
        {api.SURFACES.map((s) => (
          <Switch
            key={s}
            label={`Surface ${s}`}
            checked={surfaces.includes(s)}
            onChange={(e) =>
              setSurfaces((cur) => (e.target.checked ? [...cur, s] : cur.filter((x) => x !== s)))
            }
          />
        ))}
      </fieldset>
      <Switch
        label="Published"
        checked={published}
        onChange={(e) => setPublished(e.target.checked)}
      />
      <Banner error={error} notice={notice} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!name.trim() || !slug.trim()}>
          {collection ? 'Save collection' : 'Create collection'}
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
    const t = setTimeout(() => {
      api
        .listTitles({ title: q.trim(), limit: 8 })
        .then((p) => live && setFound(p.items))
        .catch(() => live && setFound([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [q]);

  const apply = (next: api.Title[]) => {
    setItems(next);
    setDirty(true);
  };
  const inList = new Set(items.map((i) => i.id));

  return (
    <section className="ss-card" aria-label="Collection titles">
      <h2 className="ss-sub">Titles, in order</h2>
      {items.length === 0 ? (
        <p className="ss-na">No titles yet. Search below to add some.</p>
      ) : (
        <ol className="ss-list" aria-label="Ordered titles">
          {items.map((t, i) => (
            <li
              key={t.id}
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
                <div className="ss-name">{t.title}</div>
                <div className="ss-meta">
                  {t.kind} · {t.year ?? 'no year'} · {t.published ? 'published' : 'draft'}
                </div>
              </div>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Move ${t.title} up`}
                disabled={i === 0}
                onClick={() => apply(move(items, i, i - 1))}
              >
                Up
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Move ${t.title} down`}
                disabled={i === items.length - 1}
                onClick={() => apply(move(items, i, i + 1))}
              >
                Down
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Remove ${t.title}`}
                onClick={() => apply(items.filter((x) => x.id !== t.id))}
              >
                Remove
              </Button>
            </li>
          ))}
        </ol>
      )}
      <TextInput
        label="Add a title"
        type="search"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {found.length > 0 && (
        <ul className="ss-list" aria-label="Search results">
          {found.map((t) => (
            <li key={t.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">{t.title}</div>
                <div className="ss-meta">
                  {t.kind} · {t.year ?? 'no year'} ·{' '}
                  {t.published ? 'published' : 'draft (hidden until published)'}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                disabled={inList.has(t.id)}
                aria-label={`Add ${t.title}`}
                onClick={() => apply([...items, t])}
              >
                {inList.has(t.id) ? 'Added' : 'Add'}
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
            }, 'Order saved.')
          }
        >
          Save order
        </Button>
        <Button
          variant="ghost"
          disabled={!dirty || busy}
          onClick={() => {
            setItems(initial);
            setDirty(false);
          }}
        >
          Revert
        </Button>
      </div>
    </section>
  );
}
