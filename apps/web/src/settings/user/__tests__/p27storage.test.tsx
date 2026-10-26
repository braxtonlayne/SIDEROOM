import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { byosMockHandlers, mockMount } from '../storage/mocks';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('User Settings > Storage (BYOS-UI-001, against MSW mocks)', () => {
  it('lists mounts with status and key hint but never a secret', async () => {
    server.use(...byosMockHandlers({ mounts: [mockMount({ key_hint: 'zz99' })] }));
    openSettings('storage');
    expect(await screen.findByText('My Garage')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(screen.getByText(/secret ending zz99/)).toBeInTheDocument();
    expect(screen.getByText('Not ready')).toBeInTheDocument();
  });

  it('shows the empty state and connects a bucket with the provider defaults', async () => {
    const user = userEvent.setup();
    let sent: Record<string, unknown> = {};
    server.use(
      http.post('*/api/v1/users/@me/storage/mounts', async ({ request }) => {
        sent = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(mockMount({ id: '700', label: 'R2 shelf', provider: 'r2' }), {
          status: 201,
        });
      }),
      ...byosMockHandlers(),
    );
    openSettings('storage');
    expect(await screen.findByText('No bucket connected yet.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Connect a bucket' }));
    // Provider guidance follows the provider.
    expect(screen.getByText(/In IAM, create a user/)).toBeInTheDocument();
    await user.click(screen.getByRole('combobox', { name: 'Provider' }));
    await user.click(await screen.findByRole('option', { name: 'Cloudflare R2' }));
    expect(screen.getByText(/R2 API token/)).toBeInTheDocument();
    expect(screen.getByLabelText('Region')).toHaveValue('auto');
    await user.type(screen.getByLabelText('Name'), 'R2 shelf');
    await user.type(screen.getByLabelText('Bucket'), 'shelf');
    await user.type(screen.getByLabelText('Access key ID'), 'AKIAEXAMPLE');
    await user.type(screen.getByLabelText('Secret access key'), 'sekrit-value');
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() =>
      expect(sent).toMatchObject({
        provider: 'r2',
        region: 'auto',
        bucket: 'shelf',
        path_style: false,
        secret_access_key: 'sekrit-value',
      }),
    );
  });

  it('renders the pass, warn and fail rows of a connection test', async () => {
    const user = userEvent.setup();
    server.use(
      ...byosMockHandlers({
        mounts: [mockMount()],
        probes: [
          { id: 'head_bucket', status: 'pass', message: 'The bucket exists.' },
          { id: 'list_buckets', status: 'warn', message: 'This key can see other buckets.' },
          { id: 'put', status: 'fail', message: 'Writing a probe object was refused.' },
        ],
      }),
    );
    openSettings('storage');
    await user.click(await screen.findByRole('button', { name: 'Test connection' }));
    const table = await screen.findByRole('table', { name: 'Connection test results' });
    expect(within(table).getByText('Pass')).toBeInTheDocument();
    expect(within(table).getByText('Warning')).toBeInTheDocument();
    expect(within(table).getByText('Fail')).toBeInTheDocument();
    expect(within(table).getByText('This key can see other buckets.')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('Failing')).toBeInTheDocument());
  });

  it('checks CORS, applies it, and then reports the bucket ready', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ mounts: [mockMount()], cors: 'missing' }));
    openSettings('storage');
    await user.click(await screen.findByRole('button', { name: 'Check CORS' }));
    expect(await screen.findByText(/no matching rule/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Apply CORS' }));
    expect(await screen.findByText('The bucket rules are correct.')).toBeInTheDocument();
    expect(await screen.findByText('Ready')).toBeInTheDocument();
  });

  it('shows the provider steps when the API refuses CORS changes', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ mounts: [mockMount({ provider: 'b2' })], cors: 'manual' }));
    openSettings('storage');
    await user.click(await screen.findByRole('button', { name: 'Apply CORS' }));
    expect(await screen.findByText(/paste the CORS rule by hand/)).toBeInTheDocument();
    expect(screen.getByText('Not ready')).toBeInTheDocument();
  });

  it('surfaces the mount cap and removes a mount after confirmation', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ mounts: [mockMount()], cap: 1 }));
    openSettings('storage');
    await user.click(await screen.findByRole('button', { name: 'Connect a bucket' }));
    await user.type(screen.getByLabelText('Name'), 'Second');
    await user.type(screen.getByLabelText('Bucket'), 'b');
    await user.type(screen.getByLabelText('Access key ID'), 'k');
    await user.type(screen.getByLabelText('Secret access key'), 's');
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    expect(await screen.findByText('Could not connect the bucket')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await user.click(screen.getByRole('button', { name: 'Remove' }));
    await user.click(screen.getByRole('button', { name: 'Confirm remove' }));
    expect(await screen.findByText('No bucket connected yet.')).toBeInTheDocument();
  });
});

describe('Provider policy examples', () => {
  it('shows a copyable bucket-scoped policy for S3 and words only for R2', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers());
    openSettings('storage');
    await user.click(await screen.findByRole('button', { name: 'Connect a bucket' }));
    await user.type(screen.getByLabelText('Bucket'), 'my-shelf');
    await user.type(screen.getByLabelText('Folder prefix'), 'media');
    const pre = screen.getByLabelText('Bucket policy example');
    expect(pre.textContent).toContain('arn:aws:s3:::my-shelf/media/*');
    expect(pre.textContent).toContain('s3:AbortMultipartUpload');
    expect(screen.getByText(/current docs before you use it/)).toBeInTheDocument();
    await user.click(screen.getByRole('combobox', { name: 'Provider' }));
    await user.click(await screen.findByRole('option', { name: 'Cloudflare R2' }));
    expect(screen.queryByLabelText('Bucket policy example')).toBeNull();
    expect(screen.getByText(/Check against your provider/)).toBeInTheDocument();
  });
});
