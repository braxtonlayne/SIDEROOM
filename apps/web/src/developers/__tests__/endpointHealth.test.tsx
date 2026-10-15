import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { APP, http, HttpResponse, openAt, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

const KEY = 'ab'.repeat(32);
const disabledApp = {
  ...APP,
  interactions_endpoint_url: 'https://bot.example.com/i',
  verify_key: KEY,
  interactions_endpoint_state: 'disabled' as const,
};

describe('application: endpoint health (D-186)', () => {
  it('explains a switched-off endpoint and re-enables it after the PING check', async () => {
    const user = userEvent.setup();
    serveApp(disabledApp);
    let verified = 0;
    server.use(
      http.post('*/api/v1/applications/500/endpoint/verify', () => {
        verified += 1;
        return HttpResponse.json({
          interactions_endpoint_url: 'https://bot.example.com/i',
          interactions_endpoint_state: 'enabled',
          verify_key: KEY,
        });
      }),
    );
    openAt('/developers/applications/500');
    expect(await screen.findByText('Endpoint switched off')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Verify and re-enable' }));
    await waitFor(() => expect(verified).toBe(1));
    await waitFor(() =>
      expect(screen.queryByText('Endpoint switched off')).not.toBeInTheDocument(),
    );
    expect(
      await screen.findByText('Active: interactions are sent to this URL.'),
    ).toBeInTheDocument();
  });

  it('shows the refusal when the endpoint still fails the check', async () => {
    const user = userEvent.setup();
    serveApp(disabledApp);
    server.use(
      http.post('*/api/v1/applications/500/endpoint/verify', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              interactions_endpoint_url: {
                _errors: [{ code: 'X', message: 'The endpoint could not be reached.' }],
              },
            },
          },
          { status: 400 },
        ),
      ),
    );
    openAt('/developers/applications/500');
    await user.click(await screen.findByRole('button', { name: 'Verify and re-enable' }));
    expect(await screen.findByText('The endpoint could not be reached.')).toBeInTheDocument();
    expect(screen.getByText('Endpoint switched off')).toBeInTheDocument();
  });
});
