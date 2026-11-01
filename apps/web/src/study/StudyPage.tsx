import { Notice } from '@sideroom/ui';
import { useQueries, useQuery } from '@tanstack/react-query';
import { useCallback, useMemo, useState } from 'react';
import { apiFetch } from '../api/client';
import { useT } from '../i18n';
import { useStore } from '../store/store';
import type { ObjectEntry } from '../storage/api';
import { studyKeys, visibleShelves } from './api';
import { baseName, titleOf } from './fileTypes';
import {
  continueRails,
  loadOpened,
  loadPins,
  recentlyAdded,
  recordOpened,
  writeOpened,
  savePins,
  togglePin,
  type Opened,
  type Pin,
} from './rails';
import { RoomsNav } from './RoomsNav';
import { StudyAreas } from './StudyAreas';
import { StudyFiles } from './StudyFiles';
import {
  allEntries,
  buildSidebar,
  entryOfMount,
  provenanceOf,
  sortEntries,
  type Provenance,
  type StudyEntry,
  type VisibleSource,
} from './sources';
import './study.css';

const RECENT_SHELVES = 6;

/** Every object below the root of a shelf, one page (the Recently Added rail). */
const listRecursive = (mountId: string) =>
  apiFetch<{ objects: ObjectEntry[] }>(
    `/storage/mounts/${encodeURIComponent(mountId)}/objects?prefix=&delimiter=`,
    { retries: 0 },
  );

function useNames() {
  const users = useStore((s) => s.users);
  const guilds = useStore((s) => s.guilds);
  const self = useStore((s) => s.currentUserId);
  const handleOf = useCallback(
    (id: string) => {
      const u = users[id];
      return u?.username ?? null;
    },
    [users],
  );
  const guildNameOf = useCallback((id: string) => guilds[id]?.name ?? null, [guilds]);
  return { handleOf, guildNameOf, self: self ?? null };
}

/** The words of a provenance badge. */
export function useProvenanceText() {
  const t = useT();
  return (p: Provenance): string => {
    if (p.kind === 'guild') {
      return p.name ? t('study.badge.guild', { name: p.name }) : t('study.badge.guildUnknown');
    }
    if (p.handle) return t('study.badge.bucket', { handle: p.handle });
    return p.kind === 'own' ? t('study.badge.own') : t('study.badge.friendUnknown');
  };
}

interface Selection {
  entryId: string;
  mountId: string;
  prefix: string;
}

/**
 * The Study (STU-SHF-001): a sidebar by source (My Files, Friends' Shelves, Guild Shelves), the files of the chosen
 * shelf with type filters and covers taken from the files, and three personal rails. The shelves come through one
 * `VisibleSource` (`GET /shelves/visible` by default); file bodies through per-object URLs (`./objectUrl`).
 */
export function StudyPage({ source = visibleShelves }: { source?: VisibleSource }) {
  const t = useT();
  const names = useNames();
  const text = useProvenanceText();
  const visible = useQuery({ queryKey: studyKeys.visible, queryFn: source, retry: false });
  const sidebar = useMemo(() => buildSidebar(visible.data ?? []), [visible.data]);
  const nameOf = useCallback(
    (e: StudyEntry) =>
      (e.group === 'friends'
        ? (names.handleOf(e.ownerId ?? '') ?? e.ownerHandle)
        : (names.guildNameOf(e.guildId ?? '') ?? e.guildName)) ??
      e.mounts[0]?.label ??
      '',
    [names],
  );
  const friends = useMemo(() => sortEntries(sidebar.friends, nameOf), [sidebar.friends, nameOf]);
  const guilds = useMemo(() => sortEntries(sidebar.guilds, nameOf), [sidebar.guilds, nameOf]);

  const uid = names.self ?? 'anon';
  const [opened, setOpened] = useState<Opened[]>(() => loadOpened(uid));
  const [pins, setPins] = useState<Pin[]>(() => loadPins(uid));
  const [sel, setSel] = useState<Selection | null>(null);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [resume, setResume] = useState<Opened | undefined>();

  const entries = allEntries({ mine: sidebar.mine, friends, guilds });
  const current = (sel && entries.find((e) => e.id === sel.entryId)) ?? entries[0] ?? null;
  const mountId =
    sel && current?.mounts.some((m) => m.mountId === sel.mountId)
      ? sel.mountId
      : current?.mounts[0]?.mountId;
  const prefix = sel && sel.mountId === mountId ? sel.prefix : '';
  const mount = current?.mounts.find((m) => m.mountId === mountId);

  const select = (e: StudyEntry) =>
    setSel({ entryId: e.id, mountId: e.mounts[0]!.mountId, prefix: '' });
  const goto = (id: string, key: string | null, prefixPath: string, r?: Opened) => {
    const e = entryOfMount(sidebar, id);
    if (!e) return;
    setSel({ entryId: e.id, mountId: id, prefix: prefixPath });
    setOpenKey(key);
    setResume(r);
  };

  const onOpened = (o: Omit<Opened, 'updatedAt'>) => {
    setOpened((list) => {
      const prev = list.find((x) => x.mountId === o.mountId && x.key === o.key);
      const keep = prev && o.at === 0 && o.of === 0;
      const rec = { ...o, ...(keep ? { at: prev.at, of: prev.of } : {}), updatedAt: Date.now() };
      const next = recordOpened(list, rec);
      writeOpened(uid, next);
      return next;
    });
  };
  const flipPin = (p: Pin) =>
    setPins((list) => {
      const next = togglePin(list, p);
      savePins(uid, next);
      return next;
    });

  // Recently Added: the newest files of up to six shelves the viewer reaches.
  const recentMounts = entries
    .flatMap((e) => e.mounts.map((m) => m.mountId))
    .slice(0, RECENT_SHELVES);
  const recentQ = useQueries({
    queries: recentMounts.map((id) => ({
      queryKey: studyKeys.recent(id),
      queryFn: () => listRecursive(id),
      retry: false,
      staleTime: 60_000,
    })),
  });
  const recent = recentlyAdded(
    recentQ.map((q, i) => ({ mountId: recentMounts[i]!, objects: q.data?.objects ?? [] })),
  );
  const rails = continueRails(opened.filter((o) => entryOfMount(sidebar, o.mountId)));
  const livePins = pins.filter((p) => entryOfMount(sidebar, p.mountId));

  const prov = current
    ? provenanceOf(current, names.handleOf, names.guildNameOf, names.self)
    : null;
  const badgeKind = current
    ? current.group === 'mine'
      ? 'own'
      : current.group === 'friends'
        ? 'friend'
        : 'guild'
    : 'own';

  const entryLabel = (e: StudyEntry) =>
    e.group === 'mine' ? t('study.side.mine') : nameOf(e) || t('study.side.unknown');

  const group = (title: string, list: StudyEntry[]) =>
    list.length > 0 && (
      <section className="stu-group" aria-label={title}>
        <h2>{title}</h2>
        <ul>
          {list.map((e) => (
            <li key={e.id}>
              <button
                type="button"
                className="stu-entry"
                aria-current={current?.id === e.id ? 'true' : undefined}
                onClick={() => select(e)}
              >
                {entryLabel(e)}
              </button>
            </li>
          ))}
        </ul>
      </section>
    );

  return (
    <>
      <header className="stu-bar">
        <RoomsNav current="study" className="stu-bar-nav" linkClassName="stu-bar-link" />
        <StudyAreas current="shelves" className="stu-bar-nav" linkClassName="stu-bar-link" />
      </header>
      <div className="stu-page">
        <nav className="stu-side" aria-label={t('study.side.label')}>
          <h1>{t('study.title')}</h1>
          {visible.isPending && <p className="stu-muted">{t('study.loading')}</p>}
          {visible.isError && <Notice variant="danger">{t('study.error')}</Notice>}
          {sidebar.mine && group(t('study.side.mineGroup'), [sidebar.mine])}
          {group(t('study.side.friends'), friends)}
          {group(t('study.side.guilds'), guilds)}
        </nav>
        <main className="stu-main">
          {visible.isSuccess && entries.length === 0 && (
            <Notice variant="info">{t('study.noShelves')}</Notice>
          )}
          <Rails
            watching={rails.watching}
            reading={rails.reading}
            recent={recent.map((r) => ({ mountId: r.mountId, key: r.key }))}
            pins={livePins}
            onOpen={(o, r) => goto(o.mountId, o.key, o.key.slice(0, o.key.lastIndexOf('/') + 1), r)}
            onPin={(p) => goto(p.mountId, null, p.prefix)}
          />
          {current && mount && mountId && prov && (
            <>
              {current.mounts.length > 1 && (
                <div className="stu-mounts" role="group" aria-label={t('study.mounts')}>
                  {current.mounts.map((m) => (
                    <button
                      key={m.mountId}
                      type="button"
                      aria-pressed={m.mountId === mountId}
                      onClick={() =>
                        setSel({ entryId: current.id, mountId: m.mountId, prefix: '' })
                      }
                    >
                      {m.label}
                    </button>
                  ))}
                </div>
              )}
              <StudyFiles
                key={mountId}
                mountId={mountId}
                mountLabel={mount.label}
                prefix={prefix}
                onPrefix={(p) => setSel({ entryId: current.id, mountId, prefix: p })}
                badge={text(prov)}
                badgeKind={badgeKind}
                pinned={pins.some((p) => p.mountId === mountId && p.prefix === prefix)}
                onTogglePin={flipPin}
                onOpened={onOpened}
                resume={resume}
                openRequest={openKey}
                onOpenDone={() => {
                  setOpenKey(null);
                  setResume(undefined);
                }}
              />
            </>
          )}
        </main>
      </div>
    </>
  );
}

function Rails({
  watching,
  reading,
  recent,
  pins,
  onOpen,
  onPin,
}: {
  watching: Opened[];
  reading: Opened[];
  recent: { mountId: string; key: string }[];
  pins: Pin[];
  onOpen: (o: { mountId: string; key: string }, resume?: Opened) => void;
  onPin: (p: Pin) => void;
}) {
  const t = useT();
  const rail = (title: string, items: { id: string; label: string; go: () => void }[]) =>
    items.length > 0 && (
      <section className="stu-rail" aria-label={title}>
        <h2>{title}</h2>
        <ul>
          {items.map((i) => (
            <li key={i.id}>
              <button type="button" onClick={i.go}>
                {i.label}
              </button>
            </li>
          ))}
        </ul>
      </section>
    );
  return (
    <div className="stu-rails">
      {rail(
        t('study.rail.watching'),
        watching.map((o) => ({
          id: `${o.mountId}:${o.key}`,
          label: o.title,
          go: () => onOpen(o, o),
        })),
      )}
      {rail(
        t('study.rail.reading'),
        reading.map((o) => ({
          id: `${o.mountId}:${o.key}`,
          label: o.title,
          go: () => onOpen(o, o),
        })),
      )}
      {rail(
        t('study.rail.recent'),
        recent.map((r) => ({
          id: `${r.mountId}:${r.key}`,
          label: titleOf(r.key) || baseName(r.key),
          go: () => onOpen(r),
        })),
      )}
      {rail(
        t('study.rail.pinned'),
        pins.map((p) => ({ id: `${p.mountId}:${p.prefix}`, label: p.label, go: () => onPin(p) })),
      )}
    </div>
  );
}
