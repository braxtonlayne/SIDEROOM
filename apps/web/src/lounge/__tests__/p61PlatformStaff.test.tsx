// SEC-AUD-137a (wave 61): a staff check that failed is not remembered as "not staff".
import { renderHook, waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { usePlatformStaff, useIsPlatformStaff } from '../shell/platformStaff';

beforeEach(() => {
  usePlatformStaff.setState({ staff: false, loaded: false });
});
afterEach(() => vi.restoreAllMocks());

describe('platform staff check', () => {
  it('a_failed_check_is_asked_again_on_the_next_use', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/admin/me', () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json({ message: 'boom' }, { status: 500 })
          : HttpResponse.json({ is_owner: false, platform_permissions: '1', user_id: '1' });
      }),
    );
    const first = renderHook(() => useIsPlatformStaff());
    await waitFor(() => expect(calls).toBe(1));
    await new Promise((r) => setTimeout(r, 20));
    expect(first.result.current).toBe(false);
    expect(usePlatformStaff.getState().loaded).toBe(false);
    first.unmount();

    // The next use (a few seconds later) asks again and shows the staff entry.
    const now = Date.now();
    vi.spyOn(Date, 'now').mockReturnValue(now + 10_000);
    const second = renderHook(() => useIsPlatformStaff());
    await waitFor(() => expect(second.result.current).toBe(true));
    expect(calls).toBe(2);
  });

  it('a_403_is_the_answer_not_staff_and_is_kept', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/admin/me', () => {
        calls += 1;
        return HttpResponse.json({ message: 'Missing permissions' }, { status: 403 });
      }),
    );
    const first = renderHook(() => useIsPlatformStaff());
    await waitFor(() => expect(usePlatformStaff.getState().loaded).toBe(true));
    first.unmount();
    renderHook(() => useIsPlatformStaff());
    await new Promise((r) => setTimeout(r, 20));
    expect(calls).toBe(1);
  });
});
