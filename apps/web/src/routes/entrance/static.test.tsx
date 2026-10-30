import { render, screen, within } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadCatalog, renderHouseNotes } from '../../../../../scripts/render-legal.mjs';
import { createPublicRouter } from '../../test/publicRouter';
import { resetCompliancePublic } from '../../lib/compliancePublic';
import { http, HttpResponse, server } from '../../test/server';

function at(path: string) {
  render(
    <RouterProvider
      router={createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }))}
    />,
  );
}

describe('the sign-in answers on /login (they were the public /help page)', () => {
  beforeEach(() => resetCompliancePublic());

  it('lists the self-service answers, with plain links to the other surfaces', async () => {
    at('/login');
    const help = (await screen.findByText('Trouble signing in?')).closest('details')!;
    const main = within(help);
    for (const h of [
      'I cannot sign in',
      'I lost my second factor',
      'I do not have an invitation',
      'Something is wrong here',
    ])
      expect(main.getByRole('heading', { name: h, hidden: true })).toBeInTheDocument();
    expect(main.getByRole('link', { name: 'Reset my password', hidden: true })).toHaveAttribute(
      'href',
      '/forgot',
    );
    expect(main.getByRole('link', { name: 'House notes', hidden: true })).toHaveAttribute(
      'href',
      '/house-notes',
    );
  });

  it('shows the operator contact as the copyright notice address when set, and nothing when unset', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, operator_contact: 'legal@example.org' }),
      ),
    );
    at('/login');
    expect(
      await screen.findByRole('link', { name: 'legal@example.org', hidden: true }),
    ).toHaveAttribute('href', 'mailto:legal@example.org');
  });

  it('adds no operator lines while the operator has set none', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () => HttpResponse.json({ minimum_age: 13 })),
    );
    at('/login');
    await screen.findByText('Trouble signing in?');
    expect(screen.queryByRole('heading', { name: 'Copyright notices', hidden: true })).toBeNull();
  });
});

describe('the house notes (plain HTML rendered at build time, D-385)', () => {
  const html = renderHouseNotes(loadCatalog(), '/entrance/legal-test.css') as string;

  it('has no script, no event handler and one stylesheet', () => {
    expect(html).not.toMatch(/<script/i);
    expect(html).not.toMatch(/\son[a-z]+=/i);
    expect(html.match(/<link /g)).toHaveLength(1);
  });

  it('carries the anchors the registration form and the footers link to', () => {
    for (const id of ['notes-rules', 'notes-privacy', 'notes-terms', 'notes-credits'])
      expect(html).toContain(`id="${id}"`);
    expect(html).toContain('href="/fonts/README.md"');
  });

  it('leaves the two operator placeholders for the server to fill', () => {
    expect(html).toContain('{{operator_country}}');
    expect(html).toContain('mailto:{{operator_contact}}');
  });

  it('is linked from the sign-in surfaces as a plain anchor, never a router link', () => {
    const shell = readFileSync(resolve(__dirname, '../../components/EntranceShell.tsx'), 'utf8');
    expect(shell).toContain('<a href="/house-notes">');
  });
});
