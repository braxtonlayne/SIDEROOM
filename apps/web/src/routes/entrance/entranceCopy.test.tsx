// ENT-UI-010 / ENT-UI-025: the Entrance shows staff-edited copy when there is some and the built-in text otherwise.
import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';
import { EntranceMarkdown, resetEntranceHero, setEntranceCopy } from './entranceCopy';

function at(path: string) {
  render(
    <RouterProvider router={createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }))} />,
  );
}

beforeEach(() => setEntranceCopy(null));

describe('operator copy on the Entrance', () => {
  it('replaces the no-invitation text and keeps the heading', async () => {
    server.use(
      http.get('*/api/v1/sr/entrance/copy', () =>
        HttpResponse.json({ 'no-invitation': 'Ask **Sam** at the front desk.' }),
      ),
    );
    at('/no-invitation');
    expect(await screen.findByText('Sam')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1 })).toBeVisible();
    expect(screen.queryByText(/invitation-only/i)).toBeNull();
  });

  it('shows the landing note under the invitation box', async () => {
    server.use(
      http.get('*/api/v1/sr/entrance/copy', () =>
        HttpResponse.json({ 'landing-note': 'Opening hours: evenings.' }),
      ),
    );
    at('/');
    expect(await screen.findByText('Opening hours: evenings.')).toBeInTheDocument();
  });
});

describe('EntranceMarkdown', () => {
  it('renders the restricted subset and never raw HTML or script links', () => {
    render(
      <EntranceMarkdown
        source={
          '# Title\n\nSome *em* and **strong** and `code`.\n\n1. one\n2. two\n\n[ok](https://example.test) [bad](javascript:alert(1)) <img src=x onerror=alert(1)>'
        }
      />,
    );
    expect(screen.getByRole('heading', { level: 2, name: 'Title' })).toBeVisible();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByRole('link', { name: 'ok' })).toHaveAttribute(
      'href',
      'https://example.test',
    );
    expect(screen.queryByRole('link', { name: 'bad' })).toBeNull();
    expect(document.querySelector('img')).toBeNull();
    expect(screen.getByText(/<img src=x/)).toBeInTheDocument();
  });
});

describe('operator hero image on the landing page (H-41)', () => {
  it('shows the picture in the door slot when the server has one', async () => {
    server.use(
      http.get('*/api/v1/sr/entrance/copy', () =>
        HttpResponse.json({ 'hero-alt': 'A door ajar in a library' }),
      ),
      http.get(
        '*/api/v1/sr/entrance/hero',
        () =>
          new HttpResponse(new Uint8Array([137, 80, 78, 71]), {
            headers: { 'content-type': 'image/png' },
          }),
      ),
    );
    resetEntranceHero();
    at('/');
    const img = await screen.findByAltText('A door ajar in a library');
    expect(img).toHaveAttribute('src', '/api/v1/sr/entrance/hero');
    expect(screen.getByTestId('door')).toHaveAttribute('data-hero', 'true');
  });

  it('keeps the shipped art without an image', async () => {
    resetEntranceHero();
    at('/');
    expect(await screen.findByTestId('door')).not.toHaveAttribute('data-hero');
    expect(document.querySelector('.ent-door-img')).toBeNull();
  });
});
