// D-587: the Applications tab of the Review hub: list, search, state filter, suspend (reason required) and
// unsuspend (reason optional), and the tab only for REVIEW_APPS holders.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const owner = { id: '7', username: 'ada', global_name: 'Ada' };
const app = (over: object = {}) => ({
  id: '900',
  name: 'Helper Bot',
  release_state: 'released',
  owner,
  suspended_reason: null,
  suspended_by: null,
  suspended_at: null,
  created_at: '2026-10-01T10:00:00Z',
  ...over,
});

async function openApplications() {
  openAdmin('review');
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole('tab', { name: 'All applications' }, { timeout: 15000 }),
  );
  return user;
}

describe('review hub: Applications (D-587)', () => {
  it('lists applications with their owner and release state, and shows who suspended one', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS'] });
    server.use(
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/activities', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/apps', () =>
        HttpResponse.json([
          app({
            id: '901',
            name: 'Spam Bot',
            release_state: 'suspended',
            suspended_reason: 'sends spam',
            suspended_by: { id: '4', username: 'rhea' },
            suspended_at: '2026-11-02T12:00:00Z',
          }),
          app(),
        ]),
      ),
    );
    await openApplications();
    expect(await screen.findByText('Helper Bot')).toBeInTheDocument();
    expect(screen.getAllByText('@ada').length).toBe(2);
    expect(screen.getByText('Released')).toBeInTheDocument();
    expect(screen.getByText('Reason: sends spam')).toBeInTheDocument();
    expect(screen.getByText(/Suspended by @rhea on/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Unsuspend Spam Bot' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Suspend Helper Bot' })).toBeInTheDocument();
  });

  it('searches and filters on the server', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS'] });
    const urls: string[] = [];
    server.use(
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/activities', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/apps', ({ request }) => {
        urls.push(new URL(request.url).search);
        return HttpResponse.json([app()]);
      }),
    );
    const user = await openApplications();
    await screen.findByText('Helper Bot');
    await user.type(screen.getByRole('searchbox', { name: 'Search' }), 'helper');
    await user.click(screen.getByRole('button', { name: 'Search' }));
    await waitFor(() => expect(urls.some((u) => u.includes('q=helper'))).toBe(true));
    await user.click(screen.getByRole('combobox', { name: 'Release state' }));
    await user.click(await screen.findByRole('option', { name: 'Suspended' }));
    await waitFor(() => expect(urls.some((u) => u.includes('state=suspended'))).toBe(true));
  });

  it('suspends with a required reason and unsuspends with an optional one', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS'] });
    const sent: unknown[] = [];
    let rows = [app()];
    server.use(
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/activities', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/apps', () => HttpResponse.json(rows)),
      http.post('*/api/v1/admin/developers/apps/:id/suspend', async ({ request, params }) => {
        sent.push(['suspend', params.id, await readJson(request)]);
        rows = [
          app({
            release_state: 'suspended',
            suspended_reason: 'sends spam',
            suspended_by: { id: '4', username: 'rhea' },
            suspended_at: '2026-11-02T12:00:00Z',
          }),
        ];
        return HttpResponse.json({});
      }),
      http.post('*/api/v1/admin/developers/apps/:id/unsuspend', async ({ request, params }) => {
        sent.push(['unsuspend', params.id, await readJson(request)]);
        rows = [app()];
        return HttpResponse.json({});
      }),
    );
    const user = await openApplications();
    await user.click(await screen.findByRole('button', { name: 'Suspend Helper Bot' }));
    const dialog = await screen.findByRole('dialog', { name: 'Suspend Helper Bot' });
    const confirm = within(dialog).getByRole('button', { name: 'Suspend application' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Reason (1 to 500 characters)'), 'sends spam');
    await user.click(confirm);
    await waitFor(() => expect(sent).toEqual([['suspend', '900', { reason: 'sends spam' }]]));
    expect(await screen.findByText('Helper Bot was suspended.')).toBeInTheDocument();
    expect(await screen.findByText('Reason: sends spam')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Unsuspend Helper Bot' }));
    const undo = await screen.findByRole('dialog', { name: 'Unsuspend Helper Bot' });
    await user.click(within(undo).getByRole('button', { name: 'Unsuspend application' }));
    await waitFor(() => expect(sent[1]).toEqual(['unsuspend', '900', { reason: null }]));
    expect(await screen.findByRole('button', { name: 'Suspend Helper Bot' })).toBeInTheDocument();
  });

  it('keeps the dialog open and shows the error when the server refuses', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS'] });
    server.use(
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/activities', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/apps', () => HttpResponse.json([app()])),
      http.post('*/api/v1/admin/developers/apps/:id/suspend', () =>
        HttpResponse.json({ code: 40001, message: 'Already suspended.' }, { status: 409 }),
      ),
    );
    const user = await openApplications();
    await user.click(await screen.findByRole('button', { name: 'Suspend Helper Bot' }));
    const dialog = await screen.findByRole('dialog', { name: 'Suspend Helper Bot' });
    await user.type(within(dialog).getByLabelText('Reason (1 to 500 characters)'), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Suspend application' }));
    expect(await within(dialog).findByText(/Already suspended/)).toBeInTheDocument();
    expect(screen.queryByText('Helper Bot was suspended.')).toBeNull();
  });

  it('is hidden from staff without REVIEW_APPS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    server.use(http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])));
    openAdmin('review');
    expect(await screen.findByRole('tab', { name: 'Packages' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'All applications' })).toBeNull();
  });
});
