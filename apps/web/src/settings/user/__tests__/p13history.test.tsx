import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('History management (LNG-USET-035)', () => {
  it('removes one watched item and one read chapter on their own', async () => {
    const gone: string[] = [];
    let watchRemoved = false;
    let readRemoved = false;
    server.use(
      http.get('*/api/v1/cinema/me/history', () =>
        HttpResponse.json({
          items: watchRemoved
            ? []
            : [
                {
                  title: { id: 't1', kind: 'series', title: 'Space Show' },
                  episode: { id: 'e1', title_id: 't1', season: 1, number: 3, title: 'Landing' },
                  progress: null,
                },
              ],
        }),
      ),
      http.get('*/api/v1/cinema/me/history/settings', () => HttpResponse.json({ paused: false })),
      http.delete('*/api/v1/cinema/titles/t1/episodes/e1/progress', () => {
        watchRemoved = true;
        gone.push('episode');
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/library/me/progress', () =>
        HttpResponse.json({
          items: readRemoved
            ? []
            : [
                {
                  series: { id: 's1', title: 'Blue Hour', cover_url: null },
                  chapter: {
                    id: 'c9',
                    number: 12,
                    page_count: 20,
                    progress: { page_index: 5, percent: 25, completed: false, updated_at_ms: 1 },
                  },
                },
              ],
        }),
      ),
      http.delete('*/api/v1/library/chapters/c9/progress', () => {
        readRemoved = true;
        gone.push('chapter');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('activity');
    await user.click(await screen.findByRole('button', { name: 'Remove Space Show from history' }));
    expect(await screen.findByText('Nothing in your watch history.')).toBeInTheDocument();
    expect(await screen.findByText('Chapter 12 · 25% read')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove Blue Hour from history' }));
    expect(await screen.findByText('Nothing in your reading history.')).toBeInTheDocument();
    await waitFor(() => expect(gone).toEqual(['episode', 'chapter']));
  });
});
