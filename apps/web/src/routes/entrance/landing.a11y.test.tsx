// RDY-A11Y-001 (a11y audit): focus after a refused code, and the reduced-motion preference on the glimpse scroll.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';

function at(path: string) {
  render(
    <RouterProvider router={createAppRouter(createMemoryHistory({ initialEntries: [path] }))} />,
  );
}

function stubReducedMotion(reduce: boolean) {
  vi.stubGlobal(
    'matchMedia',
    (query: string) =>
      ({
        matches: reduce && query.includes('prefers-reduced-motion'),
        media: query,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        addListener: () => undefined,
        removeListener: () => undefined,
        onchange: null,
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList,
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

describe('the glimpse scroll', () => {
  const click = async (reduce: boolean) => {
    stubReducedMotion(reduce);
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    const user = userEvent.setup();
    at('/');
    await user.click(await screen.findByRole('button', { name: /a glimpse/i }));
    return scroll;
  };

  it('animates by default', async () => {
    expect(await click(false)).toHaveBeenCalledWith({ behavior: 'smooth' });
  });

  it('jumps when the person asked for reduced motion', async () => {
    expect(await click(true)).toHaveBeenCalledWith({ behavior: 'auto' });
  });
});
