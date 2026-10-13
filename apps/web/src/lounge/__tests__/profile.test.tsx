import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { usePopout } from '../profile/popoutStore';
import { resetProfiles } from '../profile/useProfile';
import { at, baseHandlers, bob, calls, me, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const profile = {
  user: { id: '2', username: 'bob', global_name: 'Bob', avatar: null, banner: null },
  user_profile: { bio: 'I like trains.', pronouns: 'he/him', banner: null, accent_color: 0x336699 },
  connected_accounts: [{ id: 'c1', name: 'bobby', type: 'github', verified: true }],
  friends_since: '2025-02-03T00:00:00Z',
  mutual_friends: [{ id: '5', username: 'dana', global_name: 'Dana' }],
  mutual_friends_count: 1,
  mutual_guilds: [{ id: 'g1', nick: 'Bobby', member_since: '2025-01-10T12:00:00Z' }],
};

const roles = [
  { id: 'r1', guild_id: 'g1', name: 'Mods', color: 0xff0000, position: 2, permissions: '0' },
  { id: 'r2', guild_id: 'g1', name: 'Readers', color: 0, position: 1, permissions: '0' },
];

function setup() {
  baseHandlers();
  resetProfiles();
  usePopout.setState({ profile: null, full: null });
  seedLounge({
    messages: [msg('100', { content: 'hello from bob' })],
    roles,
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: ['r1'] },
    },
  });
  server.use(
    http.get('*/api/v1/users/2/profile', () => HttpResponse.json(profile)),
    http.get('*/api/v1/users/@me/notes/2', () => HttpResponse.json({ note: 'owes me a coffee' })),
  );
}

describe('profile popout host', () => {
  beforeEach(setup);

  it('opens from a message author, shows the profile and closes on Escape', async () => {
    const { container } = at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const nameBtn = await within(log).findByRole('button', { name: 'Bob' });
    await userEvent.click(nameBtn);
    const pop = await screen.findByRole('dialog', { name: 'User profile' });
    expect(await within(pop).findByText('I like trains.')).toBeInTheDocument();
    expect(within(pop).getByRole('heading', { name: 'Bob' })).toBeInTheDocument();
    expect(within(pop).getByText(/he\/him/)).toBeInTheDocument();
    expect(within(pop).getByText('Mods')).toBeInTheDocument();
    expect(await within(pop).findByText(/Jan 10, 2025/)).toBeInTheDocument();
    expect(await within(pop).findByDisplayValue('owes me a coffee')).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    fireEvent.keyDown(document.body, { key: 'Escape' });
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'User profile' })).not.toBeInTheDocument(),
    );
    expect(nameBtn).toHaveFocus();
  });

  it('closes on a click elsewhere and toggles from the same trigger', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const avatarBtn = await within(log).findByRole('button', { name: 'Open profile of Bob' });
    await userEvent.click(avatarBtn);
    await screen.findByRole('dialog', { name: 'User profile' });
    await userEvent.click(avatarBtn);
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'User profile' })).toBeNull());
    await userEvent.click(avatarBtn);
    await screen.findByRole('dialog', { name: 'User profile' });
    await userEvent.click(log);
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'User profile' })).toBeNull());
  });

  it('lets a manager add and remove roles', async () => {
    server.use(
      http.delete('*/api/v1/guilds/g1/members/2/roles/r1', async (ctx) => {
        await record('DELETE')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
      http.put('*/api/v1/guilds/g1/members/2/roles/r2', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await userEvent.click(await within(log).findByRole('button', { name: 'Bob' }));
    const pop = await screen.findByRole('dialog', { name: 'User profile' });
    await userEvent.click(await within(pop).findByRole('button', { name: 'Add role' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Readers' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'PUT /guilds/g1/members/2/roles/r2')).toBe(true),
    );
    await waitFor(() => expect(useStore.getState().members['g1:2']?.roles).toContain('r2'));
    await userEvent.click(within(pop).getByRole('button', { name: 'Remove role Mods' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'DELETE /guilds/g1/members/2/roles/r1')).toBe(true),
    );
    await waitFor(() => expect(useStore.getState().members['g1:2']?.roles).not.toContain('r1'));
  });

  it('saves the note on blur and sends a direct message from the box', async () => {
    server.use(
      http.put('*/api/v1/users/@me/notes/2', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
      http.post('*/api/v1/users/@me/channels', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ id: '900', type: 1, recipients: [bob] });
      }),
      http.post('*/api/v1/channels/900/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ ...msg('901', { author: me }), channel_id: '900' });
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await userEvent.click(await within(log).findByRole('button', { name: 'Bob' }));
    const pop = await screen.findByRole('dialog', { name: 'User profile' });
    const note = await within(pop).findByDisplayValue('owes me a coffee');
    await userEvent.clear(note);
    await userEvent.type(note, 'paid back');
    await userEvent.tab();
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PUT /users/@me/notes/2')?.body).toEqual({
        note: 'paid back',
      }),
    );
    await userEvent.type(within(pop).getByRole('textbox', { name: 'Message @Bob' }), 'hey{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/900/messages')).toBe(true),
    );
    expect(calls.find((c) => c.key === 'POST /channels/900/messages')!.body).toMatchObject({
      content: 'hey',
    });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'User profile' })).toBeNull());
  });

  it('shows mutual servers and friends in the full profile', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await userEvent.click(await within(log).findByRole('button', { name: 'Bob' }));
    const pop = await screen.findByRole('dialog', { name: 'User profile' });
    await userEvent.click(within(pop).getByRole('button', { name: 'View Full Profile' }));
    const modal = await screen.findByRole('dialog', { name: 'Bob' });
    expect(await within(modal).findByText('Mutual servers — 1')).toBeInTheDocument();
    expect(within(modal).getByText('Store Guild')).toBeInTheDocument();
    expect(within(modal).getByText('Dana')).toBeInTheDocument();
    expect(within(modal).getByText(/github/)).toBeInTheDocument();
    expect(await axeViolations(modal)).toEqual([]);
  });

  it('opens from the member list, and Transfer Ownership is offered to the owner only', async () => {
    server.use(
      http.patch('*/api/v1/guilds/g1', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json({ id: 'g1', owner_id: '2' });
      }),
    );
    at('/channels/g1/c1');
    const row = await screen.findByRole('button', { name: /^Bob, / });
    fireEvent.contextMenu(row);
    expect(await screen.findByRole('menuitem', { name: 'Profile' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    );
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Transfer Ownership' }));
    const confirm = await screen.findByRole('dialog', { name: 'Transfer Ownership' });
    expect(confirm).toHaveTextContent(/owner of Store Guild/);
    await userEvent.click(within(confirm).getByRole('button', { name: 'Transfer Ownership' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /guilds/g1')?.body).toEqual({ owner_id: '2' }),
    );
    await waitFor(() => expect(useStore.getState().guilds['g1']?.owner_id).toBe('2'));
    // Now Bob owns it: the old owner no longer sees the entry.
    fireEvent.contextMenu(row);
    await screen.findByRole('menuitem', { name: 'Profile' });
    expect(screen.queryByRole('menuitem', { name: 'Transfer Ownership' })).not.toBeInTheDocument();
  });

  it('opens the popout from the member list row', async () => {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /^Bob, / }));
    expect(await screen.findByRole('dialog', { name: 'User profile' })).toBeInTheDocument();
  });
});
