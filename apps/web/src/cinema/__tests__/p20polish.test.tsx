// P19 follow-up (MOD-AGE-001): the staff age button on the Cinema title page.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { AP } from '../../admin/permissions';
import { http, HttpResponse, server } from '../../test/server';
import { DUNE, RESTRICTED, detail, renderAt, useCinemaApi } from './harness';

const SLOW = { timeout: 10_000 };

const asReviewer = (...perms: (keyof typeof AP)[]) => {
  const bits = perms.reduce((a, n) => a | (1n << BigInt(AP[n])), 0n).toString();
  server.use(
    http.get('*/api/v1/admin/me', () =>
      HttpResponse.json({ is_owner: false, platform_permissions: bits, user_id: '1' }),
    ),
  );
};

describe('age button on the title page', () => {
  it('marks a title for a cinema reviewer, with a reason', async () => {
    useCinemaApi({ detail: detail(DUNE) });
    asReviewer('REVIEW_MEDIA');
    let body: unknown;
    server.use(
      http.post('*/api/v1/moderation/age/restrictions', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          kind: 'cinema_title',
          id: '1',
          label: DUNE.title,
          restricted: true,
          changed: true,
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/cinema/t/1');
    await user.click(await screen.findByRole('button', { name: 'Mark 18+' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: 'Mark this item 18+' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'Explicit scenes');
    await user.click(within(dialog).getByRole('button', { name: 'Mark 18+' }));
    await waitFor(() =>
      expect(body).toEqual({
        kind: 'cinema_title',
        id: '1',
        restricted: true,
        reason: 'Explicit scenes',
      }),
    );
  });

  it('offers removal on a restricted title and stays hidden from a library reviewer', async () => {
    useCinemaApi({ detail: detail(RESTRICTED) });
    asReviewer('REVIEW_READING');
    renderAt(`/cinema/t/${RESTRICTED.id}`);
    await screen.findByRole('heading', { name: 'Alien' }, SLOW);
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.queryByRole('button', { name: 'Remove 18+ mark' })).not.toBeInTheDocument();
  });
});
