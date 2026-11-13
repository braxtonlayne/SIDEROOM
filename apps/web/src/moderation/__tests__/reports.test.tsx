import { act, render, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import {
  closeReport,
  entryList,
  ReportButton,
  setReportsEnabled,
  useReportEntry,
  useReports,
} from '../reports';

afterEach(() => {
  closeReport();
  setReportsEnabled(false);
});

describe('moderation.reports flag (XC-MOD-002)', () => {
  it('is off by default: no menu entry and no button', () => {
    const { result } = renderHook(() => useReportEntry());
    expect(result.current({ kind: 'message', id: '1' })).toBeNull();
    expect(entryList(result.current({ kind: 'user', id: '2' }))).toEqual([]);
    const { container } = render(<ReportButton target={{ kind: 'library', id: '3' }} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('when on, the entry carries its target to the dialog store', () => {
    setReportsEnabled(true);
    const { result } = renderHook(() => useReportEntry());
    const entry = result.current({ kind: 'message', id: '1', channelId: '9' });
    expect(entry).toMatchObject({ label: 'Report Message', danger: true });
    expect(useReports.getState().target).toBeNull();
    act(() => {
      (entry as { onSelect: () => void }).onSelect();
    });
    expect(useReports.getState().target).toEqual({ kind: 'message', id: '1', channelId: '9' });
    expect(result.current({ kind: 'guild', id: '7' })).toMatchObject({ label: 'Report Server' });
  });
});

describe('client flag feed (D-216)', () => {
  it('turns reporting on from GET /sr/features/flags and leaves it off on failure', async () => {
    const { http, HttpResponse, server } = await import('../../test/server');
    const { loadClientFlags, useReports } = await import('../reports');
    server.use(
      http.get('*/api/v1/sr/features/flags', () =>
        HttpResponse.json({ flags: { 'moderation.reports': true } }),
      ),
    );
    await loadClientFlags();
    expect(useReports.getState().enabled).toBe(true);
    setReportsEnabled(false);
    server.use(
      http.get('*/api/v1/sr/features/flags', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    await loadClientFlags();
    expect(useReports.getState().enabled).toBe(false);
  });
});

describe('SR_RESYNC (D-646)', () => {
  it('reads the flags again after a platform resync only', async () => {
    const { useClientFlags, ReportsFlagSync } = await import('../reports');
    const { emitDispatch } = await import('../../store/events');
    const { http, HttpResponse, server } = await import('../../test/server');
    let flags: Record<string, boolean> = { 'other.flag': true };
    let reads = 0;
    server.use(
      http.get('*/api/v1/sr/features/flags', () => {
        reads += 1;
        return HttpResponse.json({ flags });
      }),
    );
    const { unmount } = render(<ReportsFlagSync />);
    await vi.waitFor(() => expect(useClientFlags.getState().flags['other.flag']).toBe(true));
    const first = reads;
    // A server-scoped marker is not about flags.
    act(() => emitDispatch('SR_RESYNC', { scope: 'guild', guild_id: 'g1' }));
    await new Promise((r) => setTimeout(r, 30));
    expect(reads).toBe(first);
    // A broadcast was lost while the server was behind: the new map is read.
    flags = { 'other.flag': false, 'new.flag': true };
    act(() => emitDispatch('SR_RESYNC', { scope: 'platform' }));
    await vi.waitFor(() => expect(useClientFlags.getState().flags['new.flag']).toBe(true));
    expect(useClientFlags.getState().flags['other.flag']).toBe(false);
    unmount();
  });
});

describe('SR_FEATURE_UPDATE (G-81)', () => {
  it('replaces the whole flag map from the gateway event', async () => {
    const { flagsOfUpdate, setClientFlags, useClientFlags, useReports, ReportsFlagSync } =
      await import('../reports');
    const { emitDispatch } = await import('../../store/events');
    const { http, HttpResponse, server } = await import('../../test/server');
    server.use(
      http.get('*/api/v1/sr/features/flags', () =>
        HttpResponse.json({ flags: { 'moderation.reports': false, 'other.flag': true } }),
      ),
    );
    const { unmount } = render(<ReportsFlagSync />);
    await vi.waitFor(() => expect(useClientFlags.getState().flags['other.flag']).toBe(true));
    expect(useReports.getState().enabled).toBe(false);

    // The server flips reporting on: the whole map arrives, and a flag it no longer lists is gone.
    act(() => {
      emitDispatch('SR_FEATURE_UPDATE', {
        scope: 'platform',
        flags: { 'moderation.reports': true },
      });
    });
    expect(useReports.getState().enabled).toBe(true);
    expect(useClientFlags.getState().flags).toEqual({ 'moderation.reports': true });

    // Other events and other scopes are ignored.
    act(() => {
      emitDispatch('SR_FEATURE_UPDATE', { scope: 'guild', flags: { 'moderation.reports': false } });
      emitDispatch('MESSAGE_CREATE', { flags: {} });
    });
    expect(useReports.getState().enabled).toBe(true);
    expect(
      flagsOfUpdate('SR_FEATURE_UPDATE', { scope: 'platform', flags: { a: 1, b: true } }),
    ).toEqual({
      a: false,
      b: true,
    });
    expect(flagsOfUpdate('SR_FEATURE_UPDATE', null)).toBeNull();
    unmount();
    // After unmount the event no longer reaches it.
    setClientFlags({});
    act(() => {
      emitDispatch('SR_FEATURE_UPDATE', {
        scope: 'platform',
        flags: { 'moderation.reports': true },
      });
    });
    expect(useReports.getState().enabled).toBe(false);
  });
});
