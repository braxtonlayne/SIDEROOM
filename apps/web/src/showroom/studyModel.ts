// UX-SHOW-001: the pure part of the sample Study (source filter and kind filter over the sample shelves).
import type { FixtureItem, FixtureKind, FixtureShelf } from './fixtures';

export type SourceFilter = 'all' | FixtureShelf['source'];
export type KindFilter = 'all' | FixtureKind;

export interface Row {
  shelf: FixtureShelf;
  item: FixtureItem;
}

/** The shelves a source filter keeps. */
export function shelvesFor(shelves: readonly FixtureShelf[], source: SourceFilter): FixtureShelf[] {
  return shelves.filter((s) => source === 'all' || s.source === source);
}

/**
 * The rows to show: the chosen shelf alone, or every shelf the source filter keeps; then the kind filter; newest first.
 */
export function rowsFor(
  shelves: readonly FixtureShelf[],
  source: SourceFilter,
  shelfId: string | null,
  kind: KindFilter,
): Row[] {
  return shelvesFor(shelves, source)
    .filter((s) => shelfId === null || s.id === shelfId)
    .flatMap((shelf) => shelf.items.map((item) => ({ shelf, item })))
    .filter((r) => kind === 'all' || r.item.kind === kind)
    .sort((a, b) => b.item.added.localeCompare(a.item.added) || a.item.id.localeCompare(b.item.id));
}
