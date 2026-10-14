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
