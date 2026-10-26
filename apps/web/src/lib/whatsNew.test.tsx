// RDY-PWA-001: the one-time "what's new" badge across tabs and across an update.
import { act, renderHook } from '@testing-library/react';
import { APP_VERSION, hasUnseenNews, markNewsSeen, useNewsBadge } from './whatsNew';

const KEY = 'sr.whatsNew.seen';

beforeEach(() => localStorage.clear());

describe('hasUnseenNews', () => {
  it('records the version of a first visit without a badge, then shows one for a newer version', () => {
    expect(hasUnseenNews('Phase 25')).toBe(false);
    expect(localStorage.getItem(KEY)).toBe('Phase 25');
    expect(hasUnseenNews('Phase 25')).toBe(false);
    expect(hasUnseenNews('Phase 26')).toBe(true);
    markNewsSeen('Phase 26');
    expect(hasUnseenNews('Phase 26')).toBe(false);
  });

  it('shows no badge when storage is blocked', () => {
    const get = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    const set = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(hasUnseenNews('Phase 26')).toBe(false);
    expect(() => markNewsSeen('Phase 26')).not.toThrow();
    get.mockRestore();
    set.mockRestore();
  });
});

describe('useNewsBadge', () => {
  it('follows another tab that read the news, without a reload', () => {
    // A browser that last saw another version than this build has news.
    localStorage.setItem(KEY, 'Phase 1');
    const { result } = renderHook(() => useNewsBadge());
    expect(result.current).toBe(true);
    // Another tab marks the news seen: the browser sends this tab a storage event.
    act(() => {
      localStorage.setItem(KEY, APP_VERSION);
      window.dispatchEvent(new StorageEvent('storage', { key: KEY, newValue: APP_VERSION }));
    });
    expect(result.current).toBe(false);
  });

  it('ignores other keys and goes away with the hook', () => {
    localStorage.setItem(KEY, 'Phase 1');
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    const { result, unmount } = renderHook(() => useNewsBadge());
    act(() => {
      localStorage.setItem(KEY, APP_VERSION);
      window.dispatchEvent(new StorageEvent('storage', { key: 'something.else' }));
    });
    expect(result.current).toBe(true);
    expect(add.mock.calls.filter(([t]) => t === 'storage')).toHaveLength(1);
    unmount();
    expect(remove.mock.calls.filter(([t]) => t === 'storage')).toHaveLength(1);
    add.mockRestore();
    remove.mockRestore();
  });
});
