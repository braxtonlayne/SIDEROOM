import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { HttpResponse, http, server } from '../../test/server';
import { ReportDialog } from '../ReportDialog';
import { closeReport, openReport, setReportsEnabled } from '../reports';

afterEach(() => {
  closeReport();
  setReportsEnabled(false);
});

async function choose(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement, name: string) {
  await user.click(within(dialog).getByRole('combobox', { name: 'What is wrong?' }));
  await user.click(await screen.findByRole('option', { name }));
}

describe('ReportDialog (MOD-RPT-010)', () => {
  it('renders nothing until a target is chosen', () => {
    const { container } = render(<ReportDialog />);
    expect(container).toBeEmptyDOMElement();
  });

  it('sends only ids, the category and the note, then confirms', async () => {
    let sent: unknown;
    server.use(
      http.post('*/api/v1/moderation/reports', async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ id: '77', state: 'received' }, { status: 201 });
      }),
    );
    const user = userEvent.setup();
    render(<ReportDialog />);
    openReport({ kind: 'message', id: '5', channelId: '9', authorId: '3' });
    const dialog = await screen.findByRole('dialog', { name: 'Report this message' });
    // Nothing can be sent before a reason is chosen.
    expect(within(dialog).getByRole('button', { name: 'Send report' })).toBeDisabled();
    await choose(user, dialog, 'Harassment, hate or threats');
    await user.type(within(dialog).getByLabelText(/Anything else/), 'He keeps doing it');
    await user.click(within(dialog).getByLabelText(/Server moderators may see my name/));
    await user.click(within(dialog).getByRole('button', { name: 'Send report' }));
    expect(await screen.findByRole('dialog', { name: 'Report sent' })).toBeInTheDocument();
    expect(sent).toEqual({
      target_kind: 'message',
      target_id: '5',
      channel_id: '9',
      category: 'harassment',
      note: 'He keeps doing it',
      moderators_may_see_name: true,
    });
  });

  it('blocks the person too when asked, and keeps the name hidden by default', async () => {
    let sent: Record<string, unknown> = {};
    let blocked: unknown;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.post('*/api/v1/moderation/reports', async ({ request }) => {
        sent = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ id: '1', state: 'received' }, { status: 201 });
      }),
      http.put('*/api/v1/users/@me/relationships/42', async ({ request }) => {
        blocked = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    render(<ReportDialog />);
    openReport({ kind: 'user', id: '42', guildId: '8' });
    const dialog = await screen.findByRole('dialog', { name: 'Report this person' });
    await choose(user, dialog, 'Spam or scams');
    await user.click(within(dialog).getByLabelText('Also block this person'));
    await user.click(within(dialog).getByRole('button', { name: 'Send report' }));
    await screen.findByRole('dialog', { name: 'Report sent' });
    expect(sent).toMatchObject({
      target_kind: 'user',
      target_id: '42',
      guild_id: '8',
      moderators_may_see_name: false,
    });
    expect(blocked).toEqual({ type: 2 });
  });

  it('does not offer the moderator-name choice for a library item, and names the failure', async () => {
    server.use(
      http.post('*/api/v1/moderation/reports', () =>
        HttpResponse.json({ code: 900010, message: 'conflict' }, { status: 409 }),
      ),
    );
    const user = userEvent.setup();
    render(<ReportDialog />);
    openReport({ kind: 'library', id: '12' });
    const dialog = await screen.findByRole('dialog', { name: 'Report this series' });
    expect(within(dialog).queryByLabelText(/Server moderators may see my name/)).toBeNull();
    expect(within(dialog).queryByLabelText('Also block this person')).toBeNull();
    await choose(user, dialog, 'Something else');
    await user.click(within(dialog).getByRole('button', { name: 'Send report' }));
    expect(
      await within(dialog).findByText('You already have an open report about this.'),
    ).toBeInTheDocument();
    // Still open so the person can read it.
    expect(screen.getByRole('dialog', { name: 'Report this series' })).toBeInTheDocument();
  });

  it('says so when reporting is switched off or the daily limit is reached', async () => {
    const user = userEvent.setup();
    render(<ReportDialog />);
    openReport({ kind: 'cinema', id: '4' });
    let dialog = await screen.findByRole('dialog', { name: 'Report this title' });
    await choose(user, dialog, 'Illegal content');
    server.use(
      http.post('*/api/v1/moderation/reports', () =>
        HttpResponse.json({ code: 900001, message: 'off' }, { status: 403 }),
      ),
    );
    await user.click(within(dialog).getByRole('button', { name: 'Send report' }));
    expect(await within(dialog).findByText('Reporting is not turned on here.')).toBeInTheDocument();
    server.use(
      http.post('*/api/v1/moderation/reports', () =>
        HttpResponse.json({ code: 40062, message: 'slow' }, { status: 429 }),
      ),
    );
    dialog = screen.getByRole('dialog', { name: 'Report this title' });
    await user.click(within(dialog).getByRole('button', { name: 'Send report' }));
    expect(
      await within(dialog).findByText('You have sent too many reports today. Try again tomorrow.'),
    ).toBeInTheDocument();
  });
});
