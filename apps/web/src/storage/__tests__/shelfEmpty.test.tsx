import { render, screen } from '@testing-library/react';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ShelfEmptyState } from '../ShelfEmpty';

describe('ShelfEmptyState (BYOS-NAV-001)', () => {
  it.each([
    ['cinema', 'No films or shows are reachable yet.'],
    ['library', 'No books, comics or manga are reachable yet.'],
    ['music', 'No tracks are reachable yet.'],
  ] as const)('says what is missing for %s and how to fix it', (kind, text) => {
    render(<ShelfEmptyState kind={kind} />);
    expect(screen.getByText('Nothing here yet')).toBeInTheDocument();
    expect(screen.getByText(new RegExp(text, 'i'))).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Mount a bucket' })).toHaveAttribute(
      'href',
      '/settings/user/storage',
    );
    expect(screen.getByRole('link', { name: 'Ask a friend' })).toBeInTheDocument();
  });

  it('has no accessibility violations', async () => {
    render(<ShelfEmptyState kind="cinema" />);
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
