// P19 follow-up (MOD-AGE-001): staff age buttons on the series page. The route is POST /moderation/age/restrictions.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { AP, type AdminPerm } from '../../admin/permissions';
import { http, HttpResponse, server } from '../../test/server';
import { renderAt, useLibraryApi } from './harness';

const SLOW = { timeout: 10_000 };

function staff(...perms: AdminPerm[]) {
  const bits = perms.reduce((a, n) => a | (1n << BigInt(AP[n])), 0n).toString();
  server.use(
    http.get('*/api/v1/admin/me', () =>
      HttpResponse.json({ is_owner: false, platform_permissions: bits, user_id: '1' }),
    ),
  );
}

describe('age buttons on the series page', () => {
  it('stay hidden from members and from staff without a matching bit', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/admin/me', () => HttpResponse.json({ message: 'no' }, { status: 403 })),
    );
    renderAt('/library/s/1');
    await screen.findByRole('heading', { name: 'Frieren' }, SLOW);
    expect(screen.queryByRole('button', { name: 'Mark 18+' })).not.toBeInTheDocument();
  });

  it('are not offered to a cinema reviewer for a library series', async () => {
    useLibraryApi();
    staff('REVIEW_MEDIA');
    renderAt('/library/s/1');
    await screen.findByRole('heading', { name: 'Frieren' }, SLOW);
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.queryByRole('button', { name: 'Mark 18+' })).not.toBeInTheDocument();
  });

  it('mark a series with a required reason and refetch it', async () => {
    useLibraryApi();
    staff('REVIEW_READING');
    let body: unknown;
    server.use(
      http.post('*/api/v1/moderation/age/restrictions', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          kind: 'library_series',
          id: '1',
          label: 'Frieren',
          restricted: true,
          changed: true,
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('button', { name: 'Mark 18+' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: 'Mark this item 18+' });
    const confirm = within(dialog).getByRole('button', { name: 'Mark 18+' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Reason/), 'Graphic violence');
    await user.click(confirm);
    await waitFor(() =>
      expect(body).toEqual({
        kind: 'library_series',
        id: '1',
        restricted: true,
        reason: 'Graphic violence',
      }),
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('offer to remove the mark on a restricted series and name a refusal', async () => {
    useLibraryApi();
    staff('MANAGE_CASES');
    server.use(
      http.post('*/api/v1/moderation/age/restrictions', () =>
        HttpResponse.json({ code: 1, message: 'nope' }, { status: 403 }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/s/2');
    await user.click(await screen.findByRole('button', { name: 'Remove 18+ mark' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: 'Remove the 18+ mark' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'Wrongly marked');
    await user.click(within(dialog).getByRole('button', { name: 'Remove mark' }));
    expect(
      await within(dialog).findByText('You do not have the permission to change this mark.'),
    ).toBeInTheDocument();
  });
});
