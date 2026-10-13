import { useMemo } from 'react';
import type { HiddenAuthors } from '../messageRows';
import { useIgnores } from './ignores';
import { useSocial } from './social';

const BLOCKED = 2;
/** People whose messages collapse in the message list: blocked (relationship type 2) and ignored. */
export function useHiddenAuthors(): HiddenAuthors {
  const relationships = useSocial((s) => s.relationships);
  const ignoredIds = useIgnores((s) => s.ids);
  return useMemo(
    () => ({
      blocked: new Set(relationships.filter((r) => r.type === BLOCKED).map((r) => r.id)),
      ignored: new Set(ignoredIds),
    }),
    [relationships, ignoredIds],
  );
}
