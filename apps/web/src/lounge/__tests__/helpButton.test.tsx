// The "?" menu (LNG-CHN-018) with the handbook, What's new (RDY-ONB-003) and the welcome again (RDY-ONB-001).
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider } from '@sideroom/ui';
import { APP_VERSION } from '../../lib/whatsNew';
import { http, HttpResponse, server } from '../../test/server';
import { HelpButton } from '../header/HelpButton';
import { useFirstRunUi } from '../shell/firstRun';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigate,
}));

const mount = () =>
  render(
    <ThemeProvider section="lounge">
      <HelpButton />
    </ThemeProvider>,
  );

beforeEach(() => {
  navigate.mockClear();
  localStorage.clear();
  useFirstRunUi.getState().hide();
});

describe('help menu', () => {
  it('lists the shortcuts, the handbook, what is new and the welcome', async () => {
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByRole('button', { name: 'Help' }));
    for (const name of ['Keyboard shortcuts', 'Help', "What's new", 'Show the welcome again']) {
      expect(await screen.findByRole('menuitem', { name })).toBeVisible();
    }
  });

  it("What's new opens the changelog page", async () => {
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByRole('button', { name: 'Help' }));
    await user.click(await screen.findByRole('menuitem', { name: "What's new" }));
    expect(navigate).toHaveBeenCalledWith({ to: '/help/whats-new' });
  });

  it('Show the welcome again stores not done and opens it', async () => {
    const puts: unknown[] = [];
    server.use(
      http.put('*/api/v1/sr/auth/first-run', async ({ request }) => {
        puts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByRole('button', { name: 'Help' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Show the welcome again' }));
    await waitFor(() => expect(useFirstRunUi.getState().forced).toBe(true));
    expect(puts).toEqual([{ done: false }]);
  });
});

describe('the one-time badge', () => {
  it('shows nothing to a browser that has never seen a version', () => {
    mount();
    expect(screen.getByRole('button', { name: 'Help' })).not.toHaveAttribute('data-news');
    expect(localStorage.getItem('sr.whatsNew.seen')).toBe(APP_VERSION);
  });

  it('shows a dot when the version changed since the last visit', () => {
    localStorage.setItem('sr.whatsNew.seen', 'Phase 1');
    mount();
    const button = screen.getByRole('button', { name: 'Help, there is news' });
    expect(button).toHaveAttribute('data-news');
  });

  it('does not show when this version was already seen', () => {
    localStorage.setItem('sr.whatsNew.seen', APP_VERSION);
    mount();
    expect(screen.getByRole('button', { name: 'Help' })).not.toHaveAttribute('data-news');
  });
});
