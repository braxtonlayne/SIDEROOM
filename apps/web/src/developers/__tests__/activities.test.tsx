import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ActivityBundle, ActivityConfig } from '../types';
import { APP, http, HttpResponse, openAt, readJson, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

const SHA_A = 'a'.repeat(64);
const SHA_B = 'b'.repeat(64);

const bundle = (over: Partial<ActivityBundle> = {}): ActivityBundle => ({
  id: '801',
  sha256: SHA_A,
  byte_size: 2048,
  entry_count: 3,
  state: 'draft',
  uploaded_at_ms: 1,
  reviewed_at_ms: null,
  note: null,
  ...over,
});

const base = (over: Partial<ActivityConfig['config']> = {}): ActivityConfig => ({
  enabled: true,
  origin: 'https://500.activities.chat.example.org',
  implicit_redirect_uri: 'https://500.activities.chat.example.org/',
  config: {
    application_id: '500',
    dev_entry_url: null,
    url_mappings: [],
    review_state: 'draft',
    reviewed_bundle: null,
    bundles: [],
    review_note: null,
    updated_at_ms: 0,
    ...over,
  },
});

const ENTRIES = [
  { path: 'index.html', size: 120, content_type: 'text/html; charset=utf-8' },
  { path: 'app.js', size: 900, content_type: 'text/javascript; charset=utf-8' },
  { path: 'img/logo.png', size: 1028, content_type: 'image/png' },
];

const zip = (name = 'game.zip') =>
  new File([new Uint8Array([80, 75, 3, 4])], name, { type: 'application/zip' });

describe('application: activities tab (DEV-PORT-003, J-12)', () => {
  it('uploads a zip, lists its files, submits that bundle and shows it in review', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let current = base();
    const posted: { type: string | null; size: number }[] = [];
    let submitted: unknown;
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(current)),
      http.post('*/api/v1/applications/500/activity/bundles', async ({ request }) => {
        const body = await request.arrayBuffer();
        posted.push({ type: request.headers.get('content-type'), size: body.byteLength });
        current = base({ bundles: [bundle()], updated_at_ms: 5 });
        return HttpResponse.json({ bundle: bundle(), entries: ENTRIES }, { status: 201 });
      }),
      http.get('*/api/v1/applications/500/activity/bundles/801', () =>
        HttpResponse.json({ bundle: bundle(), entries: ENTRIES }),
      ),
      http.post('*/api/v1/applications/500/activity/submit', async ({ request }) => {
        submitted = await readJson(request);
        current = base({
          bundles: [bundle({ state: 'pending' })],
          review_state: 'pending',
          updated_at_ms: 6,
        });
        return HttpResponse.json(current);
      }),
    );
    openAt('/developers/applications/500?tab=activities');
    expect(await screen.findByText('https://500.activities.chat.example.org')).toBeInTheDocument();
    // The Activity address needs no registration.
    expect(screen.getByText(/redirect URI of this application automatically/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Submit for review' })).toBeNull();

    await user.upload(screen.getByTestId('bundle-input'), zip());
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(posted[0]).toEqual({ type: 'application/zip', size: 4 });
    expect(await screen.findByText(/Uploaded: 3 files/)).toBeInTheDocument();
    // The listing shows what is in the bundle.
    const list = await screen.findByRole('list', { name: 'Files in the bundle' });
    expect(within(list).getByText('index.html')).toBeInTheDocument();
    expect(within(list).getByText('img/logo.png')).toBeInTheDocument();
    expect(screen.getByText(SHA_A)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Submit for review' }));
    await waitFor(() => expect(submitted).toEqual({ bundle_id: '801' }));
    expect(await screen.findByText('Waiting for staff to review it.')).toBeInTheDocument();
    expect(screen.getByText('In review')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Submit for review' })).toBeNull();
  });

  it('names the offending entry when a zip is refused, and checks the size first', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let uploads = 0;
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(base())),
      http.post('*/api/v1/applications/500/activity/bundles', () => {
        uploads += 1;
        return HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              bundle: {
                _errors: [
                  { code: 'INVALID_VALUE', message: '../evil.js: leaves the bundle with . or ..' },
                ],
              },
            },
          },
          { status: 400 },
        );
      }),
    );
    openAt('/developers/applications/500?tab=activities');
    const input = await screen.findByTestId('bundle-input');
    await user.upload(input, zip('bad.zip'));
    expect(await screen.findByRole('alert')).toHaveTextContent('../evil.js: leaves the bundle');

    // Over 32 MiB is refused without sending anything.
    const big = new File([new Uint8Array(8)], 'big.zip', { type: 'application/zip' });
    Object.defineProperty(big, 'size', { value: 33 * 1024 * 1024 });
    await user.upload(input, big);
    expect(await screen.findByRole('alert')).toHaveTextContent('larger than 32 MiB');
    expect(uploads).toBe(1);
  });

  it('saves the development URL and a mapping, for an application in development', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let current = base();
    const saved: unknown[] = [];
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(current)),
      http.put('*/api/v1/applications/500/activity', async ({ request }) => {
        const body = (await request.json()) as {
          dev_entry_url: string | null;
          url_mappings: { prefix: string; target: string }[];
        };
        saved.push(body);
        current = base({ dev_entry_url: body.dev_entry_url, url_mappings: body.url_mappings });
        return HttpResponse.json(current);
      }),
    );
    openAt('/developers/applications/500?tab=activities');
    await user.type(await screen.findByLabelText('Development URL'), 'https://dev.example.com/');
    await user.click(screen.getByRole('button', { name: 'Add a mapping' }));
    await user.type(screen.getByLabelText('Path'), '/api');
    await user.type(screen.getByLabelText('Target'), 'https://api.example.com');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      dev_entry_url: 'https://dev.example.com/',
      url_mappings: [{ prefix: '/api', target: 'https://api.example.com' }],
    });
  });

  it('has no development URL once the application is released, and shows the reviewed bundle', async () => {
    serveApp({ ...APP, release_state: 'released' });
    const approved = bundle({ id: '802', sha256: SHA_B, state: 'approved' });
    server.use(
      http.get('*/api/v1/applications/500/activity', () =>
        HttpResponse.json(
          base({ review_state: 'approved', reviewed_bundle: approved, bundles: [approved] }),
        ),
      ),
    );
    openAt('/developers/applications/500?tab=activities');
    expect(await screen.findByText(/Approved\. People can start it/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Development URL')).toBeNull();
    expect(screen.getByText('Approved bundle (SHA-256):').parentElement).toHaveTextContent(SHA_B);
  });

  it('says when Activities are off and shows a reviewer note', async () => {
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/activity', () =>
        HttpResponse.json({
          ...base({
            review_state: 'rejected',
            review_note: 'Too many permissions',
            bundles: [bundle({ state: 'rejected', note: 'Too many permissions' })],
          }),
          enabled: false,
        }),
      ),
    );
    openAt('/developers/applications/500?tab=activities');
    expect(await screen.findByText(/not switched on for this server yet/)).toBeInTheDocument();
    expect(screen.getAllByText(/Reviewer note: Too many permissions/).length).toBeGreaterThan(0);
  });

  it('shows the server field error when the settings are refused', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(base())),
      http.put('*/api/v1/applications/500/activity', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              dev_entry_url: {
                _errors: [{ code: 'INVALID_VALUE', message: 'Must be an https URL.' }],
              },
            },
          },
          { status: 400 },
        ),
      ),
    );
    openAt('/developers/applications/500?tab=activities');
    await user.type(await screen.findByLabelText('Development URL'), 'http://nope.example.com');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Must be an https URL.');
  });
});

describe('application: OAuth2 tab lists the implicit Activity redirect (J-13)', () => {
  it('shows the Activity address read-only as automatic, and nothing without an Activity', async () => {
    serveApp({ ...APP });
    server.use(http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(base())));
    openAt('/developers/applications/500?tab=oauth2');
    const auto = await screen.findByRole('list', {
      name: 'Automatic redirect URI of the Activity',
    });
    expect(within(auto).getByText('https://500.activities.chat.example.org/')).toBeInTheDocument();
    expect(within(auto).getByText('Activity (automatic)')).toBeInTheDocument();
    // Read-only: no Remove button beside it.
    expect(within(auto).queryByRole('button')).toBeNull();
  });

  it('shows no automatic row when the application has no Activity', async () => {
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/activity', () =>
        HttpResponse.json({ ...base(), implicit_redirect_uri: null }),
      ),
    );
    openAt('/developers/applications/500?tab=oauth2');
    await screen.findAllByText('https://example.com/cb');
    expect(
      screen.queryByRole('list', { name: 'Automatic redirect URI of the Activity' }),
    ).toBeNull();
  });
});
