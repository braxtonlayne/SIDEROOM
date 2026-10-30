// RDY-A11Y-001 (a11y audit): focus after a refused code, and the reduced-motion preference on the glimpse scroll.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';

function at(path: string) {
  render(
    <RouterProvider router={createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }))} />,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe('invite field focus', () => {
  it.each([
    [{ valid: false }, "That invitation doesn't open this door."],
    [{ valid: false, reason: 'closed' }, "The door's closed for now. We're at capacity."],
  ])('returns to the input after a refused code %j', async (body, text) => {
    server.use(http.get('*/api/v1/sr/invites/:code', () => HttpResponse.json(body)));
    const user = userEvent.setup();
    at('/');
    const input = await screen.findByLabelText('Your invitation');
    await user.type(input, 'ABCDEFGHJK');
    await user.click(screen.getByRole('button', { name: /check invitation/i }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByLabelText('Your invitation')),
    );
  });
});
