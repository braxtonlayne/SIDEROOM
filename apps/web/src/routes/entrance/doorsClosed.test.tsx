// OPS-INV-001 / D-385: the entrance's only change is the "doors are closed" notice (Lock the Doors).
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';
import { resetCompliancePublic } from '../../lib/compliancePublic';
import { nav } from '../../lib/enterApp';

const CLOSED = /The doors are closed right now/;

beforeEach(() => {
  resetCompliancePublic();
  vi.spyOn(nav, 'assign').mockImplementation(() => {});
  vi.spyOn(nav, 'replace').mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
  resetCompliancePublic();
});

function at(path: string) {
  render(
    <RouterProvider
      router={createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }))}
    />,
  );
}

describe('the doors-closed notice', () => {
  it('shows on the landing page as soon as the operator has locked the doors', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, doors_closed: true }),
      ),
    );
    at('/');
    const note = await screen.findByText(CLOSED);
    expect(note).toHaveAttribute('role', 'status');
  });

  it('is absent while the doors are open', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, doors_closed: false }),
      ),
    );
    at('/');
    await screen.findByLabelText('Your invitation');
    expect(screen.queryByText(CLOSED)).toBeNull();
  });

  it('answers a checked code with the same words and keeps the code out of storage', async () => {
    server.use(
      http.get('*/api/v1/sr/invites/:code', () =>
        HttpResponse.json({ valid: false, reason: 'locked' }),
      ),
    );
    const user = userEvent.setup();
    at('/');
    await user.type(await screen.findByLabelText('Your invitation'), 'ABCDEFGHJK');
    await user.click(screen.getByRole('button', { name: /check invitation/i }));
    expect(await screen.findByText(CLOSED)).toBeInTheDocument();
    expect(sessionStorage.getItem('sr.invite')).toBeNull();
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByLabelText('Your invitation')),
    );
  });

  it('shows on the register page too', async () => {
    sessionStorage.setItem('sr.invite', 'ABCDE-FGHJK');
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, doors_closed: true }),
      ),
      http.get('*/api/v1/sr/auth/discord/pending', () => HttpResponse.json(null)),
      http.get('*/api/v1/sr/usernames/check', () => HttpResponse.json({ available: true })),
    );
    at('/register');
    expect(await screen.findByText(CLOSED)).toBeInTheDocument();
    sessionStorage.removeItem('sr.invite');
  });
});
