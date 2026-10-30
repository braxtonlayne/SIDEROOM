import { readStored, writeStored } from '../calc/hooks';
import { isValidZone, type PersonZone } from './zones';

/**
 * The people on the grid. Other members' time zones are not exposed by the server, so a "person" is a name the
 * member types and a zone they pick; the list lives in this browser only (a per-viewer convenience).
 */
const KEY = 'sr.tools.timezone.people';
export const MAX_PEOPLE = 11;

const isPeople = (v: unknown): v is PersonZone[] =>
  Array.isArray(v) &&
  v.every((p) => {
    const q = p as PersonZone;
    return typeof q?.id === 'string' && typeof q.name === 'string' && typeof q.zone === 'string';
  });

export function loadPeople(): PersonZone[] {
  return readStored(KEY, [], isPeople)
    .filter((p) => isValidZone(p.zone))
    .slice(0, MAX_PEOPLE);
}

export function savePeople(list: readonly PersonZone[]): void {
  writeStored(KEY, list.slice(0, MAX_PEOPLE));
}

/** Adds a person; returns the new list, or null when the zone is unknown, the name is empty or the list is full. */
export function addPerson(
  list: readonly PersonZone[],
  name: string,
  zone: string,
  id: string,
): PersonZone[] | null {
  const n = name.trim().slice(0, 40);
  const z = zone.trim();
  if (n === '' || !isValidZone(z) || list.length >= MAX_PEOPLE) return null;
  return [...list, { id, name: n, zone: z }];
}
