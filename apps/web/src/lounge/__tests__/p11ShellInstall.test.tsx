import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { __resetPwaState } from '../../pwa/state';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('LNG-RAIL-015 install button on the rail', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    __resetPwaState();
  });

  it('is absent until the browser offers an install, then prompts', async () => {
    at('/channels/g1/c1');
    await screen.findByRole('button', { name: 'Discover' });
    expect(screen.queryByRole('button', { name: 'Install app' })).toBeNull();

    const prompt = vi.fn(() => Promise.resolve());
    const event = Object.assign(new Event('beforeinstallprompt', { cancelable: true }), {
      prompt,
      userChoice: Promise.resolve({ outcome: 'accepted' as const }),
    });
    act(() => {
      window.dispatchEvent(event);
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Install app' }));
    expect(prompt).toHaveBeenCalledOnce();
    // The prompt is spent, so the button goes away.
    await vi.waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Install app' })).toBeNull(),
    );
  });

  it('shows the Add to Home Screen steps on iOS Safari', async () => {
    const ua = vi
      .spyOn(navigator, 'userAgent', 'get')
      .mockReturnValue(
        'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1',
      );
    try {
      at('/channels/g1/c1');
      await userEvent.click(await screen.findByRole('button', { name: 'Install app' }));
      const dlg = await screen.findByRole('dialog', { name: 'Add SIDE ROOM to your Home Screen' });
      expect(dlg).toHaveTextContent('Tap the Share button in Safari.');
      await userEvent.click(screen.getByRole('button', { name: 'Got it' }));
      expect(screen.queryByRole('dialog', { name: /Home Screen/ })).toBeNull();
    } finally {
      ua.mockRestore();
    }
  });
});
