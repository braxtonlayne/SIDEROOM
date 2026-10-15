import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { http, HttpResponse, server } from '../../test/server';
import { NamingGuide } from '../NamingGuide';

describe('Naming guide tester', () => {
  it('parses pasted names and shows how they are read', async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    server.use(
      http.post('*/api/v1/submissions/naming/parse', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          items: [
            {
              name: 'Series v02 c013 - Title [Grp].cbz',
              cinema: null,
              library: {
                role: 'archive',
                series: 'Series',
                volume: 2,
                chapter: 13,
                title: 'Title',
                group: 'Grp',
                page: null,
                warnings: [],
              },
            },
          ],
        });
      }),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <NamingGuide />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: /Naming guide and tester/ }));
    await user.click(screen.getByLabelText('File names'));
    await user.paste('Series v02 c013 - Title [Grp].cbz');
    await user.click(screen.getByRole('button', { name: 'Test names' }));
    const table = await screen.findByRole('table', { name: 'Parsed names' });
    expect(
      within(table).getByText(/Archive · Series · Volume 2 · Chapter 13 · Title · \[Grp\]/),
    ).toBeInTheDocument();
    expect(bodies).toEqual([{ section: 'library', names: ['Series v02 c013 - Title [Grp].cbz'] }]);
  });
});
