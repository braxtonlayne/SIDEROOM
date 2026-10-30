/**
 * UTL-SRC-001 in the drawer (wave-32 integration): the three-stage search of `sr-tools-wasm` (prefix, typo with
 * "Did you mean", task phrases from `tools/corpus.json`) ranks the tools. The wasm module loads lazily the first
 * time the Tools tab is shown; until it is ready, or where it cannot load, the small pure filter of
 * `toolSearch.ts` answers, so the list never waits on the network.
 */
import type { ToolSearchIndex } from '@sideroom/wasm';
import { searchTools, type ToolText } from './toolSearch';

let index: Promise<ToolSearchIndex | null> | null = null;

/** The tool search index, created once. Resolves to `null` when the wasm module cannot load. */
export function loadToolIndex(): Promise<ToolSearchIndex | null> {
  index ??= import('@sideroom/wasm')
    .then((m) => m.loadTools())
    .then((tools) => tools.search.create())
    .catch(() => {
      index = null;
      return null;
    });
  return index;
}

/** Tests only: forget the loaded index. */
export function resetToolIndexForTests(): void {
  index = null;
}

export interface RankedTools {
  rows: ToolText[];
  /** The tool the query was probably meant to name, when nothing matched by prefix. */
  didYouMean: ToolText | null;
}

/**
 * Tools for `query`, best first. With an index: the wasm hits in its order, then any further matches of the
 * pure filter (a word of a translated description, which the English corpus does not hold). Without one: the
 * pure filter alone.
 */
export function rankTools(
  list: readonly ToolText[],
  query: string,
  idx: Pick<ToolSearchIndex, 'search'> | null,
): RankedTools {
  const fallback = searchTools(list, query);
  if (!idx || query.trim() === '') return { rows: fallback, didYouMean: null };
  let res;
  try {
    res = idx.search(`>${query.trim()}`, 50);
  } catch {
    return { rows: fallback, didYouMean: null };
  }
  const byId = new Map(list.map((r) => [r.entry.id, r]));
  const seen = new Set<string>();
  const rows: ToolText[] = [];
  for (const hit of res.hits) {
    const row = hit.kind === 'tool' ? byId.get(hit.id) : undefined;
    if (row && !seen.has(row.entry.id)) {
      seen.add(row.entry.id);
      rows.push(row);
    }
  }
  for (const row of fallback) {
    if (!seen.has(row.entry.id)) {
      seen.add(row.entry.id);
      rows.push(row);
    }
  }
  let didYouMean: ToolText | null = null;
  if (res.didYouMean) {
    const hit = res.hits.find((h) => h.kind === 'tool' && h.label === res.didYouMean);
    didYouMean = (hit && byId.get(hit.id)) ?? null;
  }
  return { rows, didYouMean };
}
