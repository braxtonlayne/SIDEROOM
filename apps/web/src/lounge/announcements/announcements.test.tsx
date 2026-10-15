// Platform announcements (ADM-PLT-009 client side). The Lounge notices bar renders them (p11ShellNotices).
import { act, renderHook, waitFor } from '@testing-library/react';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { canDismiss, useActiveAnnouncements } from './index';

const note = (over: Record<string, unknown> = {}) => ({
  id: '1',
  title: 'Maintenance tonight',
  body: 'Short downtime at 22:00 UTC.',
  severity: 'warning',
  starts_at_ms: 1,
  ...over,
});

describe('useActiveAnnouncements', () => {
  beforeEach(() => localStorage.clear());

  it('loads the active notices, and an empty list when there are none', async () => {
    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([note()])));
    const first = renderHook(() => useActiveAnnouncements());
    await waitFor(() => expect(first.result.current.announcements).toHaveLength(1));
    expect(first.result.current.announcements[0]!.title).toBe('Maintenance tonight');
    first.unmount();

    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([])));
    const second = renderHook(() => useActiveAnnouncements());
    await new Promise((r) => setTimeout(r, 30));
    expect(second.result.current.announcements).toEqual([]);
  });

  it('remembers a dismissed notice but never lets a critical one go', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () =>
        HttpResponse.json([
          note(),
          note({ id: '2', title: 'Outage', severity: 'critical', body: 'Down.' }),
        ]),
      ),
    );
    const view = renderHook(() => useActiveAnnouncements());
    await waitFor(() => expect(view.result.current.announcements).toHaveLength(2));
    expect(view.result.current.announcements.filter(canDismiss).map((a) => a.id)).toEqual(['1']);
    act(() => {
      view.result.current.dismiss('1');
      view.result.current.dismiss('2');
    });
    expect(view.result.current.announcements.map((a) => a.id)).toEqual(['2']);

    view.unmount();
    const again = renderHook(() => useActiveAnnouncements());
    await waitFor(() => expect(again.result.current.announcements).toHaveLength(1));
    expect(again.result.current.announcements[0]!.title).toBe('Outage');
  });

  it('re-reads the list when the gateway sends SR_ANNOUNCEMENT_UPDATE (G-69)', async () => {
    let title = 'Maintenance tonight';
    server.use(
      http.get('*/api/v1/announcements/active', () => HttpResponse.json([note({ title })])),
    );
    const view = renderHook(() => useActiveAnnouncements());
    await waitFor(() => expect(view.result.current.announcements[0]?.title).toBe(title));
    title = 'Maintenance moved to Friday';
    act(() => {
      emitDispatch('MESSAGE_CREATE', {});
      emitDispatch('SR_ANNOUNCEMENT_UPDATE', { change: 'edit' });
    });
    await waitFor(() =>
      expect(view.result.current.announcements[0]?.title).toBe('Maintenance moved to Friday'),
    );
    view.unmount();
  });

  it('keeps quiet when the request fails', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () => new HttpResponse(null, { status: 500 })),
    );
    const view = renderHook(() => useActiveAnnouncements());
    await new Promise((r) => setTimeout(r, 30));
    expect(view.result.current.announcements).toEqual([]);
  });
});
