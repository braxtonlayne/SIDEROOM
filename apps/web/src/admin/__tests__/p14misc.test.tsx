// P14 misc admin additions: integration status in the Setup checklist and on the System page
// (ADM-DASH-003, ADM-PLT-006).
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const COUNTS = {
  accounts_active: 7,
  accounts_banned: 0,
  accounts_deactivated: 1,
  accounts_deleted: 0,
  jobs_dead: 0,
  jobs_queued: 0,
  jobs_running: 0,
  sessions_active: 4,
  staff: 1,
};

const SYSTEM = {
  counts: COUNTS,
  gateway_sessions: 2,
  probes: [{ name: 'database', ok: true }],
  version: '1',
  integrations: [
    { id: 'smtp', configured: true, detail: 'mail.example.test' },
    { id: 'discord_oauth', configured: false, detail: null },
    { id: 'voice', configured: true, detail: null },
    { id: 'turn', configured: false, detail: null },
  ],
};

describe('integrations', () => {
  it('feeds the Setup checklist from the system status', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(COUNTS)),
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.get('*/api/v1/admin/settings', () => HttpResponse.json([])),
    );
    openAdmin('dashboard');
    const setup = await screen.findByRole('list', { name: 'Setup checklist' });
    // SMTP is set up; Discord sign-in is not; voice counts only when TURN is set up too.
    const rows = within(setup).getAllByRole('listitem');
    const row = (label: RegExp) => rows.find((r) => label.test(r.textContent ?? ''))!;
    expect(within(row(/Email \(SMTP\)/)).getByText('Done')).toBeInTheDocument();
    expect(within(row(/Discord sign-in/)).getByText('To do')).toBeInTheDocument();
    expect(within(row(/Voice and TURN/)).getByText('To do')).toBeInTheDocument();
  });

  it('lists the integrations on the System page and sends a test email', async () => {
    asStaff({ perms: [], owner: true });
    let sent = 0;
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.post('*/api/v1/admin/system/email-test', () => {
        sent += 1;
        return HttpResponse.json({ sent_to: 'staff@example.test' });
      }),
    );
    openAdmin('system');
    const list = await screen.findByRole('list', { name: 'Integrations' });
    expect(within(list).getByText('mail.example.test')).toBeInTheDocument();
    expect(within(list).getAllByText('Not configured')).toHaveLength(2);
    await userEvent.click(screen.getByRole('button', { name: 'Send a test email to me' }));
    expect(await screen.findByText('Test message sent to staff@example.test.')).toBeInTheDocument();
    expect(sent).toBe(1);
  });
});
