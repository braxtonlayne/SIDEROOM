import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setClientFlags } from '../../moderation/reports';
import { BugReportGate } from '../BugReportGate';
import { BETA_REPORTS_FLAG } from '../flag';
import { bugReportRequest, closeBugReport, openBugReport } from '../openState';

// The dialog chunk is not under test here.
vi.mock('../BugReportDialog', () => ({
  BugReportDialog: () => <div data-testid="dialog" />,
}));

const press = (init: KeyboardEventInit = {}) =>
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'B',
        altKey: true,
        shiftKey: true,
        bubbles: true,
        ...init,
      }),
    );
  });

beforeEach(() => {
  closeBugReport();
});
afterEach(() => {
  closeBugReport();
  setClientFlags({});
  vi.restoreAllMocks();
});

describe('BugReportGate (D-647)', () => {
  it('with the switch off renders nothing, adds no listener and ignores the key', () => {
    setClientFlags({});
    const add = vi.spyOn(window, 'addEventListener');
    const { container } = render(<BugReportGate />);
    expect(container).toBeEmptyDOMElement();
    expect(add.mock.calls.filter(([type]) => type === 'keydown')).toHaveLength(0);
    press();
    expect(bugReportRequest().open).toBe(false);
  });

  it('with the switch on the key opens the dialog once, without a picture where nothing can capture', async () => {
    setClientFlags({ [BETA_REPORTS_FLAG]: true });
    const { getByTestId, queryByTestId } = render(<BugReportGate />);
    expect(queryByTestId('dialog')).toBeNull();
    press();
    expect(bugReportRequest().open).toBe(true);
    expect(bugReportRequest().start?.note).toBe('unsupported');
    expect(bugReportRequest().start?.frame).toBeUndefined();
    const nonce = bugReportRequest().nonce;
    press(); // already open: ignored
    expect(bugReportRequest().nonce).toBe(nonce);
    await vi.waitFor(() => expect(getByTestId('dialog')).toBeInTheDocument());
  });

  it('a menu entry opens it too, and turning the switch off closes it', () => {
    setClientFlags({ [BETA_REPORTS_FLAG]: true });
    render(<BugReportGate />);
    act(() => openBugReport());
    expect(bugReportRequest().open).toBe(true);
    act(() => setClientFlags({}));
    expect(bugReportRequest().open).toBe(false);
  });

  it('other keys do nothing', () => {
    setClientFlags({ [BETA_REPORTS_FLAG]: true });
    render(<BugReportGate />);
    press({ key: 'P' });
    expect(bugReportRequest().open).toBe(false);
  });
});
