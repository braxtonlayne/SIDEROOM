import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { byosMockHandlers, mockMount } from '../../user/storage/mocks';
import { MANAGE, openSettings, seed, server } from './helpers';

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
