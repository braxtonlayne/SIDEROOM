// The one place the whiteboard touches the Embedded App SDK's shared-state API (ACT-CH-001, act-channel):
// `sdk.state` is a flat JSON document with a version, last-writer-wins per key, where only the instance's host
// may write `host:*` keys; `sdk.on('state', e)` reports `e.changes` (`value` is `undefined` after a delete);
// `sdk.isHost` says whether this participant launched the instance. One message carries at most 16 KiB, which
// is why the board keeps every entry and every snapshot chunk small.

import type { StateAdapter } from './sync';

/** The slice of the SDK the whiteboard needs (a `SideRoomSDK` satisfies it). */
export interface SdkLike {
  readonly isHost: boolean;
  state: {
    all(): Record<string, unknown>;
    set(patch: Record<string, unknown>): void;
    delete(...keys: string[]): void;
  };
  on(
    event: 'state',
    cb: (e: { changes: Array<{ key: string; value: unknown }> }) => void,
  ): () => void;
}

/**
 * Wrap `sdk.state` as the board's adapter. A write the SDK refuses (a message over 16 KiB, a `host:*` key from a
 * guest) is reported to `onError` and dropped; the board never depends on one write alone.
 */
export function adapterFromSdk(sdk: SdkLike, onError: (e: unknown) => void = () => {}): StateAdapter {
  return {
    snapshot: () => sdk.state.all(),
    set: (key, value) => {
      try {
        if (value === null || value === undefined) sdk.state.delete(key);
        else sdk.state.set({ [key]: value });
      } catch (e) {
        onError(e);
      }
    },
    onChange: (cb) =>
      sdk.on('state', (e) => {
        const changed: Record<string, unknown> = {};
        for (const c of e.changes) changed[c.key] = c.value === undefined ? null : c.value;
        cb(changed);
      }),
  };
}

const STORE_KEY = 'sideroom-whiteboard-member';

/**
 * Who this participant is on the board. The SDK does not yet tell an Activity its member id (open item for
 * act-channel: `sdk.self`); until it does, use `sdk.self` when the SDK has it and otherwise an id made once per
 * browser tab, kept in sessionStorage so a reload keeps its counter and its undo history.
 */
export function memberId(sdk: unknown): string {
  const self = (sdk as { self?: unknown }).self;
  if (typeof self === 'string' && /^[A-Za-z0-9_.-]{1,40}$/.test(self)) return self;
  try {
    const saved = sessionStorage.getItem(STORE_KEY);
    if (saved) return saved;
  } catch {
    // storage may be blocked
  }
  const id = `m${Math.random().toString(36).slice(2, 10)}`;
  try {
    sessionStorage.setItem(STORE_KEY, id);
  } catch {
    // the id then lasts for this page load
  }
  return id;
}
