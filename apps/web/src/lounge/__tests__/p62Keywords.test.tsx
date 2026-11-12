// SEC-AUD-148 (wave 62): a failed load of the notification keywords is not remembered; the next use asks again, at
// most once every 3 s, and without a toast.
import { renderHook, waitFor } from '@testing-library/react';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { resetKeywordLoad, useNotificationKeywords } from '../keywords';

beforeEach(() => {
  resetKeywordLoad();
  useStore.setState({ notificationKeywords: null });
});
afterEach(() => vi.restoreAllMocks());

describe('notification keywords load', () => {
  it('a_failed_load_is_asked_again_on_the_next_use_but_not_within_3_seconds', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/users/@me/notification-keywords', () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json({ message: 'boom' }, { status: 500 })
          : HttpResponse.json({ keywords: ['deploy'] });
      }),
    );
    const first = renderHook(() => useNotificationKeywords());
    await waitFor(() => expect(calls).toBe(1));
    await new Promise((r) => setTimeout(r, 20));
    expect(first.result.current).toEqual([]);
    first.unmount();

    // A new use straight away does not hammer the server.
    const quick = renderHook(() => useNotificationKeywords());
    await new Promise((r) => setTimeout(r, 20));
    expect(calls).toBe(1);
    quick.unmount();

    // A few seconds later the next use asks again and gets the list.
    const now = Date.now();
    vi.spyOn(Date, 'now').mockReturnValue(now + 10_000);
    const later = renderHook(() => useNotificationKeywords());
    await waitFor(() => expect(later.result.current).toEqual(['deploy']));
    expect(calls).toBe(2);
  });
});
