// D-466: the app's own shelf picker for an Activity's `pickShelfObject`. Rendered by the app, never inside the frame,
// like the consent dialogs. It lists the shelves the member can open and the files in one shelf (the catalog the
// server already cuts to what they may open), filtered by the Activity's `accept` and `max_size`. What it returns is
// the one chosen object; the Activity never receives a listing.
import { Button, Modal, Select, Spinner, TextInput } from '@sideroom/ui';
import type { ShelfObjectPick } from '@sideroom/embedded-app-sdk';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import { visibleShelves } from '../study/api';
import type { VisibleShelf } from '../study/sources';
import { shelfCatalogPage, type ShelfCatalogObject, type ShelfCatalogPage } from './api';
import { contentTypeOf, fileName, pickable } from './shelfPick';
import './activities.css';

/** Most catalog pages (200 objects each) read for one shelf. */
const MAX_PAGES = 15;
/** Objects in one catalog page. */
const PAGE_SIZE = 200;
/** D-537: the stated cap; a shelf past it lists only this many. */
export const MAX_LISTED = MAX_PAGES * PAGE_SIZE;
/** Longest search the server takes (it refuses more). */
const MAX_QUERY = 100;
/** Quiet time after the last keystroke before the server is asked. */
const SEARCH_DEBOUNCE_MS = 300;
/** Most rows drawn at once; the search narrows the rest. */
const MAX_ROWS = 200;

export interface PickRequest {
  accept: string[];
  maxSize: number | null;
}

const fmtSize = (n: number): string => {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
};

export default function ShelfPicker({
  appName,
  req,
  onDone,
  ownOnly = false,
  title,
  description,
}: {
  appName: string;
  req: PickRequest;
  /** List only the member's own shelves (a profile showcase names only their own mounts, D-502). */
  ownOnly?: boolean;
  /** Replaces the Activity wording of the dialog's title and description. */
  title?: string;
  description?: string;
  /** The chosen object, or `null` for cancel / closing the dialog. */
  onDone: (pick: ShelfObjectPick | null) => void;
}) {
  const t = useT();
  const [shelves, setShelves] = useState<VisibleShelf[] | null>(null);
  const [shelf, setShelf] = useState('');
  const [files, setFiles] = useState<ShelfCatalogObject[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [truncated, setTruncated] = useState(false);
  const [query, setQuery] = useState('');
  // P51: the server searches file names once the member stops typing; the list filters instantly meanwhile.
  const [term, setTerm] = useState('');
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    let dead = false;
    visibleShelves().then(
      (all) => {
        if (dead) return;
        const s = ownOnly ? all.filter((x) => x.kind === 'own') : all;
        setShelves(s);
        setShelf((cur) => cur || (s[0]?.shelf_id ?? ''));
      },
      () => {
        if (dead) return;
        setShelves([]);
        setFailed(true);
      },
    );
    return () => {
      dead = true;
    };
  }, [ownOnly]);

  useEffect(() => {
    const next = query.trim().slice(0, MAX_QUERY);
    if (next === term) return;
    const id = setTimeout(() => setTerm(next), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [query, term]);

  // A new shelf starts from its plain listing.
  useEffect(() => {
    setFiles(null);
    setTerm('');
    setQuery('');
  }, [shelf]);

  useEffect(() => {
    if (!shelf) return;
    let dead = false;
    setSearching(true);
    setFailed(false);
    setTruncated(false);
    void (async () => {
      try {
        const all: ShelfCatalogObject[] = [];
        let cursor: string | null = null;
        for (let i = 0; i < MAX_PAGES; i++) {
          const page: ShelfCatalogPage = await shelfCatalogPage(shelf, cursor, term || undefined);
          if (dead) return;
          all.push(...page.objects);
          cursor = page.next_cursor;
          if (!cursor) break;
        }
        if (!dead) {
          setFiles(all);
          setSearching(false);
          // The last page still had a next page: the shelf is longer than the picker reads (D-537).
          setTruncated(cursor !== null);
        }
      } catch {
        if (!dead) {
          setFiles([]);
          setFailed(true);
          setSearching(false);
        }
      }
    })();
    return () => {
      dead = true;
    };
  }, [shelf, term]);

  const options = useMemo(
    () =>
      (shelves ?? []).map((s) => ({
        value: s.shelf_id,
        label: s.label,
        description:
          s.kind === 'own'
            ? t('activities.pick.mine')
            : s.kind === 'friend'
              ? t('activities.pick.friend', { name: s.owner_handle ? `@${s.owner_handle}` : '…' })
              : t('activities.pick.guild', { name: s.guild_name ?? '…' }),
      })),
    [shelves, t],
  );

  const { rows, hidden } = useMemo(() => {
    const ok: ShelfCatalogObject[] = [];
    let skipped = 0;
    for (const f of files ?? []) {
      if (pickable(fileName(f.path), f.size, req)) ok.push(f);
      else skipped += 1;
    }
    ok.sort((a, b) => a.path.localeCompare(b.path));
    return { rows: ok, hidden: skipped };
  }, [files, req]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q
      ? rows.filter((f) => f.path.toLowerCase().includes(q) || f.title.toLowerCase().includes(q))
      : rows;
  }, [rows, query]);

  const choose = (f: ShelfCatalogObject) => {
    const name = fileName(f.path);
    onDone({
      shelf_id: shelf,
      object_hash: f.object_hash,
      name,
      size: f.size,
      content_type: contentTypeOf(name),
    });
  };

  const noShelves = shelves !== null && shelves.length === 0;

  return (
    <Modal
      open
      onClose={() => onDone(null)}
      title={title ?? t('activities.pick.title', { name: appName })}
      description={description ?? t('activities.pick.description')}
      size="md"
      footer={
        <Button variant="ghost" onClick={() => onDone(null)}>
          {t('activities.pick.cancel')}
        </Button>
      }
    >
      <div className="act-pick" aria-busy={searching}>
        {noShelves ? (
          <p className="act-muted">
            {t(failed ? 'activities.pick.error' : 'activities.pick.noShelves')}
          </p>
        ) : (
          <>
            <div className="act-pick-fields">
              <Select
                aria-label={t('activities.pick.shelf')}
                placeholder={t('activities.pick.shelfPlaceholder')}
                options={options}
                value={shelf || null}
                onValueChange={setShelf}
                rowHeight={52}
              />
              <TextInput
                label={t('activities.pick.search')}
                placeholder={t('activities.pick.searchPlaceholder')}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </div>
            {files === null ? (
              <p className="act-muted act-pick-status" role="status">
                <Spinner size="sm" label={t('activities.pick.loading')} />
                {t('activities.pick.loading')}
              </p>
            ) : failed ? (
              <p className="act-muted" role="alert">
                {t('activities.pick.error')}
              </p>
            ) : shown.length === 0 ? (
              <p className="act-muted">
                {t(rows.length === 0 ? 'activities.pick.empty' : 'activities.pick.noMatch')}
              </p>
            ) : (
              <ul className="act-pick-list" aria-label={t('activities.pick.files')}>
                {shown.slice(0, MAX_ROWS).map((f) => (
                  <li key={f.object_hash}>
                    <button type="button" className="act-pick-row" onClick={() => choose(f)}>
                      <span className="act-pick-name">{f.title || fileName(f.path)}</span>
                      <span className="act-pick-meta">
                        {f.path !== fileName(f.path) ? `${f.path} · ` : ''}
                        {fmtSize(f.size)}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {files !== null && !failed && shown.length > MAX_ROWS ? (
              <p className="act-muted act-pick-note">
                {t('activities.pick.more', { count: MAX_ROWS })}
              </p>
            ) : null}
            {files !== null && !failed && truncated ? (
              <p className="act-muted act-pick-note" role="note">
                {t(term ? 'activities.pick.searchTruncated' : 'activities.pick.truncated', {
                  count: MAX_LISTED.toLocaleString(),
                })}
              </p>
            ) : null}
            {files !== null && !failed && hidden > 0 ? (
              <p className="act-muted act-pick-note">
                {t('activities.pick.hidden', { count: hidden })}
              </p>
            ) : null}
          </>
        )}
      </div>
    </Modal>
  );
}
