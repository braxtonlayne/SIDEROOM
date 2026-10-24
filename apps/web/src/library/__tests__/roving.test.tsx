import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { mockGridRows, tabStops } from '../../a11y/rovingTestUtils';
import { localProgress, setProgressApi } from '../progress';
import { renderAt, series, seriesPage, useLibraryApi } from './harness';

beforeEach(() => {
  localStorage.clear();
  setProgressApi(localProgress);
});

/** RDY-A11Y-002: the Library's card grids are one Tab stop with arrow keys, and the shell has a skip link. */
describe('Library keyboard navigation', () => {
  it('Browse: the card grid is one Tab stop and the arrows move by card and by row', async () => {
    const restore = mockGridRows('[data-poster-link]', 3);
    useLibraryApi({
      browse: () =>
        seriesPage(
          Array.from({ length: 7 }, (_, i) => series(`k${i}`, { title: `Series ${i}` })),
          7,
        ),
    });
    const user = userEvent.setup();
    renderAt('/library/browse');
    await screen.findByText('Series 0');
    const grid = screen.getByTestId('browse-grid');
    const cards = () => within(grid).getAllByRole('link', { name: /^Series \d/ });
    await waitFor(() => expect(tabStops(cards())).toEqual([0, -1, -1, -1, -1, -1, -1]));
    cards()[0]!.focus();
    await user.keyboard('{ArrowRight}');
    expect(cards()[1]).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(cards()[4]).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(cards()[6]).toHaveFocus();
    await user.keyboard('{Home}');
    expect(cards()[0]).toHaveFocus();
    expect(tabStops(cards())).toEqual([0, -1, -1, -1, -1, -1, -1]);
    restore();
  });

  it('has a skip link to the main region', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    renderAt('/library');
    await screen.findByText('Frieren');
    await user.click(screen.getByRole('link', { name: 'Skip to main content' }));
    expect(screen.getByRole('main')).toHaveFocus();
  });
});
