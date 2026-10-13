import { useEffect } from 'react';
import { useStore } from '../store';
import { memberKey } from '../store/reducer';

/** ISO end of the viewer's timeout in a guild, or null when they are not timed out. */
export function useMyTimeout(guildId: string | undefined): string | null {
  const until = useStore((s) =>
    guildId && s.currentUserId
      ? (s.members[memberKey(guildId, s.currentUserId)]?.communication_disabled_until ?? null)
      : null,
  );
  // The server sends no event when a timeout ends. Touch the member map then, so permissions that were
  // computed with the timeout (the composer lock) are computed again.
  useEffect(() => {
    if (!until) return;
    const ms = Date.parse(until) - Date.now();
    if (!(ms > 0) || ms > 2 ** 31 - 1) return;
    const t = setTimeout(() => useStore.setState((s) => ({ members: { ...s.members } })), ms + 100);
    return () => clearTimeout(t);
  }, [until]);
  return until && Date.parse(until) > Date.now() ? until : null;
}
