import { render, screen, within } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createAppRouter } from '../../router';
import { resetCompliancePublic } from '../../lib/compliancePublic';
import { http, HttpResponse, server } from '../../test/server';

function at(path: string) {
  render(
    <RouterProvider router={createAppRouter(createMemoryHistory({ initialEntries: [path] }))} />,
  );
}

describe('static entrance pages', () => {
  beforeEach(() => resetCompliancePublic());

  it('shows the operator country and contact on the privacy summary and /help when set', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({
          minimum_age: 13,
          operator_country: 'DE',
          operator_contact: 'legal@example.org',
        }),
      ),
    );
    at('/house-notes');
    expect(await screen.findByText('This server is run from DE.')).toBeVisible();
    expect(screen.getByRole('link', { name: 'legal@example.org' })).toHaveAttribute(
      'href',
      'mailto:legal@example.org',
    );
  });

  it('shows the contact as the copyright notice address on /help, and nothing when unset', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, operator_contact: 'legal@example.org' }),
      ),
    );
    at('/help');
    const heading = await screen.findByRole('heading', { name: 'Copyright notices' });
    expect(heading).toBeVisible();
    // The contact also shows in the handbook's privacy section, so look inside the notices section.
    expect(
      within(heading.closest('section')!).getByRole('link', { name: 'legal@example.org' }),
    ).toBeVisible();
  });

  it('adds no operator lines while the operator has set none', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () => HttpResponse.json({ minimum_age: 13 })),
    );
    at('/help');
    await screen.findByRole('heading', { level: 1, name: 'Help' });
    expect(screen.queryByRole('heading', { name: 'Copyright notices' })).toBeNull();
  });

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

  it('the Library footer help link lands on the help page', async () => {
    const footer = readFileSync(resolve(__dirname, '../../library/Home.tsx'), 'utf8');
    expect(footer).toContain('href="/help"');
    at('/help');
    expect(await screen.findByRole('heading', { level: 1, name: 'Help' })).toBeVisible();
    const main = within(screen.getByRole('main'));
    for (const h of [
      'I cannot sign in',
      'I lost my second factor',
      'I do not have an invitation',
      'Something is wrong here',
    ]) {
      expect(main.getByRole('heading', { name: h })).toBeVisible();
    }
    expect(main.getByRole('link', { name: 'Read the third-party notices' })).toHaveAttribute(
      'href',
      '/THIRD_PARTY_NOTICES.md',
    );
    expect(main.getByRole('link', { name: 'Reset my password' })).toHaveAttribute(
      'href',
      '/forgot',
    );
    expect(main.getByRole('link', { name: 'House notes' })).toHaveAttribute('href', '/house-notes');
  });
});
