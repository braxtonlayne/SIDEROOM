// The one place the whiteboard touches the Embedded App SDK's shared-state API (ACT-CH-001, written by
// act-channel). Until that API is merged this is coded against the roadmap text: `sdk.state` is a shared JSON
// document with a version and last-writer-wins per key, host-authoritative for `host:*` keys, and
// `sdk.on('state', ...)` reports changes. If the published names differ, only this file changes.

import type { StateAdapter } from './sync';

/** The slice of `sdk.state` the board needs. */
export interface SdkStateLike {
  /** The whole document as a flat key to value map. */
  all(): Record<string, unknown>;
  /** Write one key (`null` deletes). Host-only for `host:*`. */
  set(key: string, value: unknown): void | Promise<void>;
}

/** The slice of the SDK the whiteboard needs. */
export interface SdkLike {
  state: SdkStateLike;
  /** `on('state', cb)` where cb receives the changed keys. Returns an unsubscribe function. */
  on(event: 'state', cb: (changed: Record<string, unknown>) => void): () => void;
}

export function adapterFromSdk(sdk: SdkLike): StateAdapter {
  return {
    snapshot: () => sdk.state.all(),
    set: (key, value) => {
      // Writes are fire-and-forget; the relay keeps order per sender and a failed write is retried by the
      // next edit being appended (nothing here depends on a single write).
      void Promise.resolve(sdk.state.set(key, value)).catch(() => undefined);
    },
    onChange: (cb) => sdk.on('state', cb),
  };
}

/** Who the local member is, and whether they launched the instance (the host). */
export interface Identity {
  me: string;
  isHost: boolean;
}

/** Read the identity from the ready config. act-channel adds `member_id` and `host` to it. */
export function identityFromConfig(config: unknown): Identity | null {
  if (typeof config !== 'object' || config === null) return null;
  const c = config as Record<string, unknown>;
  const me = c.member_id ?? c.user_id;
  if (typeof me !== 'string' || me === '') return null;
  const host = c.host ?? c.is_host;
  return { me, isHost: host === true || c.host_id === me };
}
