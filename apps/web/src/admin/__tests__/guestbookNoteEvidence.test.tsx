import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GuestbookNoteEvidence } from '../moderation/GuestbookNoteEvidence';
import { http, HttpResponse, server } from '../../test/server';

// PRF-PRO-001 (wave 50, D-528 / D-529): the case card's Remove asks for a reason and learns on the press.

const payload = {
  kind: 'guestbook_note',
  id: 'n1',
  body: 'hello',
  status: 'pending',
  author_id: null,
};

function setup(status: number) {
  const calls: string[] = [];
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.delete('*/api/v1/moderation/guestbook-notes/n1', ({ request }) => {
      calls.push(new URL(request.url).search);
      return status === 204
        ? new HttpResponse(null, { status })
        : HttpResponse.json({ code: 'SR_NOT_FOUND', message: 'gone' }, { status });
    }),
  );
  const run = vi.fn(async (fn: () => Promise<unknown>) => {
    await fn();
  });
  render(<GuestbookNoteEvidence payload={payload} canRemove run={run} />);
  return calls;
}

async function remove() {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Remove note' }));
  const dialog = await screen.findByRole('dialog');
  const confirm = Array.from(dialog.querySelectorAll('button')).find(
    (b) => b.textContent === 'Remove note',
  ) as HTMLButtonElement;
  expect(confirm).toBeDisabled();
  await user.type(screen.getByRole('textbox'), 'targeted harassment');
  expect(confirm).toBeEnabled();
  await user.click(confirm);
}

describe('the guestbook note evidence card', () => {
  it('sends the reason and turns the button into "Note removed"', async () => {
    const calls = setup(204);
    await remove();
    expect(await screen.findByText('Note removed')).toBeInTheDocument();
    expect(calls).toEqual(['?reason=targeted+harassment']);
    expect(screen.queryByRole('button', { name: 'Remove note' })).toBeNull();
  });

  it('a 404 says the note was already removed', async () => {
    setup(404);
    await remove();
    expect(await screen.findByText('This note was already removed')).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Remove note' })).toBeNull());
  });
});
