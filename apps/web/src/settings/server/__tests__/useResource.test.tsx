import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useResource } from '../hooks';

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

describe('useResource', () => {
  it('drops a load that a local change already superseded (the bridge switch race)', async () => {
    const loads = [deferred<string>(), deferred<string>()];
    let n = 0;
    const { result } = renderHook(() => useResource('k', () => loads[n++]!.promise));
    await act(async () => loads[0]!.resolve('off'));
    await waitFor(() => expect(result.current.data).toBe('off'));

    // A reload starts, the owner flips the switch, then the old read answers.
    let reloading!: Promise<void>;
    act(() => {
      reloading = result.current.reload();
    });
    act(() => result.current.set('on'));
    await act(async () => {
      loads[1]!.resolve('off');
      await reloading;
    });
    expect(result.current.data).toBe('on');
  });

  it('still lands the first load when a change comes before it', async () => {
    const load = deferred<string[]>();
    const { result } = renderHook(() => useResource('k', () => load.promise));
    act(() => result.current.set((cur) => cur ?? []));
    await act(async () => load.resolve(['a']));
    await waitFor(() => expect(result.current.data).toEqual(['a']));
  });
});
