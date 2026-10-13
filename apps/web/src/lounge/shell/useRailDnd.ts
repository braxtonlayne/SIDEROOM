import { useCallback, useRef, useState, type DragEvent } from 'react';
import type { RailEntry } from '../data';
import { useLoungeUi } from '../store';
import {
  addToFolder,
  dropZone,
  entryKey,
  makeFolder,
  moveEntry,
  moveGuildNextTo,
  newFolderId,
  FOLDER_COLORS,
} from './railLayout';

type Drag = { kind: 'guild' | 'folder'; id: string };
export type DropMark = { key: string; zone: 'before' | 'into' | 'after' } | null;

/** Where an item sits: at the top level, or inside an open folder. */
export type DropTarget =
  | { kind: 'top'; entry: RailEntry }
  | { kind: 'inFolder'; folderId: string; guildId: string; siblings: string[] };

/**
 * Drag and drop for the server rail (LNG-RAIL-007/008). Native HTML5 drag events; the keyboard
 * alternative lives in the item context menus. Dropping on the middle of a server makes a folder.
 */
export function useRailDnd(rail: RailEntry[]) {
  const setLayout = useLoungeUi((s) => s.setRailLayout);
  const drag = useRef<Drag | null>(null);
  const [mark, setMark] = useState<DropMark>(null);
  const [dragging, setDragging] = useState<string | null>(null);

  const markFor = (e: DragEvent, key: string, folderDrag: boolean): NonNullable<DropMark> => {
    const r = e.currentTarget.getBoundingClientRect();
    const y = e.clientY - r.top;
    const zone = dropZone(y, r.height);
    if (zone === 'into' && folderDrag) return { key, zone: y < r.height / 2 ? 'before' : 'after' };
    return { key, zone };
  };

  const itemProps = useCallback(
    (key: string, source: Drag, target: DropTarget) => ({
      draggable: true,
      onDragStart: (e: DragEvent) => {
        drag.current = source;
        setDragging(key);
        e.dataTransfer.effectAllowed = 'move';
        try {
          e.dataTransfer.setData('application/x-sideroom-rail', key);
        } catch {
          /* some engines refuse custom types; the ref carries the state */
        }
      },
      onDragEnd: () => {
        drag.current = null;
        setDragging(null);
        setMark(null);
      },
      onDragOver: (e: DragEvent) => {
        const d = drag.current;
        if (!d || d.id === key) return;
        // A folder cannot go inside a folder.
        if (d.kind === 'folder' && target.kind === 'inFolder') return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        setMark(markFor(e, key, d.kind === 'folder'));
      },
      onDragLeave: () => setMark((m) => (m?.key === key ? null : m)),
      onDrop: (e: DragEvent) => {
        const d = drag.current;
        drag.current = null;
        setDragging(null);
        setMark(null);
        if (!d || d.id === key) return;
        e.preventDefault();
        const { zone } = markFor(e, key, d.kind === 'folder');
        const next = applyDrop(rail, d, target, zone);
        if (next) setLayout(next);
      },
    }),
    [rail, setLayout],
  );

  return { itemProps, mark, dragging };
}

/** Pure: the layout after dropping `d` on `target` at `zone`, or null for a no-op. */
export function applyDrop(
  rail: RailEntry[],
  d: Drag,
  target: DropTarget,
  zone: 'before' | 'into' | 'after',
): RailEntry[] | null {
  if (target.kind === 'inFolder') {
    if (d.kind === 'folder') return null;
    const { folderId, guildId, siblings } = target;
    const i = siblings.indexOf(guildId);
    const beforeGuild = zone === 'before' ? guildId : siblings[i + 1];
    return addToFolder(rail, d.id, folderId, beforeGuild);
  }
  const t = target.entry;
  const tKey = entryKey(t);
  if (d.kind === 'folder')
    return moveEntry(rail, d.id, tKey, zone === 'before' ? 'before' : 'after');
  if (zone === 'into') {
    if (t.kind === 'folder') return addToFolder(rail, d.id, t.id);
    return makeFolder(rail, d.id, t.guildId, {
      id: newFolderId(),
      color: FOLDER_COLORS[0],
    });
  }
  return moveGuildNextTo(rail, d.id, tKey, zone);
}
