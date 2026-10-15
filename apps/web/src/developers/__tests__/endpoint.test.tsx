import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { endpointUrlError } from '../tabs/InteractionsEndpoint';
import { APP, http, HttpResponse, openAt, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

const KEY = 'ab'.repeat(32);

describe('endpoint url rules', () => {
  it('mirrors the server', () => {
    expect(endpointUrlError('')).toBeNull();
    expect(endpointUrlError('https://bot.example.com/interactions')).toBeNull();
    expect(endpointUrlError('http://bot.example.com/')).toBe('https');
    expect(endpointUrlError('https://u:p@bot.example.com/')).toBe('invalid');
    expect(endpointUrlError('https://bot.example.com/#x')).toBe('invalid');
    expect(endpointUrlError('https://bot.example.com/a b')).toBe('invalid');
    expect(endpointUrlError('https:///x')).toBe('invalid');
    expect(endpointUrlError(`https://a.com/${'x'.repeat(520)}`)).toBe('invalid');
  });
});

describe('application: interactions endpoint', () => {
  it('creates the public key first, then verifies and saves the URL', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp({ ...APP });
    let created = 0;
    server.use(
      http.post('*/api/v1/applications/500/interactions-key', () => {
        created += 1;
        return HttpResponse.json({ verify_key: KEY });
      }),
    );
    openAt('/developers/applications/500');
    const url = await screen.findByLabelText('Interactions endpoint URL');
    const save = screen.getByRole('button', { name: 'Verify and save' });
    await user.type(url, 'https://bot.example.com/i');
    expect(save).toBeDisabled(); // no key yet
    await user.click(screen.getByRole('button', { name: 'Create public key' }));
    expect(await screen.findByDisplayValue(KEY)).toBeInTheDocument();
    expect(created).toBe(1);
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);
    await waitFor(() =>
      expect(patches).toEqual([{ interactions_endpoint_url: 'https://bot.example.com/i' }]),
    );
    expect(await screen.findByText('Endpoint verified and saved.')).toBeInTheDocument();
  });

  it('shows what the server said when it refuses the URL', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP, verify_key: KEY });
    server.use(
      http.patch('*/api/v1/applications/500', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              interactions_endpoint_url: {
                _errors: [
                  {
                    code: 'INTERACTIONS_ENDPOINT_VERIFY',
                    message:
                      'The endpoint must reject a request with an invalid signature with 401.',
                  },
                ],
              },
            },
          },
          { status: 400 },
        ),
      ),
    );
    openAt('/developers/applications/500');
    await user.type(
      await screen.findByLabelText('Interactions endpoint URL'),
      'https://bot.example.com/i',
    );
    await user.click(screen.getByRole('button', { name: 'Verify and save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The endpoint must reject a request with an invalid signature with 401.',
    );
  });

  it('refuses a non-https URL locally and can remove a saved endpoint', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp({
      ...APP,
      verify_key: KEY,
      interactions_endpoint_url: 'https://bot.example.com/i',
    });
    openAt('/developers/applications/500');
    const url = await screen.findByLabelText('Interactions endpoint URL');
    expect(url).toHaveValue('https://bot.example.com/i');
    await user.clear(url);
    await user.type(url, 'http://plain.example.com/');
    expect(await screen.findByText('The URL must start with https://.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Verify and save' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Remove endpoint' }));
    await waitFor(() => expect(patches).toEqual([{ interactions_endpoint_url: null }]));
    expect(
      await screen.findByText('Endpoint removed. Interactions use the gateway again.'),
    ).toBeInTheDocument();
  });
});
