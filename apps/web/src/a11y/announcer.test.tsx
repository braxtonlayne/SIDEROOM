import { act, render, screen } from '@testing-library/react';
import { announce, LiveRegions } from './announcer';

describe('announce', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('fills the polite region, and assertive messages go to the alert region', () => {
    render(<LiveRegions />);
    act(() => {
      announce('3 new messages');
      announce('Call dropped', 'assertive');
      vi.advanceTimersByTime(60);
    });
    expect(screen.getByRole('status')).toHaveTextContent('3 new messages');
    expect(screen.getByRole('alert')).toHaveTextContent('Call dropped');
  });

  it('re-announces identical text and clears itself afterwards', () => {
    render(<LiveRegions />);
    act(() => {
      announce('Saved');
      vi.advanceTimersByTime(60);
    });
    expect(screen.getByRole('status')).toHaveTextContent('Saved');
    act(() => {
      announce('Saved');
    });
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
    act(() => void vi.advanceTimersByTime(60));
    expect(screen.getByRole('status')).toHaveTextContent('Saved');
    act(() => void vi.advanceTimersByTime(9000));
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
  });
});
