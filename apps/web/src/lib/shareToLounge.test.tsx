// "Share to Lounge" channel picker (D-226): only channels where the person can send and use library embeds are offered.
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ToastProvider } from '@sideroom/ui';
import { ShareToLoungeDialog } from './ShareToLounge';
import { resetStore, useStore } from '../store';
import { setPermEngine } from '../lounge/permEngine';
import { PERM } from '../lounge/liveMap';

const target = { url: 'https://x.test/l/1', card: '<library:1>', title: 'Series', kind: 'Library' };

const channel = (id: string, deny = '0') =>
  ({
    id,
    type: 0,
    name: id,
    guild_id: 'g1',
    permission_overwrites: [{ id: 'g1', type: 0, allow: '0', deny }],
  }) as never;

function seed(permissions: bigint) {
  useStore.setState({
    currentUserId: 'me',
    guilds: { g1: { id: 'g1', name: 'Club', owner_id: 'boss' } as never },
    guildOrder: ['g1'],
    roles: { g1: { id: 'g1', name: '@everyone', permissions: permissions.toString() } as never },
    roleIdsByGuild: { g1: ['g1'] },
    members: { 'g1:me': { user: { id: 'me' }, roles: [] } as never },
    channels: {
      open: channel('open'),
      muted: channel('muted', PERM.SEND_MESSAGES.toString()),
      noembed: channel('noembed', PERM.USE_LIBRARY_EMBEDS.toString()),
    },
    channelIdsByGuild: { g1: ['open', 'muted', 'noembed'] },
  });
}

beforeEach(() => {
  resetStore();
  setPermEngine(null);
});
afterEach(() => setPermEngine(null));

describe('Share to Lounge channel picker', () => {
  it('hides channels where sending or library embeds are denied (overwrites applied by the shared engine)', async () => {
    seed(PERM.SEND_MESSAGES | PERM.USE_LIBRARY_EMBEDS | PERM.VIEW_CHANNEL);
    setPermEngine({
      compute: (input) => {
        const deny = BigInt(input.overwrites[0]?.deny ?? '0');
        return ((BigInt(input.everyone) & ~deny) | 0n).toString();
      },
    });
    const user = userEvent.setup();
    render(
      <ToastProvider>
        <ShareToLoungeDialog open onClose={() => undefined} target={target} />
      </ToastProvider>,
    );
    const dialog = await screen.findByRole('dialog', { name: 'Share to Lounge' });
    // Pick the server in the first dropdown, then open the channel list.
    await user.click(within(dialog).getByRole('combobox', { name: 'Where' }));
    await user.click(await screen.findByRole('option', { name: 'Club' }));
    await user.click(within(dialog).getByRole('combobox', { name: 'Channel' }));
    expect(await screen.findByRole('option', { name: '# open' })).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: '# muted' })).not.toBeInTheDocument();
    expect(screen.queryByRole('option', { name: '# noembed' })).not.toBeInTheDocument();
  });

  it('offers nothing in a server where the person cannot send at all', async () => {
    seed(PERM.VIEW_CHANNEL);
    const user = userEvent.setup();
    render(
      <ToastProvider>
        <ShareToLoungeDialog open onClose={() => undefined} target={target} />
      </ToastProvider>,
    );
    const dialog = await screen.findByRole('dialog', { name: 'Share to Lounge' });
    await user.click(within(dialog).getByRole('combobox', { name: 'Where' }));
    await user.click(await screen.findByRole('option', { name: 'Club' }));
    expect(within(dialog).getByRole('combobox', { name: 'Channel' })).toHaveTextContent(
      /no channels/i,
    );
  });
});
