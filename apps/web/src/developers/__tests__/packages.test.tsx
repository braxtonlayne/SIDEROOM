import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, openAt, server, signedIn } from './helpers';

beforeEach(signedIn);

const version = (over: object = {}) => ({
  id: '801',
  package_id: '800',
  kind: 'theme',
  slug: 'ocean-night',
  name: 'Ocean Night',
  version: '1.0.0',
  review_state: 'pending',
  review_note: null,
  size_bytes: 2048,
  created_at_ms: 1,
  files: [],
  warnings: [],
  ...over,
});

describe('developer portal: Packages (EXT-PKG-003, EXT-PKG-007)', () => {
  it('submits a zip, shows the version waiting for review and withdraws it', async () => {
    let mine: object[] = [];
    let sentType = '';
    let withdrawn = 0;
    server.use(
      http.get('*/api/v1/packages/mine', () => HttpResponse.json(mine)),
      http.post('*/api/v1/packages', async ({ request }) => {
        sentType = request.headers.get('content-type') ?? '';
        mine = [
          {
            id: '800',
            kind: 'theme',
            slug: 'ocean-night',
            name: 'Ocean Night',
            versions: [version()],
          },
        ];
        return HttpResponse.json(version({ warnings: ['tokens.--ui-text: contrast is low'] }), {
          status: 201,
        });
      }),
      http.post('*/api/v1/packages/versions/801/withdraw', () => {
        withdrawn += 1;
        mine = [
          {
            id: '800',
            kind: 'theme',
            slug: 'ocean-night',
            name: 'Ocean Night',
            versions: [version({ review_state: 'withdrawn' })],
          },
        ];
        return HttpResponse.json(version({ review_state: 'withdrawn' }));
      }),
    );
    openAt('/developers/packages');
    const user = userEvent.setup();
    expect(await screen.findByText('No packages yet')).toBeInTheDocument();

    const zip = new File([new Uint8Array([80, 75, 3, 4])], 'ocean.zip', {
      type: 'application/zip',
    });
    await user.upload(screen.getByLabelText('Package zip'), zip);
    expect(await screen.findByText('Submitted for review')).toBeInTheDocument();
    expect(
      screen.getByText('Ocean Night 1.0.0 is waiting for staff to review it.'),
    ).toBeInTheDocument();
    expect(screen.getByText('tokens.--ui-text: contrast is low')).toBeInTheDocument();
    expect(sentType).toBe('application/zip');

    const list = await screen.findByRole('list', { name: 'Your packages' });
    expect(within(list).getByText('Waiting for review')).toBeInTheDocument();
    await user.click(within(list).getByRole('button', { name: 'Withdraw Ocean Night 1.0.0' }));
    await waitFor(() => expect(withdrawn).toBe(1));
    expect(await within(list).findByText('Withdrawn')).toBeInTheDocument();
    expect(within(list).queryByRole('button', { name: /Withdraw/ })).toBeNull();
  });

  it('lists every problem when the package is refused', async () => {
    server.use(
      http.get('*/api/v1/packages/mine', () => HttpResponse.json([])),
      http.post('*/api/v1/packages', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              problems: {
                '0': {
                  _errors: [
                    { code: 'INVALID_VALUE', message: 'id must be a 3 to 48 character slug' },
                  ],
                },
                '1': {
                  _errors: [
                    { code: 'INVALID_VALUE', message: 'extra.png: is not used by the package' },
                  ],
                },
              },
            },
          },
          { status: 400 },
        ),
      ),
    );
    openAt('/developers/packages');
    const user = userEvent.setup();
    await screen.findByText('No packages yet');
    await user.upload(
      screen.getByLabelText('Package zip'),
      new File([new Uint8Array([1])], 'bad.zip', { type: 'application/zip' }),
    );
    expect(await screen.findByText('The package was refused')).toBeInTheDocument();
    expect(screen.getByText('id must be a 3 to 48 character slug')).toBeInTheDocument();
    expect(screen.getByText('extra.png: is not used by the package')).toBeInTheDocument();
  });

  it('has a creator guide and generated reference pages', async () => {
    server.use(
      http.get('*/api/v1/openapi.json', () =>
        HttpResponse.json({
          paths: {
            '/packages': {
              get: {
                summary: 'Approved packages',
                tags: ['packages'],
                security: [{ session: [] }],
              },
            },
            '/healthz-like': { get: { summary: 'Public thing', tags: ['misc'] } },
          },
        }),
      ),
    );
    const { router } = openAt('/developers/docs/packages');
    expect(await screen.findByRole('heading', { name: 'Packages' })).toBeInTheDocument();
    expect(screen.getByText(/sr-cli package validate/)).toBeInTheDocument();

    await router.navigate({ to: '/developers/docs/$page', params: { page: 'api-reference' } });
    expect(await screen.findByText('Approved packages')).toBeInTheDocument();
    expect(screen.getByText('Showing 2 of 2 endpoints.')).toBeInTheDocument();
    await userEvent.setup().type(screen.getByLabelText('Search endpoints'), 'public');
    expect(screen.getByText('Showing 1 of 2 endpoints.')).toBeInTheDocument();

    await router.navigate({ to: '/developers/docs/$page', params: { page: 'gateway' } });
    expect(await screen.findByRole('table', { name: 'Gateway opcodes' })).toBeInTheDocument();
    expect(screen.getByText('IDENTIFY')).toBeInTheDocument();
    expect(screen.getByText('SR_ACTIVITY_INSTANCE_UPDATE')).toBeInTheDocument();
  });
});
