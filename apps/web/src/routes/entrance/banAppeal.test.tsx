import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

function atLogin() {
  const router = createPublicRouter('/login', createMemoryHistory({ initialEntries: ['/login'] }));
  render(<RouterProvider router={router} />);
}

async function signInBanned(user: ReturnType<typeof userEvent.setup>) {
  await user.type(await screen.findByLabelText('Username or email'), 'pat');
  await user.type(screen.getByLabelText('Password'), 'hunter2');
  await user.click(screen.getByRole('button', { name: 'Step inside' }));
}

describe('the ban screen appeal (MOD-APL-004)', () => {
  it('offers an appeal to a banned account and sends the credentials with it', async () => {
    let sent: unknown;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ code: 900008, message: 'banned' }, { status: 403 }),
      ),
      http.post('*/api/v1/moderation/appeals', async ({ request }) => {
        sent = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    atLogin();
    await signInBanned(user);
    expect(await screen.findByText('This account is banned from SIDE ROOM.')).toBeInTheDocument();
    const send = screen.getByRole('button', { name: 'Send appeal' });
    expect(send).toBeDisabled();
    await user.type(screen.getByLabelText('Why should this be looked at again?'), 'It was not me');
    await user.click(send);
    expect(await screen.findByText(/Your appeal was sent/)).toBeInTheDocument();
    expect(sent).toEqual({ login: 'pat', password: 'hunter2', message: 'It was not me' });
  });

  it('says so when the decision was already appealed, and a wrong password gets no form', async () => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ code: 900008, message: 'banned' }, { status: 403 }),
      ),
      http.post('*/api/v1/moderation/appeals', () =>
        HttpResponse.json({ code: 900010, message: 'conflict' }, { status: 409 }),
      ),
    );
    const user = userEvent.setup();
    atLogin();
    await signInBanned(user);
    await user.type(await screen.findByLabelText('Why should this be looked at again?'), 'please');
    await user.click(screen.getByRole('button', { name: 'Send appeal' }));
    expect(await screen.findByText('This decision was already appealed.')).toBeInTheDocument();

    server.use(
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ code: 40001, message: 'no' }, { status: 401 }),
      ),
    );
    await user.click(screen.getByRole('button', { name: 'Step inside' }));
    expect(await screen.findByText("That doesn't match our list.")).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Send appeal' })).toBeNull();
  });
});
