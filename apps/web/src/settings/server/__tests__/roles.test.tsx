import { renderHook, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useViewer } from '../hooks';
import { roleEditReason } from '../sections/Roles';
import {
  bits,
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  restRole,
  restRoles,
  seed,
  server,
} from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const rolesHandler = () =>
  server.use(http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())));

describe('Roles', () => {
  it('lists roles highest first with @everyone as Default permissions', async () => {
    seed({ perms: [MANAGE.roles] });
    rolesHandler();
    await openSettings('roles');
    const list = await screen.findByRole('list', { name: 'Roles' });
    const names = within(list)
      .getAllByRole('button')
      .filter((b) => !/^Move/.test(b.getAttribute('aria-label') ?? ''))
      .map((b) => b.textContent);
    expect(names).toEqual(['Admin', 'Viewer', 'Mod', 'Member', 'Default permissions']);
  });

  it('hierarchy: roles at or above the viewer are read-only and say why', async () => {
    seed({ perms: [MANAGE.roles, MANAGE.ban], position: 7 });
    rolesHandler();
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Admin' }));
    expect(await screen.findByText(/at or above your highest role/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Permissions' }));
    expect(screen.getAllByRole('switch', { name: /Ban Members/ })[0]).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Delete role Admin' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move Admin up' })).toBeDisabled();
    // a lower role is editable
    await userEvent.click(screen.getByRole('button', { name: 'Member' }));
    await userEvent.click(screen.getByRole('tab', { name: 'Permissions' }));
    expect(screen.getAllByRole('switch', { name: /Ban Members/ })[0]).toBeEnabled();
    // but the viewer cannot grant a permission they lack
    expect(screen.getAllByRole('switch', { name: /Kick Members/ })[0]).toBeDisabled();
  }, 30000);

  it('edits and saves only the changed fields', async () => {
    seed({ perms: [MANAGE.roles, MANAGE.ban], position: 7 });
    rolesHandler();
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/roles/low', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(restRole('low', 'Members', 1));
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Member' }));
    const name = await screen.findByLabelText('Role name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Members');
    await userEvent.click(screen.getByRole('tab', { name: 'Permissions' }));
    await userEvent.click(screen.getAllByRole('switch', { name: /Ban Members/ })[0]!);
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ name: 'Members', permissions: bits(2) });
  });

  it('shows the Administrator warning', async () => {
    seed({ perms: [MANAGE.admin], position: 20 });
    rolesHandler();
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Admin' }));
    await userEvent.click(screen.getByRole('tab', { name: 'Permissions' }));
    expect(screen.getByText(/bypass channel overwrites/)).toBeInTheDocument();
  });

  it('creates a role', async () => {
    seed({ perms: [MANAGE.roles] });
    const cap = capture();
    let created = false;
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () =>
        HttpResponse.json(created ? [...restRoles(), restRole('new', 'new role', 8)] : restRoles()),
      ),
      http.post('*/api/v1/guilds/g1/roles', async ({ request }) => {
        await cap.record(request);
        created = true;
        return HttpResponse.json(restRole('new', 'new role', 8));
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Create Role' }));
    expect(await screen.findByRole('heading', { name: 'new role', level: 2 })).toBeInTheDocument();
    expect(cap.calls[0]!.body).toEqual({ name: 'new role' });
  });

  it('reorders: moving swaps the position values of the affected roles', async () => {
    seed({ perms: [MANAGE.roles], position: 20 });
    rolesHandler();
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/roles', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(restRoles());
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Move Mod down' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual([
      { id: 'low', position: 5 },
      { id: 'mod', position: 1 },
    ]);
  });

  it('blocks moving a role to or above the viewer top role', async () => {
    seed({ perms: [MANAGE.roles], position: 7 });
    rolesHandler();
    await openSettings('roles');
    expect(await screen.findByRole('button', { name: 'Move Mod up' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move Mod down' })).toBeEnabled();
  });

  it('deletes a role after confirmation and surfaces failures', async () => {
    seed({ perms: [MANAGE.roles], position: 7 });
    rolesHandler();
    server.use(
      http.delete('*/api/v1/guilds/g1/roles/low', () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Member' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete role Member' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete role' }));
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });

  it('removes a role member', async () => {
    seed({ perms: [MANAGE.roles], position: 7 });
    rolesHandler();
    const cap = capture();
    server.use(
      http.delete('*/api/v1/guilds/g1/members/:uid/roles/:rid', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Member' }));
    await userEvent.click(screen.getByRole('tab', { name: 'Manage Members' }));
    const holders = screen.getByRole('list', { name: 'Members with Member' });
    expect(within(holders).getByText('bob')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove bob from Member' }));
    await waitFor(() => expect(cap.calls[0]?.method).toBe('DELETE'));
    expect(cap.calls[0]!.url).toContain('/members/2/roles/low');
    await waitFor(() => expect(screen.getByText('No members have this role.')).toBeInTheDocument());
  });
});

describe('role hierarchy helpers', () => {
  it('roleEditReason mirrors the server rules', () => {
    seed({ perms: [MANAGE.roles], position: 7 });
    const v = renderHook(() => useViewer('g1')).result.current;
    expect(v.topPosition).toBe(7);
    expect(roleEditReason(v, restRole('x', 'x', 7), 'g1')).toMatch(/at or above/);
    expect(roleEditReason(v, restRole('x', 'x', 3), 'g1')).toBeUndefined();
    expect(roleEditReason(v, restRole('g1', '@everyone', 0), 'g1')).toBeUndefined();
    expect(v.canGrant(1n << 28n)).toBe(true);
    expect(v.canGrant(1n << 2n)).toBe(false);
  });
  it('owner and administrator bypass', () => {
    seed({ perms: [], ownerId: '1' });
    let v = renderHook(() => useViewer('g1')).result.current;
    expect(v.isOwner).toBe(true);
    expect(roleEditReason(v, restRole('x', 'x', 99), 'g1')).toBeUndefined();
    seed({ perms: [MANAGE.admin] });
    v = renderHook(() => useViewer('g1')).result.current;
    expect(v.canGrant(1n << 40n)).toBe(true);
    expect(v.can('MANAGE_ROLES')).toBe(true);
  });
});
