// The owner's own words on a file (STU-SHF-001): `GET|PUT /storage/mounts/{mount}/notes`.
import { apiFetch } from '../api/client';
import { baseName, titleOf } from './fileTypes';

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

const path = (mountId: string) => `/storage/mounts/${encodeURIComponent(mountId)}/notes`;

export const listNotes = (mountId: string) => apiFetch<FileNoteList>(path(mountId), { retries: 0 });

/** Set the note of a file; an empty title, note and cover removes it. */
export const putNote = (mountId: string, n: FileNote) =>
  apiFetch<void>(path(mountId), {
    method: 'PUT',
    json: { key: n.key, title: n.title ?? '', note: n.note ?? '', cover_key: n.cover_key ?? '' },
    retries: 0,
  });

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
