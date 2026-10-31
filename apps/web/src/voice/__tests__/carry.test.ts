import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The gateway client is replaced by a counter of opens and closes.
const opened = vi.fn();
const closed = vi.fn();
vi.mock('../../gateway/client', () => ({
  GatewayClient: class {
    connect() {
      opened();
    }
    close() {
      closed();
    }
  },
}));

import { connectGateway, disconnectGateway, getGateway } from '../../store/connect';
import { carryCall, keepCarriedCall, releaseCarriedCall } from '../carry';
import { patchSession, resetSession } from '../session';

beforeEach(() => {
  vi.useFakeTimers();
  opened.mockClear();
  closed.mockClear();
});
afterEach(() => {
  releaseCarriedCall();
  disconnectGateway();
  resetSession();
  vi.useRealTimers();
});

describe('carrying a call out of the lounge (wave 37, Games)', () => {
  it('keeps the gateway open past the lounge while a page keeps the call, then lets it go', () => {
    patchSession({ status: 'connected' });
    connectGateway(); // the lounge
    carryCall(); // the Games link
    disconnectGateway(); // the lounge unmounts
    const stop = keepCarriedCall(); // the Games page mounts
    expect(closed).not.toHaveBeenCalled();
    expect(getGateway()).not.toBeNull();
    stop(); // the Games page goes elsewhere (not the lounge)
    vi.runAllTimers();
    expect(closed).toHaveBeenCalledTimes(1);
  });

  it('survives the development double mount, and a lounge that comes back keeps its gateway', () => {
    patchSession({ status: 'connected' });
    connectGateway();
    carryCall();
    disconnectGateway();
    const first = keepCarriedCall();
    first(); // StrictMode's simulated unmount
    const stop = keepCarriedCall(); // and the remount
    vi.runAllTimers();
    expect(closed).not.toHaveBeenCalled();
    stop(); // back to the lounge: it opens the gateway again before the hold goes
    connectGateway();
    vi.runAllTimers();
    expect(closed).not.toHaveBeenCalled();
    expect(opened).toHaveBeenCalledTimes(1);
  });

  it('does nothing outside a call', () => {
    connectGateway();
    carryCall();
    disconnectGateway();
    expect(closed).toHaveBeenCalledTimes(1);
  });
});
