import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const forward = (ref: Record<string, string>) =>
  msg('100', {
    content: '',
    message_reference: { message_id: '55', type: 1, ...ref },
    message_snapshots: [{ message: { content: 'the original', attachments: [], embeds: [] } }],
  });

describe('forwarded message source (LNG-MSGUI-006)', () => {
  beforeEach(() => baseHandlers());

  it('names the source channel and jumps to the original', async () => {
    seedLounge({ messages: [forward({ channel_id: 'c2', guild_id: 'g1' })] });
    const { router } = at('/channels/g1/c1');
    const frame = await screen.findByRole('group', { name: 'Forwarded message' });
    expect(within(frame).getByText('the original')).toBeInTheDocument();
    await userEvent.click(within(frame).getByRole('button', { name: '#random' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/c2/55'));
  });

  it('stays plain when the source channel is not visible to the viewer', async () => {
    seedLounge({ messages: [forward({ channel_id: '777', guild_id: 'g9' })] });
    at('/channels/g1/c1');
    const frame = await screen.findByRole('group', { name: 'Forwarded message' });
    expect(within(frame).getByText('Forwarded')).toBeInTheDocument();
    expect(within(frame).queryByRole('button')).not.toBeInTheDocument();
  });
});
