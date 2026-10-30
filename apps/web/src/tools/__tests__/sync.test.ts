import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EMPTY_LAYOUT, type Layout } from '../gadgets/layout';
import { useToolsStore } from '../gadgets/store';

const mocks = vi.hoisted(() => ({
  state: { userSettings: null as null | { settings: Record<string, unknown> } },
  listeners: new Set<(s: unknown) => void>(),
  patch: vi.fn(),
  fetch: vi.fn(),
}));

vi.mock('../../store', () => ({
  rest: { patchUserSettings: mocks.patch, fetchUserSettings: mocks.fetch },
  useStore: Object.assign(() => undefined, {
    getState: () => ({
      userSettings: mocks.state.userSettings,
      applyDispatch: (_op: string, blob: { settings: Record<string, unknown> }) => {
        mocks.state.userSettings = blob;
        for (const l of mocks.listeners) l(mocks.state);
      },
    }),
    subscribe: (fn: (s: unknown) => void) => {
      mocks.listeners.add(fn);
      return () => mocks.listeners.delete(fn);
    },
  }),
}));

const { HINT_KEY, LAYOUT_KEY, blobHasLayout, layoutHint, startLayoutSync } =
  await import('../gadgets/sync');

const L = (tools: string[], pins: string[] = []): Layout => ({
  gadgets: tools.map((tool, i) => ({ tool, x: 10, y: 10, w: 300, h: 200, z: i + 1 })),
  pins,
});

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  mocks.state.userSettings = null;
  mocks.listeners.clear();
  mocks.patch.mockReset().mockResolvedValue({ settings: {} });
  mocks.fetch.mockReset();
  useToolsStore.setState({ layout: EMPTY_LAYOUT, limitHit: false });
});
afterEach(() => vi.useRealTimers());

describe('the layout sync (UTL-SHL-001)', () => {
  it('adopts the account copy over the browser copy', () => {
    localStorage.setItem('sr.tools.layout.v1:7', JSON.stringify(L(['a'])));
    mocks.state.userSettings = { settings: { [LAYOUT_KEY]: L(['b', 'c'], ['b']) } };
    const stop = startLayoutSync('7');
    expect(useToolsStore.getState().layout.gadgets.map((g) => g.tool)).toEqual(['b', 'c']);
    expect(useToolsStore.getState().layout.pins).toEqual(['b']);
    expect(mocks.patch).not.toHaveBeenCalled();
    stop();
  });

  it('pushes the browser copy up once when the account has none', async () => {
    localStorage.setItem('sr.tools.layout.v1:7', JSON.stringify(L(['a'])));
    mocks.state.userSettings = { settings: {} };
    const stop = startLayoutSync('7');
    await vi.runAllTimersAsync();
    expect(mocks.patch).toHaveBeenCalledTimes(1);
    expect(mocks.patch.mock.calls[0]?.[0]).toEqual({ [LAYOUT_KEY]: L(['a']) });
    stop();
  });

  it('saves a change locally at once and to the account after the debounce', async () => {
    mocks.state.userSettings = { settings: { [LAYOUT_KEY]: EMPTY_LAYOUT } };
    const stop = startLayoutSync('7', 400);
    useToolsStore.getState().pin('guide');
    expect(JSON.parse(localStorage.getItem('sr.tools.layout.v1:7')!)).toEqual(L([], ['guide']));
    expect(layoutHint()).toBe(true);
    expect(mocks.patch).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(450);
    expect(mocks.patch).toHaveBeenCalledWith({ [LAYOUT_KEY]: L([], ['guide']) });
    // Emptying the layout clears the hint.
    useToolsStore.getState().unpin('guide');
    expect(localStorage.getItem(HINT_KEY)).toBeNull();
    stop();
  });

  it('keeps accounts apart in the browser copy', () => {
    localStorage.setItem('sr.tools.layout.v1:7', JSON.stringify(L(['a'])));
    mocks.state.userSettings = { settings: {} };
    const stop = startLayoutSync('8');
    expect(useToolsStore.getState().layout).toEqual(EMPTY_LAYOUT);
    stop();
  });

  it('survives a rejected write and corrupt storage', async () => {
    localStorage.setItem('sr.tools.layout.v1:7', '{not json');
    mocks.patch.mockRejectedValue(new Error('offline'));
    mocks.state.userSettings = { settings: { [LAYOUT_KEY]: EMPTY_LAYOUT } };
    const stop = startLayoutSync('7', 10);
    useToolsStore.getState().pin('guide');
    await vi.advanceTimersByTimeAsync(50);
    expect(useToolsStore.getState().layout.pins).toEqual(['guide']);
    stop();
  });

  it('tells the gate whether the blob holds a layout', () => {
    expect(blobHasLayout(undefined)).toBe(false);
    expect(blobHasLayout({ [LAYOUT_KEY]: EMPTY_LAYOUT })).toBe(false);
    expect(blobHasLayout({ [LAYOUT_KEY]: L([], ['a']) })).toBe(true);
    expect(blobHasLayout({ [LAYOUT_KEY]: 'junk' })).toBe(false);
  });
});
