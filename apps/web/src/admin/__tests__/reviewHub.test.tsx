// Review Queues hub (ADM-CNT-001): one tab per queue the staff member may work.
import { screen } from '@testing-library/react';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const TABS = ['Cinema', 'Submissions', 'Apps', 'Packages', 'Directory'];

describe('review queues hub tabs (ADM-CNT-001)', () => {
  it('shows Cinema, Submissions, Apps, Packages and Directory to staff who may work all of them', async () => {
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
    // The submissions queue holds Cinema submissions too, so it is no longer called Library.
    expect(screen.queryByRole('tab', { name: 'Library' })).toBeNull();
  });

  it('opens the Submissions tab for a card that links to a submission', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'DELETE_CONTENT', 'REVIEW_READING'] });
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ items: [], total: 0, limit: 25, offset: 0 }),
      ),
    );
    server.use(
      http.get('*/api/v1/admin/review/submissions/*', () =>
        HttpResponse.json({ error: 'not found' }, { status: 404 }),
      ),
    );
    openAdmin('review?submission=107586859906891776');
    expect(await screen.findByRole('tab', { name: 'Submissions', selected: true })).toBeVisible();
  });

  it('leaves out the queues the staff member has no permission for', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    server.use(http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])));
    openAdmin('review');
    expect(await screen.findByRole('tab', { name: 'Packages' })).toBeInTheDocument();
    for (const name of ['Cinema', 'Submissions', 'Apps', 'Directory']) {
      expect(screen.queryByRole('tab', { name })).toBeNull();
    }
  });
});
