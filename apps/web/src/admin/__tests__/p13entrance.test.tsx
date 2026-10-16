// ADM-PLT-003 / ENT-UI-025: the Entrance copy editor.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';
import { setEntranceCopy } from '../../routes/entrance/entranceCopy';

beforeEach(() => setEntranceCopy(null));

describe('Entrance pages editor', () => {
  it('saves a block after a reason, with the second-factor retry, and resets another', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    let puts = 0;
    let body: unknown;
    let deleted = 0;
    server.use(
      http.get('*/api/v1/sr/entrance/copy', () =>
        HttpResponse.json({ 'landing-note': 'Old **note**' }),
      ),
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900, required: true }),
      ),
      http.post('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900, required: true }),
      ),
      http.put('*/api/v1/admin/entrance/copy/no-invitation', async ({ request }) => {
        puts += 1;
        body = await readJson(request);
        return puts === 1
          ? HttpResponse.json({ code: 900004, message: 're-auth' }, { status: 403 })
          : new HttpResponse(null, { status: 204 });
      }),
      http.delete('*/api/v1/admin/entrance/copy/landing-note', () => {
        deleted += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openAdmin('entrance');
    const box = (await screen.findAllByLabelText('Markdown', {}, { timeout: 10_000 }))[0]!;
    await user.type(box, 'Ask a friend.');
    await user.click(screen.getAllByRole('button', { name: 'Preview' })[0]!);
    expect(
      screen.getAllByRole('region', { name: 'Preview of the page text' })[0],
    ).toHaveTextContent('Ask a friend.');
    await user.click(screen.getAllByRole('button', { name: 'Save' })[0]!);
    const dialog = await screen.findByRole('dialog', { name: /Save No invitation page/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'clearer wording');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    const reauth = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    await user.type(within(reauth).getByLabelText(/authenticator app/), '123456');
    await user.click(within(reauth).getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(/Saved\. Visitors see it/)).toBeInTheDocument();
    expect(puts).toBe(2);
    expect(body).toEqual({ markdown: 'Ask a friend.', reason: 'clearer wording' });

    const landing = within(screen.getByRole('region', { name: 'Landing note' }));
    await user.click(landing.getByRole('button', { name: 'Reset to default' }));
    const confirm = await screen.findByRole('dialog', { name: /Reset Landing note/ });
    await user.click(within(confirm).getByRole('button', { name: 'Reset to default' }));
    await waitFor(() => expect(deleted).toBe(1));
    expect(await screen.findByText('Back to the built-in text.')).toBeInTheDocument();
  });

  it('refuses text over 16 KiB', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    server.use(http.get('*/api/v1/sr/entrance/copy', () => HttpResponse.json({})));
    openAdmin('entrance');
    const box = (await screen.findAllByLabelText('Markdown', {}, { timeout: 10_000 }))[0]!;
    // Setting the value directly: typing 16 KiB character by character is slow.
    const { fireEvent } = await import('@testing-library/react');
    fireEvent.change(box, { target: { value: 'x'.repeat(16 * 1024 + 1) } });
    expect(await screen.findByText('Too long: the limit is 16 KiB.')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Save' })[0]).toBeDisabled();
  });
});
