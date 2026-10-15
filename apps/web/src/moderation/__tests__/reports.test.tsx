import { act, render, renderHook, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import {
  closeReport,
  entryList,
  ReportButton,
  ReportUnavailableDialog,
  setReportsEnabled,
  useReportEntry,
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

  it('when on, the entry opens a dialog that says reporting is unavailable', () => {
    setReportsEnabled(true);
    const { result } = renderHook(() => useReportEntry());
    const entry = result.current({ kind: 'message', id: '1' });
    expect(entry).toMatchObject({ label: 'Report Message', danger: true });
    render(<ReportUnavailableDialog />);
    expect(screen.queryByRole('dialog')).toBeNull();
    act(() => {
      (entry as { onSelect: () => void }).onSelect();
    });
    expect(screen.getByRole('dialog')).toHaveTextContent('Reporting is not available yet');
    expect(screen.getByRole('dialog')).toHaveTextContent('report a message');
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
