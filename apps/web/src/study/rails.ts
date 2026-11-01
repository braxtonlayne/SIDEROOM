// The Study's personal rails (STU-SHF-001): Continue Watching/Reading, Recently Added to Shelves You Can See and
// Pinned Folders. Continue and Pinned are kept in this browser only (localStorage, per account), never sent to the
// server; Recently Added is computed from listings the server already authorised for the viewer.
import { typeOf, type StudyType } from './fileTypes';

export interface Opened {
  mountId: string;
  key: string;
  title: string;
  /** Seconds into a video or audio file, or the 0-based page of a comic. */
  at: number;
  /** Length in seconds, when known; 0 when unknown. */
  of: number;
  updatedAt: number;
}

export interface Pin {
  mountId: string;
  prefix: string;
  label: string;
}

const MAX_OPENED = 30;
const MAX_PINS = 24;
/** Within this fraction of the end, a file counts as finished. */
const DONE = 0.95;

const openedKey = (uid: string) => `sr.study.opened.${uid}`;
const pinsKey = (uid: string) => `sr.study.pins.${uid}`;

function read<T>(k: string): T[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(k) ?? '[]');
    return Array.isArray(v) ? (v as T[]) : [];
  } catch {
    return [];
  }
}
function write(k: string, v: unknown) {
  try {
    localStorage.setItem(k, JSON.stringify(v));
  } catch {
    // Private window or full storage: the rails simply stay empty.
  }
}

export const loadOpened = (uid: string) => read<Opened>(openedKey(uid));
export const loadPins = (uid: string) => read<Pin>(pinsKey(uid));

/** Pure: record progress for a file, newest first, capped. */
export function recordOpened(list: readonly Opened[], o: Opened): Opened[] {
  return [o, ...list.filter((x) => !(x.mountId === o.mountId && x.key === o.key))].slice(
    0,
    MAX_OPENED,
  );
}
export const writeOpened = (uid: string, list: Opened[]) => write(openedKey(uid), list);

/** Pure: toggle a pin. */
export function togglePin(list: readonly Pin[], p: Pin): Pin[] {
  const has = list.some((x) => x.mountId === p.mountId && x.prefix === p.prefix);
  return has
    ? list.filter((x) => !(x.mountId === p.mountId && x.prefix === p.prefix))
    : [...list, p].slice(-MAX_PINS);
}
export function savePins(uid: string, pins: Pin[]) {
  write(pinsKey(uid), pins);
}

export const isFinished = (o: Opened) => o.of > 0 && o.at / o.of >= DONE;

/** The two continue rails: unfinished video/audio, and unfinished reading. */
export function continueRails(list: readonly Opened[]): { watching: Opened[]; reading: Opened[] } {
  const live = list.filter((o) => !isFinished(o));
  const kind = (o: Opened): StudyType => typeOf(o.key);
  return {
    watching: live.filter((o) => ['video', 'audio'].includes(kind(o))),
    reading: live.filter((o) => kind(o) === 'reading'),
  };
}

export interface Recent {
  mountId: string;
  key: string;
  size: number;
  lastModified: string;
}

/** Newest first across shelves; entries without a date are left out. */
export function recentlyAdded(
  lists: readonly {
    mountId: string;
    objects: { key: string; size: number; last_modified: string | null }[];
  }[],
  limit = 12,
): Recent[] {
  const all: Recent[] = [];
  for (const l of lists) {
    for (const o of l.objects) {
      if (o.last_modified && typeOf(o.key) !== 'other') {
        all.push({ mountId: l.mountId, key: o.key, size: o.size, lastModified: o.last_modified });
      }
    }
  }
  all.sort((a, b) => Date.parse(b.lastModified) - Date.parse(a.lastModified));
  return all.slice(0, limit);
}
