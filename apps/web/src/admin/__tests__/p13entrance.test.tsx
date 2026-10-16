// ADM-PLT-003 / ENT-UI-025: the Entrance copy editor.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';
import { setEntranceCopy } from '../../routes/entrance/entranceCopy';

beforeEach(() => setEntranceCopy(null));

describe('Entrance pages editor', () => {
  it('saves a block after a reason, with the second-factor retry, and resets another', async () => {
    asStaff({ perms: ['MANAGE_ENTRANCE'] });
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
    asStaff({ perms: ['MANAGE_ENTRANCE'] });
    server.use(http.get('*/api/v1/sr/entrance/copy', () => HttpResponse.json({})));
    openAdmin('entrance');
    const box = (await screen.findAllByLabelText('Markdown', {}, { timeout: 10_000 }))[0]!;
    // Setting the value directly: typing 16 KiB character by character is slow.
    const { fireEvent } = await import('@testing-library/react');
    fireEvent.change(box, { target: { value: 'x'.repeat(16 * 1024 + 1) } });
    expect(await screen.findByText('Too long: the limit is 16 KiB.')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Save' })[0]).toBeDisabled();
  });

  it('is not offered to staff who hold only MANAGE_FEATURES (D-239)', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    server.use(http.get('*/api/v1/admin/features', () => HttpResponse.json([])));
    openAdmin('entrance');
    // The Features section (also MANAGE_FEATURES) is offered; the Entrance section is not.
    await waitFor(() => expect(within(nav()).getAllByRole('button').length).toBeGreaterThan(0), {
      timeout: 10_000,
    });
    const names = within(nav())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(names).toContain('Features');
    expect(names).not.toContain('Entrance pages');
  });
});

describe('Entrance hero image (H-41)', () => {
  it('uploads a picture after a reason, then removes it', async () => {
    asStaff({ perms: ['MANAGE_ENTRANCE'] });
    let has = false;
    let body: { image?: string; reason?: string } | undefined;
    let deleted = 0;
    server.use(
      http.get('*/api/v1/sr/entrance/hero', () =>
        has
          ? new HttpResponse(new Uint8Array([137, 80, 78, 71]), {
              headers: { 'content-type': 'image/png' },
            })
          : new HttpResponse(null, { status: 404 }),
      ),
      http.put('*/api/v1/admin/entrance/hero', async ({ request }) => {
        body = (await readJson(request)) as typeof body;
        has = true;
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete('*/api/v1/admin/entrance/hero', () => {
        deleted += 1;
        has = false;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openAdmin('entrance');
    expect(await screen.findByText(/No image set/, {}, { timeout: 10_000 })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove image' })).toBeDisabled();

    const file = new File([new Uint8Array([137, 80, 78, 71])], 'door.png', { type: 'image/png' });
    await user.upload(screen.getByLabelText('Choose an image'), file);
    await user.click(screen.getByRole('button', { name: 'Upload image' }));
    const dialog = await screen.findByRole('dialog', { name: 'Replace the hero image?' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'new season');
    await user.click(within(dialog).getByRole('button', { name: 'Upload image' }));
    expect(await screen.findByText('Hero image saved.')).toBeInTheDocument();
    expect(body?.reason).toBe('new season');
    expect(body?.image).toMatch(/^data:image\/png;base64,/);
    expect(await screen.findByAltText('The current hero image')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Remove image' }));
    const confirm = await screen.findByRole('dialog', { name: 'Remove the hero image?' });
    await user.click(within(confirm).getByRole('button', { name: 'Remove image' }));
    await waitFor(() => expect(deleted).toBe(1));
    expect(await screen.findByText('Hero image removed.')).toBeInTheDocument();
  });

  it('refuses a file over 8 MiB before sending it', async () => {
    asStaff({ perms: ['MANAGE_ENTRANCE'] });
    openAdmin('entrance');
    const user = userEvent.setup({ applyAccept: false });
    const big = new File([new Uint8Array(8 * 1024 * 1024 + 1)], 'big.png', { type: 'image/png' });
    await user.upload(
      await screen.findByLabelText('Choose an image', {}, { timeout: 10_000 }),
      big,
    );
    expect(await screen.findByText('That file is over 8 MiB.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Upload image' })).toBeDisabled();
  });
});
