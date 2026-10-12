// Tiny subscription seam over gateway dispatches, for screens that keep their own REST-fetched data
// (server settings: bans, invites, ...) and want to refetch when someone else changes it.
type Listener = (event: string, data: unknown) => void;
const listeners = new Set<Listener>();

/** Subscribe to every dispatch after the reducer has applied it. Returns the unsubscribe function. */
export function onDispatch(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function emitDispatch(event: string, data: unknown): void {
  for (const l of [...listeners]) l(event, data);
}
