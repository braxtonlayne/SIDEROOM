// The owner's own words on a file (STU-SHF-001): the title, cover and note columns of the shelf catalog
// (`GET /shelves/{shelf_id}/objects`, 41-C) and `PATCH /shelves/{shelf_id}/objects/{object_hash}` to change them.
import { apiFetch } from '../api/client';
import { baseName, titleOf } from './fileTypes';
import { objectHash, objectPath } from './objectUrl';

export interface FileNote {
  key: string;
  title?: string;
  note?: string;
  cover_key?: string;
}

export interface FileNoteList {
  notes: FileNote[];
  can_edit: boolean;
}

/** One catalog page as `GET /shelves/{shelf_id}/objects` returns it (the fields the Study reads). */
interface CatalogPage {
  objects: { path: string; title: string; cover: string; note: string }[];
  next_cursor: string | null;
  can_edit: boolean;
}

/** Most catalog pages (200 objects each) read for the notes of one shelf. */
const MAX_PAGES = 25;

export async function listNotes(shelfId: string): Promise<FileNoteList> {
  const notes: FileNote[] = [];
  let cursor: string | null = null;
  let canEdit = false;
  for (let i = 0; i < MAX_PAGES; i++) {
    const q: string = cursor ? `?cursor=${encodeURIComponent(cursor)}` : '';
    const page: CatalogPage = await apiFetch<CatalogPage>(
      `/shelves/${encodeURIComponent(shelfId)}/objects${q}`,
      { retries: 0 },
    );
    canEdit = page.can_edit;
    for (const o of page.objects) {
      if (!o.title && !o.note && !o.cover) continue;
      const n: FileNote = { key: o.path };
      if (o.title) n.title = o.title;
      if (o.note) n.note = o.note;
      if (o.cover) n.cover_key = o.cover;
      notes.push(n);
    }
    cursor = page.next_cursor;
    if (!cursor) break;
  }
  return { notes, can_edit: canEdit };
}

/** Set the note of a file; an empty title, note and cover clears it. */
export async function putNote(shelfId: string, n: FileNote): Promise<void> {
  await apiFetch<unknown>(objectPath(shelfId, await objectHash(n.key)), {
    method: 'PATCH',
    json: { title: n.title ?? '', note: n.note ?? '', cover: n.cover_key ?? '' },
    retries: 0,
  });
}

export type NoteMap = Map<string, FileNote>;

export const noteMap = (l: FileNoteList | undefined): NoteMap =>
  new Map((l?.notes ?? []).map((n) => [n.key, n]));

/** What a card shows for a file: the owner's title if they set one, else the file name as a title. */
export function displayTitle(key: string, notes: NoteMap): string {
  return notes.get(key)?.title?.trim() || titleOf(key);
}

/** The edit form's values, merged into one note (blank fields drop out; `null` when nothing is left). */
export function noteFromForm(
  key: string,
  f: { title: string; note: string; cover: string },
): FileNote | null {
  const title = f.title.trim();
  const note = f.note.trim();
  const cover = f.cover.trim();
  if (!title && !note && !cover) return null;
  const out: FileNote = { key };
  if (title) out.title = title;
  if (note) out.note = note;
  if (cover) out.cover_key = cover;
  return out;
}

/** Apply a saved edit to the local map (so the card changes before the next fetch). */
export function withNote(notes: NoteMap, key: string, n: FileNote | null): NoteMap {
  const next = new Map(notes);
  if (n) next.set(key, n);
  else next.delete(key);
  return next;
}

/** The images in a folder listing a cover may be chosen from. */
export function coverChoices(keys: readonly string[]): string[] {
  return keys.filter((k) => /\.(jpe?g|png|webp|gif)$/i.test(baseName(k)));
}
