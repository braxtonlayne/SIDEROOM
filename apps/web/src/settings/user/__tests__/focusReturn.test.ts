import { afterEach, describe, expect, it, vi } from 'vitest';
import { setSettingsReturn, takeSettingsReturn } from '../nav';

/** RDY-A11Y-003, flow 8: closing Settings gives focus back to the control that opened them. */
describe('Settings focus return', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('focuses the opener, found by its accessible name, once the page it came from is back', async () => {
    const opener = document.createElement('button');
    opener.setAttribute('aria-label', 'User Settings');
    document.body.append(opener);
    opener.focus();
    setSettingsReturn('/channels/1/2');
    // Settings replaces the page: the opener is gone while Settings is open, and a new one appears on return.
    opener.remove();
    expect(takeSettingsReturn()).toBe('/channels/1/2');
    const back = document.createElement('button');
    back.setAttribute('aria-label', 'User Settings');
    document.body.append(back);
    await vi.waitFor(() => expect(back).toHaveFocus());
  });

  it('falls back to the main region when the opener had no name', async () => {
    const main = document.createElement('main');
    document.body.append(main);
    setSettingsReturn('/library');
    takeSettingsReturn();
    await vi.waitFor(() => expect(main).toHaveFocus());
    expect(main.getAttribute('tabindex')).toBe('-1');
  });

  it('opens at /channels when nothing was remembered', () => {
    expect(takeSettingsReturn()).toBe('/channels');
  });
});
