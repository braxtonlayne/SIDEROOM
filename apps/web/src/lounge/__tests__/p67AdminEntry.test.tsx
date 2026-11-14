// SEC-AUD-205 (wave 67, D-673): the status menu's "Platform Admin" entry reads the account's own READY, and asks
// the server nothing. (The first version of this file, wave 61, tested a retried `/admin/me` check that is gone.)
import { ToastProvider } from '@sideroom/ui';
import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { emitDispatch } from '../../store/events';
import { useStore } from '../../store';
import { emptyData, reduceDispatch } from '../../store/reducer';
import { ComponentRows } from '../msg/Components';
import { setPlatformStaff, useIsPlatformStaff } from '../shell/adminEntry';

beforeEach(() => {
  useStore.setState({ platformStaff: false });
});

const ready = (user: Record<string, unknown>) =>
  reduceDispatch(emptyData(), 'READY', { user, guilds: [] });

describe('the READY flag', () => {
  it('a_staff_ready_shows_platform_admin_at_once', () => {
    useStore.setState(ready({ id: '1', username: 'a', sr_platform_staff: true }));
    const { result } = renderHook(() => useIsPlatformStaff());
    expect(result.current).toBe(true);
  });

  it('a_member_ready_has_no_flag_and_a_new_ready_clears_a_stale_one', () => {
    useStore.setState({ platformStaff: true });
    // A resync or a new session whose READY has no flag (the appointment ended) reads it afresh.
    useStore.setState(ready({ id: '1', username: 'a' }));
    expect(useStore.getState().platformStaff).toBe(false);
  });

  it('a_member_using_the_menu_hook_makes_no_request_to_admin_me', async () => {
    const asked: string[] = [];
    server.use(
      http.get('*/api/v1/admin/me', () => {
        asked.push('/admin/me');
        return HttpResponse.json({ message: 'Missing permissions' }, { status: 403 });
      }),
    );
    const { result, unmount } = renderHook(() => useIsPlatformStaff());
    await new Promise((r) => setTimeout(r, 30));
    expect(result.current).toBe(false);
    unmount();
    renderHook(() => useIsPlatformStaff());
    await new Promise((r) => setTimeout(r, 30));
    expect(asked).toEqual([]);
  });

  it('a_revocation_hides_it', () => {
    setPlatformStaff(true);
    const { result } = renderHook(() => useIsPlatformStaff());
    expect(result.current).toBe(true);
    act(() => emitDispatch('SR_STAFF_ALERT', { kind: 'disk_low', detail: 'x' }));
    expect(result.current).toBe(true);
    act(() => emitDispatch('SR_STAFF_ALERT', { kind: 'staff_revoked', revoked: true }));
    expect(result.current).toBe(false);
  });
});

describe('accepting an appointment offer', () => {
  const rows = [
    {
      type: 1,
      components: [
        { type: 2, style: 3, label: 'Accept', custom_id: 'sr:offer-accept:77' },
        { type: 2, style: 4, label: 'Decline', custom_id: 'sr:offer-decline:77' },
      ],
    },
  ];
  const view = () =>
    render(
      <ToastProvider>
        <ComponentRows rows={rows} context={{ channelId: '500', messageId: '900' }} />
      </ToastProvider>,
    );
  beforeEach(() => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.post('*/api/v1/staff/offers/77/respond', () => new HttpResponse(null, { status: 204 })),
    );
  });

  it('shows_platform_admin_without_a_new_session', async () => {
    view();
    expect(useStore.getState().platformStaff).toBe(false);
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(useStore.getState().platformStaff).toBe(true));
  });

  it('declining_does_not', async () => {
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled());
    expect(useStore.getState().platformStaff).toBe(false);
  });
});
