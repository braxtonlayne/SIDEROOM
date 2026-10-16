import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { http, HttpResponse, server } from '../../test/server';
import { DuplicateWarning, SeriesMetaEditor, offered } from '../SeriesMetaEditor';
import { preview } from './fixtures';

const wrap = (ui: React.ReactNode) =>
  render(<QueryClientProvider client={new QueryClient()}>{ui}</QueryClientProvider>);

const candidate = {
  provider: 'anilist',
  external_id: '1',
  title: 'Berserk',
  alt_titles: [],
  year: 1989,
  synopsis: 'A mercenary and his sword.',
  authors: ['Kentaro Miura'],
  genres: ['Action'],
  url: null,
  cover: '',
};

describe('fetch details', () => {
  it('offers only the fields the provider filled', () => {
    expect(offered({ ...candidate, synopsis: '', authors: [] })).toEqual({
      title: 'Berserk',
      year: '1989',
      genres: 'Action',
    });
  });

  it('takes over the fields the person ticks and saves the edited series', async () => {
    const user = userEvent.setup();
    const saved: unknown[] = [];
    server.use(
      http.get('*/api/v1/submissions/lookup/providers', () =>
        HttpResponse.json({ enabled: true, providers: [{ id: 'anilist', label: 'AniList' }] }),
      ),
      http.get('*/api/v1/submissions/lookup', () => HttpResponse.json({ candidates: [candidate] })),
      http.put('*/api/v1/submissions/5/library/series', async ({ request }) => {
        saved.push(await request.json());
        return HttpResponse.json({});
      }),
    );
    const p = preview();
    p.series.title = 'Berserk';
    p.series.synopsis = '';
    p.series.year = null;
    p.series.authors = [];
    p.series.genres = [];
    wrap(<SeriesMetaEditor submissionId="5" preview={p} onSaved={() => undefined} />);
    await user.click(await screen.findByRole('button', { name: 'Search' }));
    await user.click(await screen.findByRole('button', { name: 'Use Berserk' }));
    // Empty fields are ticked for the person; untick the authors, then apply.
    const authors = screen.getByRole('checkbox', { name: 'Take Authors' });
    expect(authors).toBeChecked();
    await user.click(authors);
    await user.click(screen.getByRole('button', { name: /Apply 4 fields|Apply 3 fields/ }));
    expect(screen.getByLabelText('Synopsis')).toHaveValue('A mercenary and his sword.');
    expect(screen.getByLabelText('Year')).toHaveValue('1989');
    expect(screen.getByLabelText('Authors')).toHaveValue('');
    await user.click(screen.getByRole('button', { name: 'Save details' }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      title: 'Berserk',
      year: 1989,
      synopsis: 'A mercenary and his sword.',
      genres: ['Action'],
      authors: [],
    });
  });

  it('shows nothing while lookup is off', async () => {
    server.use(
      http.get('*/api/v1/submissions/lookup/providers', () =>
        HttpResponse.json({ enabled: false, providers: [] }),
      ),
    );
    wrap(<SeriesMetaEditor submissionId="5" preview={preview()} onSaved={() => undefined} />);
    expect(await screen.findByRole('button', { name: 'Save details' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Fetch details' })).toBeNull();
  });
});

describe('duplicate warning', () => {
  it('warns about a similar series and stays quiet otherwise', async () => {
    server.use(
      http.get('*/api/v1/submissions/5/duplicates', () =>
        HttpResponse.json({
          matches: [
            {
              section: 'library',
              ref_id: '9',
              title: 'Berserk',
              year: 1989,
              score: 100,
              reasons: ['title'],
              href: '/library/series/9',
            },
          ],
          resolution: null,
        }),
      ),
    );
    wrap(<DuplicateWarning submissionId="5" />);
    expect(await screen.findByText('This looks like a series we already have')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Berserk' })).toHaveAttribute(
      'href',
      '/library/series/9',
    );
  });
});
