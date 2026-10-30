import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { closePlayerLibrary, isPlayerLibraryOpen } from '../../player/presence';
import { server } from '../../test/server';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('MUS-PLY-002 rail music button', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    closePlayerLibrary();
    baseHandlers();
    seedLounge({ messages: [] });
  });
  afterEach(() => server.resetHandlers());

  it('is a labelled, keyboard-reachable button that asks for the library', async () => {
    const user = userEvent.setup();
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    const btn = await within(rail).findByRole('button', { name: 'Music library' });
    expect(btn.tabIndex).toBeGreaterThanOrEqual(-1);
    expect(isPlayerLibraryOpen()).toBe(false);
    btn.focus();
    await user.keyboard('{Enter}');
    expect(isPlayerLibraryOpen()).toBe(true);
  });
});
