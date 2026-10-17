// Review Queues > Packages (EXT-PKG-003).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const version = (over: object = {}) => ({
  id: '801',
  package_id: '800',
  kind: 'theme',
  slug: 'ocean-night',
  name: 'Ocean Night',
  version: '1.2.0',
  review_state: 'pending',
  review_note: null,
  size_bytes: 1200,
  created_at_ms: 1,
  files: [
    { path: 'theme.json', size: 200 },
    { path: 'tokens.json', size: 90 },
  ],
  warnings: ['tokens.--ui-text: contrast is low'],
  author_id: '9',
  author_name: 'Ada',
  payload: { scheme: 'dark', tokens: { '--ui-accent': '#0af', '--ui-bg': '#012' } },
  ...over,
});

describe('review queues hub: Packages (EXT-PKG-003)', () => {
  it('lists what is waiting with a summary and decides it', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    const sent: unknown[] = [];
    let waiting = [
      version(),
      version({
        id: '802',
        kind: 'recipe',
        slug: 'club-starter',
        name: 'Club starter',
        version: '1.0.0',
        warnings: [],
        files: [{ path: 'recipe.json', size: 300 }],
        payload: { features: [{ id: 'lounge.starboard' }, { id: 'lounge.sticky' }] },
      }),
    ];
    server.use(
      http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json(waiting)),
      http.post(
        '*/api/v1/admin/developers/packages/versions/:id/decision',
        async ({ request, params }) => {
          sent.push([params.id, await readJson(request)]);
          waiting = waiting.filter((w) => w.id !== params.id);
          return HttpResponse.json(version({ review_state: 'approved' }));
        },
      ),
    );
    openAdmin('review');
    const user = userEvent.setup();
    expect(await screen.findByText('Ocean Night')).toBeInTheDocument();
    expect(screen.getByText('2 tokens set, dark scheme')).toBeInTheDocument();
    expect(screen.getByText('Configures: lounge.starboard, lounge.sticky')).toBeInTheDocument();
    expect(screen.getByText('tokens.--ui-text: contrast is low')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Download the zip of Ocean Night' })).toHaveAttribute(
      'href',
      '/api/v1/admin/developers/packages/versions/801/download',
    );

    await user.click(screen.getByRole('button', { name: 'Approve Ocean Night 1.2.0' }));
    const dialog = await screen.findByRole('dialog', { name: 'Approve this version' });
    await user.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(sent).toEqual([['801', { decision: 'approve' }]]));
    expect(await screen.findByText('Decision recorded.')).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText('Ocean Night')).toBeNull());

    await user.click(screen.getByRole('button', { name: 'Reject Club starter 1.0.0' }));
    const reject = await screen.findByRole('dialog', { name: 'Reject this version' });
    await user.type(
      within(reject).getByLabelText('Note for the creator'),
      'Overlaps another recipe',
    );
    await user.click(within(reject).getByRole('button', { name: 'Confirm' }));
    await waitFor(() =>
      expect(sent[1]).toEqual(['802', { decision: 'reject', note: 'Overlaps another recipe' }]),
    );
  });

  it('says so when nothing is waiting', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    server.use(http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])));
    openAdmin('review');
    expect(await screen.findByText('No packages are waiting for review.')).toBeInTheDocument();
  });
});
