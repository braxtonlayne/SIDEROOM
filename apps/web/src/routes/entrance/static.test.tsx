import { render, screen, within } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createAppRouter } from '../../router';

function at(path: string) {
  render(
    <RouterProvider router={createAppRouter(createMemoryHistory({ initialEntries: [path] }))} />,
  );
}

describe('static entrance pages', () => {
  it('house notes carry every anchor the Library footer links to', async () => {
    at('/house-notes');
    expect(await screen.findByRole('heading', { level: 1, name: 'House notes' })).toBeVisible();
    const footer = readFileSync(resolve(__dirname, '../../library/Home.tsx'), 'utf8');
    const anchors = [...footer.matchAll(/href="\/house-notes#([a-z-]+)"/g)].map((m) => m[1]!);
    expect(anchors.length).toBeGreaterThanOrEqual(3);
    for (const id of anchors) {
      const el = document.getElementById(id);
      expect(el, `#${id}`).not.toBeNull();
      expect(el!.tagName).toBe('SECTION');
      // Each anchor is a labelled region with a heading, so it is reachable by landmark navigation.
      expect(el!.getAttribute('aria-labelledby')).toBeTruthy();
    }
  });

  it('the Library footer help link lands on a real page', async () => {
    const footer = readFileSync(resolve(__dirname, '../../library/Home.tsx'), 'utf8');
    expect(footer).toContain('href="/support"');
    at('/support');
    expect(await screen.findByRole('heading', { level: 1, name: 'Help' })).toBeVisible();
    const main = within(screen.getByRole('main'));
    expect(main.getByRole('link', { name: 'Reset my password' })).toHaveAttribute(
      'href',
      '/forgot',
    );
    expect(main.getByRole('link', { name: 'House notes' })).toHaveAttribute('href', '/house-notes');
  });
});
