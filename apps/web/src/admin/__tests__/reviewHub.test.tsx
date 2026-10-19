// Review Queues hub (ADM-CNT-001): one tab per queue the staff member may work.
import { screen } from '@testing-library/react';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const TABS = ['Cinema', 'Library', 'Apps', 'Packages', 'Directory'];

describe('review queues hub tabs (ADM-CNT-001)', () => {
  it('shows Cinema, Library, Apps, Packages and Directory to staff who may work all of them', async () => {
    asStaff({
      perms: [
        'STAFF_BASIC',
        'DELETE_CONTENT',
        'REVIEW_READING',
        'REVIEW_APPS',
        'REVIEW_PACKAGES',
        'MANAGE_DIRECTORY',
      ],
    });
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ items: [], total: 0, limit: 25, offset: 0 }),
      ),
    );
    openAdmin('review');
    for (const name of TABS) {
      expect(await screen.findByRole('tab', { name })).toBeInTheDocument();
    }
  });

  it('leaves out the queues the staff member has no permission for', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    server.use(http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])));
    openAdmin('review');
    expect(await screen.findByRole('tab', { name: 'Packages' })).toBeInTheDocument();
    for (const name of ['Cinema', 'Library', 'Apps', 'Directory']) {
      expect(screen.queryByRole('tab', { name })).toBeNull();
    }
  });
});
