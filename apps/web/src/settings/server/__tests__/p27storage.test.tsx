import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { byosMockHandlers, mockMount } from '../../user/storage/mocks';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { PERMISSIONS } from '../permissions';
import { http, HttpResponse, MANAGE, openSettings, restRoles, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('Server settings > Storage (BYOS-UI-001, against MSW mocks)', () => {
  it('is hidden without Manage Server', async () => {
    seed({ perms: [MANAGE.channels] });
    await openSettings('channels');
    expect(screen.queryByRole('button', { name: 'Storage' })).toBeNull();
  });

  it('lists the guild mounts and tests one through the guild routes', async () => {
    seed({ perms: [MANAGE.guild] });
    const user = userEvent.setup();
    const m = mockMount({ id: '800', owner_kind: 'guild', owner_id: 'g1', label: 'Guild Shelf' });
    server.use(...byosMockHandlers());
    await openSettings('storage');
    expect(await screen.findByText('No bucket connected yet.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Connect a bucket' }));
    await user.type(screen.getByLabelText('Name'), m.label);
    await user.type(screen.getByLabelText('Bucket'), 'b');
    await user.type(screen.getByLabelText('Access key ID'), 'k');
    await user.type(screen.getByLabelText('Secret access key'), 'secret');
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    expect(await screen.findByText('Guild Shelf')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Test connection' }));
    expect(
      await screen.findByRole('table', { name: 'Connection test results' }),
    ).toBeInTheDocument();
  });
});

describe('Role editor > Storage grants', () => {
  it('shows the six grants, saves one through the grants route, and keeps them out of the permission catalog', async () => {
    seed({ perms: [MANAGE.roles, MANAGE.guild], position: 7 });
    const user = userEvent.setup();
    let put: unknown;
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () =>
        HttpResponse.json(
          restRoles().map((r) =>
            r.name === 'Member' ? { ...r, sr_storage_grants: ['view_cinema'] } : r,
          ),
        ),
      ),
      http.put('*/api/v1/guilds/g1/storage/grants', async ({ request }) => {
        put = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('roles');
    await user.click(await screen.findByRole('button', { name: 'Member' }));
    await user.click(screen.getByRole('tab', { name: 'Permissions' }));
    expect(screen.getAllByRole('switch', { name: /storage|shelf/i })).toHaveLength(6);
    expect(screen.getByRole('switch', { name: 'View shelf movies and shows' })).toBeChecked();
    expect(screen.getByRole('switch', { name: /View shelf games/ })).not.toBeChecked();
    await user.click(screen.getByRole('switch', { name: 'Upload to the shelf' }));
    await waitFor(() => expect(put).toEqual({ role_id: 'low', grants: ['view_cinema', 'upload'] }));
    expect(PERMISSIONS.some((p) => /storage|shelf/i.test(p.id))).toBe(false);
  });
});

describe('Role editor Storage grants accessibility (axe)', () => {
  it('has no violations on the Permissions tab', async () => {
    seed({ perms: [MANAGE.roles, MANAGE.guild], position: 7 });
    const user = userEvent.setup();
    server.use(http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())));
    await openSettings('roles');
    await user.click(await screen.findByRole('button', { name: 'Member' }));
    await user.click(screen.getByRole('tab', { name: 'Permissions' }));
    await screen.findByRole('switch', { name: 'Upload to the shelf' });
    expect(await axeViolations(document.body)).toEqual([]);
  }, 30000);
});
