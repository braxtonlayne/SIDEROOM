import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IDLE_AFTER_MS, startIdleReporter } from '../shell/idleReporter';

describe('idle reporter (G-51)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('sends afk after ten quiet minutes and clears it on the next input', () => {
    const send = vi.fn();
    const stop = startIdleReporter(send);
    vi.advanceTimersByTime(IDLE_AFTER_MS - 1000);
    expect(send).not.toHaveBeenCalled();
    window.dispatchEvent(new Event('keydown'));
    vi.advanceTimersByTime(IDLE_AFTER_MS - 1000);
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2000);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenLastCalledWith(true);
    // More quiet time does not repeat it.
    vi.advanceTimersByTime(IDLE_AFTER_MS);
    expect(send).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new Event('pointermove'));
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith(false);
    stop();
    vi.advanceTimersByTime(IDLE_AFTER_MS * 2);
    expect(send).toHaveBeenCalledTimes(2);
  });
});
