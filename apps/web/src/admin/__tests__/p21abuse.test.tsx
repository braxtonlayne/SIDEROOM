// RDY-SEC-006: Admin > Limits shows the abuse limits as their own group, with the default beside each one,
// and saves an edit as a plain settings patch (the server applies it on the next request).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const SLOW = { timeout: 10_000 };

const abuse = (key: string, description: string, max: number, value: number) => ({
  key,
  group: 'abuse',
  kind: 'integer',
  description,
  min: 1,
  max,
  value,
  default: value,
  is_default: true,
  updated_at_ms: null,
});

describe('Admin > Limits: abuse limits', () => {
  it('lists the group, shows ranges and defaults, and saves a changed limit with a reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    let sent: unknown;
    const list = [
      abuse(
        'abuse.friend_requests_per_hour',
        'Friend requests one member may send per hour.',
        1000,
        60,
      ),
      abuse('abuse.dm_opens_per_hour', 'Conversations one member may open per hour.', 1000, 40),
    ];
    server.use(
      http.get('*/api/v1/admin/settings', () => HttpResponse.json(list)),
      http.patch('*/api/v1/admin/settings', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(list);
      }),
    );
    openAdmin('limits');
    const user = userEvent.setup();
    expect(await screen.findByRole('heading', { name: 'Abuse limits' }, SLOW)).toBeInTheDocument();
    const friends = await screen.findByLabelText('abuse.friend_requests_per_hour');
    expect(friends).toHaveValue(60);
    expect(screen.getByLabelText('abuse.dm_opens_per_hour')).toHaveValue(40);
    await user.clear(friends);
    await user.type(friends, '5000');
    expect(await screen.findByText('Must be between 1 and 1000.')).toBeInTheDocument();
    await user.clear(friends);
    await user.type(friends, '30');
    await user.click(screen.getByRole('button', { name: 'Save 1 change' }));
    const dialog = await screen.findByRole('dialog', { name: 'Save 1 setting' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'tighter after a spam wave');
    await user.click(within(dialog).getByRole('button', { name: 'Save settings' }));
    await waitFor(() =>
      expect(sent).toEqual({
        values: { 'abuse.friend_requests_per_hour': 30 },
        reason: 'tighter after a spam wave',
      }),
    );
  });
});
