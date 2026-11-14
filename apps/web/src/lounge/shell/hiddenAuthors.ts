import { useMemo } from 'react';
import type { HiddenAuthors } from '../messageRows';
import { useIgnores } from './ignores';
import { useSocial } from './social';

const BLOCKED = 2;

/** The two sets from the lists themselves (no hook): blocked (relationship type 2) and ignored. */
export function hiddenAuthorSets(
  relationships: readonly { id: string; type: number }[],
  ignoredIds: readonly string[],
): HiddenAuthors {
  return {
    blocked: new Set(relationships.filter((r) => r.type === BLOCKED).map((r) => r.id)),
    ignored: new Set(ignoredIds),
  };
}

/** The sets as they are now, for code that runs outside a component (a notification). */
export function hiddenAuthorsNow(): HiddenAuthors {
  return hiddenAuthorSets(useSocial.getState().relationships, useIgnores.getState().ids);
}

/** People whose messages collapse in the message list: blocked (relationship type 2) and ignored. */
export function useHiddenAuthors(): HiddenAuthors {
  const relationships = useSocial((s) => s.relationships);
  const ignoredIds = useIgnores((s) => s.ids);
  return useMemo(() => hiddenAuthorSets(relationships, ignoredIds), [relationships, ignoredIds]);
}
