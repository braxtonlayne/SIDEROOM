// Tester invitation buttons on the system DM (G-01, D-204).
import { ToastProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { ComponentRows } from '../msg/Components';
import { parseStaffButton } from '../msg/staffActions';

const rows = [
  {
    type: 1,
    components: [
      { type: 2, style: 3, label: 'Accept', custom_id: 'sr:tester-accept:31' },
      { type: 2, style: 4, label: 'Decline', custom_id: 'sr:tester-decline:31' },
    ],
  },
];

describe('tester invitation buttons', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
  });

  it('parses the accept and decline ids', () => {
    expect(parseStaffButton('sr:tester-accept:31')).toEqual({
      kind: 'tester',
      inviteId: '31',
      accept: true,
    });
    expect(parseStaffButton('sr:tester-decline:9')).toEqual({
      kind: 'tester',
      inviteId: '9',
      accept: false,
    });
  });

  it('answers through the developer tester route and then disables both buttons', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/developers/tester-invites/31/respond', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ application_id: '5', name: 'Jukebox', accepted: true });
      }),
    );
    render(
      <ToastProvider>
        <ComponentRows rows={rows} context={{ channelId: '500', messageId: '900' }} />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(body).toEqual({ accept: true }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Decline' })).toBeDisabled());
  });
});
