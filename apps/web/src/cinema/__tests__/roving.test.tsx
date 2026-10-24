import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { mockGridRows, tabStops } from '../../a11y/rovingTestUtils';
import { renderAt, title, useCinemaApi } from './harness';

/** RDY-A11Y-002: Cinema rows and grids are one Tab stop with arrow keys, and the shell has a skip link. */
describe('Cinema keyboard navigation', () => {
  it('Discover: each poster row is one Tab stop; Left and Right move along the row', async () => {
    useCinemaApi();
    const user = userEvent.setup();
    renderAt('/cinema');
    await screen.findByRole('heading', { level: 1, name: /Dune:/ });
    const rows = Array.from(document.querySelectorAll<HTMLElement>('.cin-row-wrap')).filter(
      (r) => r.querySelectorAll('[data-poster-link]').length >= 2,
    );
    expect(rows.length).toBeGreaterThan(0);
    const row = rows[0]!;
    const cards = () => Array.from(row.querySelectorAll<HTMLElement>('[data-poster-link]'));
    await waitFor(() => expect(tabStops(cards())).toEqual([0, ...cards().slice(1).map(() => -1)]));
    cards()[0]!.focus();
    await user.keyboard('{ArrowRight}');
    expect(cards()[1]).toHaveFocus();
    expect(tabStops(cards())[1]).toBe(0);
    // Up and Down are not row keys: they stay put (the page scrolls).
    await user.keyboard('{ArrowDown}');
    expect(cards()[1]).toHaveFocus();
    await user.keyboard('{End}');
    expect(cards().at(-1)).toHaveFocus();
  });

  it('Movies: the title grid moves by card and by row', async () => {
    const restore = mockGridRows('[data-poster-link]', 2);
    useCinemaApi({
      browse: () => ({
        items: ['a', 'b', 'c', 'd', 'e'].map((id) => title(id, { title: `Film ${id}` })),
        total: 5,
        next_cursor: null,
      }),
    });
    const user = userEvent.setup();
    renderAt('/cinema/movies');
    await screen.findByText('Film a');
    const grid = document.querySelector<HTMLElement>('.cin-grid')!;
    const cards = () =>
      within(grid).getAllByRole('link', { name: /^Film / }) as HTMLElement[];
    await waitFor(() => expect(tabStops(cards())).toEqual([0, -1, -1, -1, -1]));
    cards()[0]!.focus();
    await user.keyboard('{ArrowDown}');
    expect(cards()[2]).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    expect(cards()[3]).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(cards()[4]).toHaveFocus();
    await user.keyboard('{ArrowUp}{ArrowUp}');
    expect(cards()[0]).toHaveFocus();
    restore();
  });

  it('has a skip link to the main region', async () => {
    useCinemaApi();
    const user = userEvent.setup();
    renderAt('/cinema');
    await screen.findByRole('heading', { level: 1, name: /Dune:/ });
    await user.click(screen.getByRole('link', { name: 'Skip to main content' }));
    expect(screen.getByRole('main')).toHaveFocus();
  });
});
