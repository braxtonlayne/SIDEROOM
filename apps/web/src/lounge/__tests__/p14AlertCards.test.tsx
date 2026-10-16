// AutoMod alert cards and the join-rate alert (J-02, J-03, D-243, D-244): buttons with `sr:automod:*` and
// `sr:incident:*` ids go to the server's own press route, which checks the presser when pressed.
import { ToastProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { ComponentRows } from '../msg/Components';
import { isSystemMessage, SYSTEM_AUTOMOD_ACTION } from '../msg/SystemMessage';
import { parseStaffButton } from '../msg/staffActions';

const alertRows = [
  {
    type: 1,
    components: [
      { type: 2, style: 4, label: 'Delete message', custom_id: 'sr:automod:delete:900' },
      { type: 2, style: 4, label: 'Kick', custom_id: 'sr:automod:kick:900' },
      { type: 2, style: 4, label: 'Ban', custom_id: 'sr:automod:ban:900' },
      { type: 2, style: 2, label: 'Mark resolved', custom_id: 'sr:automod:resolve:900' },
    ],
  },
  {
    type: 1,
    components: [
      {
        type: 3,
        custom_id: 'sr:automod:timeout:900',
        placeholder: 'Timeout',
        options: [
          { label: '1 minute', value: '60' },
          { label: '1 hour', value: '3600' },
        ],
      },
    ],
  },
];

const view = (rows: typeof alertRows) =>
  render(
    <ToastProvider>
      <ComponentRows rows={rows} guildId="g1" context={{ channelId: '500', messageId: '900' }} />
    </ToastProvider>,
  );

describe('alert button ids', () => {
  it('are recognised as alert presses, not staff card actions', () => {
    expect(parseStaffButton('sr:automod:ban:900')).toEqual({
      kind: 'alert',
      customId: 'sr:automod:ban:900',
    });
    expect(parseStaffButton('sr:incident:resume-invites:5')).toEqual({
      kind: 'alert',
      customId: 'sr:incident:resume-invites:5',
    });
    // Anything else under the prefix stays an ordinary card action.
    expect(parseStaffButton('sr:automod:nuke:900')).toEqual({
      kind: 'card',
      customId: 'sr:automod:nuke:900',
    });
  });

  it('render as messages (type 24), not as one-line system notices', () => {
    expect(isSystemMessage({ type: SYSTEM_AUTOMOD_ACTION })).toBe(false);
    expect(isSystemMessage({ type: 7 })).toBe(true);
  });
});

describe('alert card buttons', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
  });

  it('presses a button through the guild route and keeps the other buttons usable', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post('*/api/v1/guilds/g1/alert-actions', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({ ok: true, message: 'Done.' });
      }),
    );
    view(alertRows);
    await userEvent.click(screen.getByRole('button', { name: 'Delete message' }));
    await waitFor(() =>
      expect(bodies[0]).toEqual({
        channel_id: '500',
        message_id: '900',
        custom_id: 'sr:automod:delete:900',
      }),
    );
    expect(await screen.findByText('Done.')).toBeInTheDocument();
    // The server rewrites the card; until then the other buttons stay pressable.
    expect(screen.getByRole('button', { name: 'Kick' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Mark resolved' }));
    await waitFor(() => expect(bodies).toHaveLength(2));
  });

  it('sends the chosen timeout from the select', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/guilds/g1/alert-actions', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ ok: true, message: 'Done.' });
      }),
    );
    view(alertRows);
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Timeout' }), '3600');
    await waitFor(() =>
      expect(body).toEqual({
        channel_id: '500',
        message_id: '900',
        custom_id: 'sr:automod:timeout:900',
        values: ['3600'],
      }),
    );
  });

  it('shows the server text when the presser lacks the permission', async () => {
    server.use(
      http.post('*/api/v1/guilds/g1/alert-actions', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    view(alertRows);
    await userEvent.click(screen.getByRole('button', { name: 'Ban' }));
    expect(await screen.findByText('Missing Permissions')).toBeInTheDocument();
  });

  it('resumes invites from the join-rate alert', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/guilds/123/alert-actions', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ ok: true, message: 'Invites resumed.' });
      }),
    );
    render(
      <ToastProvider>
        <ComponentRows
          rows={[
            {
              type: 1,
              components: [
                {
                  type: 2,
                  style: 2,
                  label: 'Resume invites',
                  custom_id: 'sr:incident:resume-invites:123',
                },
              ],
            },
          ]}
          guildId="123"
          context={{ channelId: '500', messageId: '901' }}
        />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Resume invites' }));
    await waitFor(() =>
      expect(body).toEqual({
        channel_id: '500',
        message_id: '901',
        custom_id: 'sr:incident:resume-invites:123',
      }),
    );
    expect(await screen.findByText('Invites resumed.')).toBeInTheDocument();
  });
});
