// BYOS-LEG-001: Admin > Storage lists every mount with its owner and status, delists and relists a whole bucket
// with a reason, never offers the instance bucket for delisting, and is hidden without MANAGE_TAKEDOWNS.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const mount = (over: Record<string, unknown> = {}) => ({
  id: '700',
  owner_kind: 'user',
  owner_id: '100',
  owner_name: 'alice',
  label: 'Home shelf',
  provider: 'garage',
  bucket: 'shelf',
  status: 'active',
  key_hint: 'abcd',
  created_at_ms: 1_700_000_000_000,
  delisted_at_ms: null,
  delist_reason: '',
  delisted_objects: 0,
  ...over,
});

describe('Admin storage', () => {
  it('lists mounts, offers no delist for the instance bucket and shows delisted objects', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    server.use(
      http.get('*/api/v1/admin/storage/mounts', () =>
        HttpResponse.json([
          mount({ delisted_objects: 2 }),
          mount({
            id: '1',
            owner_kind: 'platform',
            owner_id: '0',
            owner_name: '',
            label: 'Instance',
          }),
        ]),
      ),
    );
    openAdmin('storage');
    const list = await screen.findByRole('list', { name: 'Storage buckets' });
    const [home, instance] = within(list).getAllByRole('listitem');
    expect(within(home!).getByText('2 delisted objects')).toBeInTheDocument();
    expect(within(home!).getByText(/Member alice/)).toBeInTheDocument();
    expect(within(home!).getByRole('button', { name: 'Delist Home shelf' })).toBeInTheDocument();
    expect(within(instance!).queryByRole('button')).toBeNull();
  });

  it('delists with a reason and relists', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let current = mount();
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/storage/mounts', () => HttpResponse.json([current])),
      http.post('*/api/v1/admin/storage/mounts/700/delist', async ({ request }) => {
        calls.push(['delist', await readJson(request)]);
        current = mount({
          status: 'delisted',
          delisted_at_ms: 1_700_000_100_000,
          delist_reason: 'Notice TK-1',
        });
        return HttpResponse.json({ id: '700', status: 'delisted' });
      }),
      http.post('*/api/v1/admin/storage/mounts/700/relist', async ({ request }) => {
        calls.push(['relist', await readJson(request)]);
        current = mount();
        return HttpResponse.json({ id: '700', status: 'active' });
      }),
    );
    openAdmin('storage');
    await userEvent.click(await screen.findByRole('button', { name: 'Delist Home shelf' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delist this bucket?' });
    await userEvent.type(within(dialog).getByRole('textbox'), 'Notice TK-1');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delist' }));
    expect(await screen.findByText(/Delisted Home shelf/)).toBeInTheDocument();
    expect(await screen.findByText(/Delisted .*: Notice TK-1/)).toBeInTheDocument();

    await userEvent.click(await screen.findByRole('button', { name: 'Relist Home shelf' }));
    const back = await screen.findByRole('dialog', { name: 'Relist this bucket?' });
    await userEvent.type(within(back).getByRole('textbox'), 'Counter-notice');
    await userEvent.click(within(back).getByRole('button', { name: 'Relist' }));
    expect(await screen.findByText('Relisted Home shelf.')).toBeInTheDocument();
    expect(calls).toEqual([
      ['delist', { reason: 'Notice TK-1' }],
      ['relist', { reason: 'Counter-notice' }],
    ]);
  });

  it('is hidden without MANAGE_TAKEDOWNS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json({})),
      http.get('*/api/v1/admin/system', () => HttpResponse.json({})),
    );
    openAdmin('dashboard');
    await within(await screen.findByRole('navigation', { name: /settings|admin/i })).findByText(
      'Accounts',
    );
    expect(within(nav()).queryByText('Storage')).toBeNull();
  });
});
