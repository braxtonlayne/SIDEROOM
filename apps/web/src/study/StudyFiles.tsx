import { Button, EmptyState, FilterChips, Notice, TextInput } from '@sideroom/ui';
import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useT } from '../i18n';
import { listObjects, type ObjectEntry } from '../storage/api';
import { studyKeys } from './api';
import {
  baseName,
  countByType,
  extOf,
  filterByType,
  STUDY_TYPES,
  typeOf,
  type StudyType,
} from './fileTypes';
import {
  displayTitle,
  listNotes,
  noteMap,
  putNote,
  withNote,
  type FileNote,
  type NoteMap,
} from './notes';
import { NoteEditor, StudyViewer } from './StudyDialogs';
import { useCover, useNearViewport } from './useCover';
import type { Opened, Pin } from './rails';

export const sizeText = (n: number) =>
  n < 1024
    ? `${n} B`
    : n < 1024 ** 2
      ? `${(n / 1024).toFixed(1)} KiB`
      : n < 1024 ** 3
        ? `${(n / 1024 ** 2).toFixed(1)} MiB`
        : `${(n / 1024 ** 3).toFixed(1)} GiB`;

/** The badge every card and row carries: whose bucket, or which guild. */
export function ProvenanceBadge({
  text,
  kind,
}: {
  text: string;
  kind: 'own' | 'friend' | 'guild';
}) {
  return (
    <span className="stu-badge" data-kind={kind}>
      {text}
    </span>
  );
}

function FileCard({
  mountId,
  file,
  notes,
  badge,
  badgeKind,
  canEdit,
  onOpen,
  onEdit,
}: {
  mountId: string;
  file: ObjectEntry;
  notes: NoteMap;
  badge: string;
  badgeKind: 'own' | 'friend' | 'guild';
  canEdit: boolean;
  onOpen: () => void;
  onEdit: () => void;
}) {
  const t = useT();
  const { ref, near } = useNearViewport<HTMLLIElement>();
  const note = notes.get(file.key);
  const cover = useCover(mountId, file.key, file.size, note?.cover_key, near);
  const title = displayTitle(file.key, notes);
  const type = typeOf(file.key);
  return (
    <li className="stu-card" ref={ref}>
      <button
        type="button"
        className="stu-open"
        onClick={onOpen}
        aria-label={t('study.file.open', { title, badge })}
      >
        <span className="stu-cover">
          {cover ? (
            <img src={cover} alt="" loading="lazy" draggable={false} />
          ) : (
            <span className="stu-glyph">
              {extOf(file.key).toUpperCase() || t(`study.type.${type}` as 'study.type.other')}
            </span>
          )}
        </span>
        <span className="stu-text">
          <span className="stu-title">{title}</span>
          <span className="stu-sub">
            {t(`study.type.${type}` as 'study.type.other')} · {sizeText(file.size)}
          </span>
        </span>
      </button>
      <ProvenanceBadge text={badge} kind={badgeKind} />
      {note?.note && <p className="stu-note">{note.note}</p>}
      {canEdit && (
        // The visible label stays short (a long file name stretched the card to twice the others); the name is in
        // the accessible name.
        <Button
          variant="secondary"
          size="sm"
          aria-label={t('study.file.edit', { title })}
          onClick={onEdit}
        >
          {t('study.edit.title')}
        </Button>
      )}
    </li>
  );
}

/**
 * The files of one shelf (STU-SHF-001): folders, a type filter, a name search, covers taken from the files, the
 * provenance badge on every card, and the owner's editor. The server lists only what the viewer may read.
 */
export function StudyFiles({
  mountId,
  mountLabel,
  prefix,
  onPrefix,
  badge,
  badgeKind,
  pinned,
  onTogglePin,
  onOpened,
  resume,
  openRequest,
  onOpenDone,
}: {
  mountId: string;
  mountLabel: string;
  prefix: string;
  onPrefix: (p: string) => void;
  badge: string;
  badgeKind: 'own' | 'friend' | 'guild';
  pinned: boolean;
  onTogglePin: (p: Pin) => void;
  onOpened: (o: Omit<Opened, 'updatedAt'>) => void;
  /** A file opened from a rail: shown at once. */
  resume?: Opened;
  openRequest?: string | null;
  onOpenDone: () => void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [type, setType] = useState<StudyType | null>(null);
  const [q, setQ] = useState('');
  const [viewing, setViewing] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const dir = useInfiniteQuery({
    queryKey: studyKeys.dir(mountId, prefix),
    queryFn: ({ pageParam }) => listObjects(mountId, prefix, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor,
    retry: false,
    placeholderData: keepPreviousData,
  });
  const notesQ = useQuery({
    queryKey: studyKeys.notes(mountId),
    queryFn: () => listNotes(mountId),
    retry: false,
  });
  const notes = useMemo(() => noteMap(notesQ.data), [notesQ.data]);
  const canEdit = notesQ.data?.can_edit ?? false;

  const pages = dir.data?.pages ?? [];
  const folders = pages.flatMap((p) => p.folders);
  const objects = pages.flatMap((p) => p.objects).filter((o) => !o.key.endsWith('/'));
  const counts = countByType(objects);
  const needle = q.trim().toLowerCase();
  const shown = filterByType(objects, type).filter(
    (o) => !needle || displayTitle(o.key, notes).toLowerCase().includes(needle),
  );
  const shownFolders = folders.filter((f) => !needle || baseName(f).toLowerCase().includes(needle));

  const save = useMutation({
    mutationFn: ({ key, n }: { key: string; n: FileNote | null }) => putNote(mountId, n ?? { key }),
    onSuccess: (_d, { key, n }) => {
      qc.setQueryData(studyKeys.notes(mountId), (old: typeof notesQ.data) => ({
        can_edit: old?.can_edit ?? true,
        notes: [...withNote(noteMap(old), key, n).values()],
      }));
      setEditing(null);
      setSaveError(null);
    },
    onError: () => setSaveError(t('study.edit.failed')),
  });

  const open = viewing ?? openRequest ?? null;
  const crumbs = prefix.split('/').filter(Boolean);
  const titleOfOpen = open ? displayTitle(open, notes) : '';

  return (
    <section className="stu-files" aria-label={mountLabel}>
      <nav className="stu-crumbs" aria-label={t('study.crumbs')}>
        <button type="button" onClick={() => onPrefix('')}>
          {mountLabel}
        </button>
        {crumbs.map((c, i) => (
          <span key={i}>
            {' / '}
            <button type="button" onClick={() => onPrefix(`${crumbs.slice(0, i + 1).join('/')}/`)}>
              {c}
            </button>
          </span>
        ))}
        {prefix && (
          <Button
            variant="secondary"
            size="sm"
            aria-pressed={pinned}
            onClick={() =>
              onTogglePin({ mountId, prefix, label: crumbs[crumbs.length - 1] ?? mountLabel })
            }
          >
            {pinned ? t('study.pin.unpin') : t('study.pin.pin')}
          </Button>
        )}
      </nav>
      {/* Search and the type chips only once the folder is read: "Video (0)" next to a load error read as an
          empty folder. */}
      {dir.isSuccess && (
        <>
          <div className="stu-tools">
            <TextInput
              label={t('study.search')}
              type="search"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <FilterChips
            label={t('study.filter.label')}
            value={type}
            onValueChange={(v) => setType(v as StudyType | null)}
            options={STUDY_TYPES.map((k) => ({
              value: k,
              label: `${t(`study.type.${k}` as 'study.type.other')} (${counts[k]})`,
            }))}
          />
        </>
      )}
      {dir.isPending && <p className="stu-muted">{t('study.loading')}</p>}
      {dir.isError && (
        <Notice variant="danger">
          <span className="stu-error">
            {t('study.error')}
            <Button
              variant="secondary"
              size="sm"
              loading={dir.isFetching}
              onClick={() => void dir.refetch()}
            >
              {t('study.retry')}
            </Button>
          </span>
        </Notice>
      )}
      {dir.isSuccess && shown.length === 0 && shownFolders.length === 0 && (
        <EmptyState
          title={
            objects.length === 0 && folders.length === 0
              ? t('study.empty.title')
              : t('study.noMatch')
          }
          description={
            objects.length === 0 && folders.length === 0 ? t('study.empty.body') : undefined
          }
        />
      )}
      {shownFolders.length > 0 && (
        <ul className="stu-folders">
          {shownFolders.map((f) => (
            <li key={f}>
              <button type="button" className="stu-folder" onClick={() => onPrefix(f)}>
                {baseName(f)}
              </button>
            </li>
          ))}
        </ul>
      )}
      {shown.length > 0 && (
        <ul className="stu-grid">
          {shown.map((o) => (
            <FileCard
              key={o.key}
              mountId={mountId}
              file={o}
              notes={notes}
              badge={badge}
              badgeKind={badgeKind}
              canEdit={canEdit}
              onOpen={() => {
                setViewing(o.key);
                onOpened({ mountId, key: o.key, title: displayTitle(o.key, notes), at: 0, of: 0 });
              }}
              onEdit={() => {
                setSaveError(null);
                setEditing(o.key);
              }}
            />
          ))}
        </ul>
      )}
      {dir.hasNextPage && (
        <Button
          variant="secondary"
          loading={dir.isFetchingNextPage}
          onClick={() => void dir.fetchNextPage()}
        >
          {t('study.more')}
        </Button>
      )}
      {open && (
        <StudyViewer
          mountId={mountId}
          fileKey={open}
          title={titleOfOpen}
          resume={resume && resume.key === open ? resume : undefined}
          onClose={() => {
            setViewing(null);
            onOpenDone();
          }}
          onProgress={(at, of) => onOpened({ mountId, key: open, title: titleOfOpen, at, of })}
        />
      )}
      {editing && (
        <NoteEditor
          fileKey={editing}
          current={notes.get(editing)}
          images={objects.map((o) => o.key)}
          saving={save.isPending}
          error={saveError}
          onSave={(n) => save.mutate({ key: editing, n })}
          onClose={() => setEditing(null)}
        />
      )}
    </section>
  );
}
