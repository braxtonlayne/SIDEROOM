// XC-VIS-003 G5: with an Activity running in the channel, the composer's "+" menu joins it instead of opening a
// shelf whose only answer would be "already running".
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { useSaved } from '../shell/savedMessages';
import { at, baseHandlers, calls, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const instance = {
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: 'g1',
  channel_id: 'c1',
  channel_type: 'text',
  development: false,
  started_by: '2',
  created_at_ms: 0,
  participants: [{ user_id: '2', joined_at_ms: 1 }],
};

describe('Composer + menu with a running Activity (G5)', () => {
  it('reads "Join Chess" and joins the running instance', async () => {
    baseHandlers();
    useSaved.getState().reset();
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
    server.use(
      http.get('*/api/v1/activities', () => HttpResponse.json({ enabled: true, items: [] })),
      http.get('*/api/v1/channels/c1/activity', () =>
        HttpResponse.json({ enabled: true, instance }),
      ),
      http.post('*/api/v1/activity-instances/9/join', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(instance);
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add to message' }));
    expect(screen.queryByRole('menuitem', { name: 'Start an Activity' })).not.toBeInTheDocument();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Join Chess' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /activity-instances/9/join')).toBe(true),
    );
    expect(screen.queryByRole('dialog', { name: /Activities/ })).not.toBeInTheDocument();
  });
});
