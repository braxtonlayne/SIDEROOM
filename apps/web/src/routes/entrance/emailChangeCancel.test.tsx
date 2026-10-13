import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('email change cancel page (/email-change/cancel?token=)', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf-test' })));
  });

  it('posts the token once and says what happened', async () => {
    const posted: unknown[] = [];
    server.use(
      http.post('*/api/v1/sr/auth/email/cancel', async ({ request }) => {
        posted.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/email-change/cancel?token=tok1');
    expect(
      await screen.findByText(/it is cancelled and your email is unchanged/),
    ).toBeInTheDocument();
    expect(posted).toEqual([{ token: 'tok1' }]);
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('explains an invalid or used link', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/email/cancel', () =>
        HttpResponse.json(
          { errors: { token: { _errors: [{ code: 'invalid', message: 'x' }] } } },
          { status: 400 },
        ),
      ),
    );
    at('/email-change/cancel?token=old');
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'invalid, has expired, or was already used',
    );
  });

  it('asks for the full link when the token is missing', async () => {
    at('/email-change/cancel');
    expect(await screen.findByRole('alert')).toHaveTextContent('missing its token');
  });
});
