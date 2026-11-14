// SEC-AUD-199 (D-686): the logo row of the Live TV channel editor: upload after the first save, replace, remove,
// the client-side refusals and the server's own sentence.
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const CURATOR = ['STAFF_BASIC', 'REVIEW_MEDIA', 'PUBLISH_MEDIA', 'MANAGE_TV'] as const;
const TZ = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

const channel = (over: Record<string, unknown> = {}) => ({
  id: '501',
  number: 101,
  call_sign: 'RETRO',
  name: 'Retro Room',
  era_label: null,
  logo_url: null,
  kind: 'manual',
  timezone: TZ,
  age_restricted: false,
  enabled: true,
  filler_policy: 'slate',
  theme: null,
  seed: 7,
  viewers: 0,
  rules: null,
  ...over,
});

interface Calls {
  puts: { url: string; type: string | null; bytes: number }[];
  deletes: string[];
  saves: unknown[];
}

function useLogoApi(
  o: { logo?: string | null; putStatus?: number; putMessage?: string; created?: boolean } = {},
) {
  const calls: Calls = { puts: [], deletes: [], saves: [] };
  let logo = o.logo ?? null;
  server.use(
    http.get('*/api/v1/tv/timezones', () => HttpResponse.json({ items: ['UTC', TZ] })),
    http.get('*/api/v1/admin/tv/channels', () =>
      HttpResponse.json({ items: o.created ? [] : [channel({ logo_url: logo })] }),
    ),
    http.post('*/api/v1/admin/tv/channels', async ({ request }) =>
      HttpResponse.json(
        channel({ id: '777', ...((await readJson(request)) as object), logo_url: null }),
        { status: 201 },
      ),
    ),
    http.put('*/api/v1/admin/tv/channels/:id', async ({ request, params }) => {
      calls.saves.push(await readJson(request));
      return HttpResponse.json(channel({ id: params.id, logo_url: logo }));
    }),
    http.put('*/api/v1/admin/tv/channels/:id/logo', async ({ request, params }) => {
      if (o.putStatus)
        return HttpResponse.json(
          { code: 50035, message: o.putMessage ?? 'refused' },
          { status: o.putStatus },
        );
      const body = await request.arrayBuffer();
      calls.puts.push({
        url: new URL(request.url).pathname.replace('/api/v1', ''),
        type: request.headers.get('content-type'),
        bytes: body.byteLength,
      });
      logo = `/cdn/tv/${String(params.id)}/logo-${String(900 + calls.puts.length)}.png?ex=9&sig=s`;
      return HttpResponse.json(channel({ id: params.id, logo_url: logo }));
    }),
    http.delete('*/api/v1/admin/tv/channels/:id/logo', ({ request, params }) => {
      calls.deletes.push(new URL(request.url).pathname.replace('/api/v1', ''));
      logo = null;
      return HttpResponse.json(channel({ id: params.id, logo_url: null }));
    }),
  );
  return calls;
}

const png = (bytes = 64, name = 'logo.png', type = 'image/png') =>
  new File([new Uint8Array(bytes)], name, { type });

async function openEditor(user: ReturnType<typeof userEvent.setup>) {
  openAdmin('cinema-live-tv');
  await user.click(await screen.findByRole('button', { name: 'Open Retro Room' }));
  await screen.findByRole('form', { name: 'Channel details' });
}

describe('Live TV channel editor: the logo', () => {
  it('offers the upload only after the first save, and says so', async () => {
    asStaff({ perms: [...CURATOR] });
    useLogoApi({ created: true });
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'New channel' }));
    await screen.findByRole('form', { name: 'Channel details' });
    expect(screen.queryByLabelText('Upload logo')).not.toBeInTheDocument();
    expect(screen.getByText('Save the channel first, then upload its logo here.')).toBeVisible();
    // The old "not available" lines are gone.
    expect(screen.queryByText(/not available yet/i)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Channel number'), '110');
    await user.type(screen.getByLabelText('Channel name'), 'Retro Room');
    await user.click(screen.getByRole('button', { name: 'Create channel' }));
    expect(await screen.findByLabelText('Upload logo')).toBeInTheDocument();
    expect(screen.queryByText('Save the channel first, then upload its logo here.')).toBeNull();
  });

  it('uploads a logo, shows it, keeps its key on save, replaces it and removes it; passes axe', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useLogoApi();
    const user = userEvent.setup({ applyAccept: false });
    await openEditor(user);
    expect(screen.getByText(/No logo\. The guide shows the call sign as text/)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Remove logo' })).not.toBeInTheDocument();
    expect(screen.getByText(/PNG, JPEG or WebP, up to 2 MiB/)).toBeVisible();

    await user.upload(screen.getByLabelText('Upload logo'), png(1234));
    expect(await screen.findByText('Logo uploaded.')).toBeInTheDocument();
    expect(calls.puts).toHaveLength(1);
    expect(calls.puts[0]).toMatchObject({ url: '/admin/tv/channels/501/logo', type: 'image/png' });
    const img = await screen.findByRole('img', { name: 'Channel logo' });
    expect(img.getAttribute('src')).toContain('/cdn/tv/501/logo-901.png');
    expect(screen.queryByText(/No logo\./)).not.toBeInTheDocument();

    // A save echoes the new key back so the logo stays.
    await user.click(screen.getByRole('button', { name: 'Save channel' }));
    await waitFor(() => expect(calls.saves).toHaveLength(1));
    expect((calls.saves[0] as { logo_key?: string }).logo_key).toBe('tv/501/logo-901.png');
    // A manual channel sends no rules at all: the server refuses null (422).
    expect(calls.saves[0]).not.toHaveProperty('rules');

    // Replacing swaps the image.
    await user.upload(screen.getByLabelText('Upload logo'), png(99, 'b.webp', 'image/webp'));
    await waitFor(() =>
      expect(screen.getByRole('img', { name: 'Channel logo' }).getAttribute('src')).toContain(
        'logo-902.png',
      ),
    );
    expect(calls.puts[1]).toMatchObject({ type: 'image/webp' });
    expect(await axeViolations(document.body)).toEqual([]);

    // Removing clears it, and a following save sends no key.
    await user.click(screen.getByRole('button', { name: 'Remove logo' }));
    expect(await screen.findByText('Logo removed.')).toBeInTheDocument();
    expect(calls.deletes).toEqual(['/admin/tv/channels/501/logo']);
    expect(screen.queryByRole('img', { name: 'Channel logo' })).not.toBeInTheDocument();
    expect(screen.getByText(/No logo\. The guide shows the call sign as text/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Save channel' }));
    await waitFor(() => expect(calls.saves).toHaveLength(2));
    expect(calls.saves[1]).not.toHaveProperty('logo_key');
  });

  it('refuses a file that is too large or not a PNG, JPEG or WebP before sending it', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useLogoApi();
    const user = userEvent.setup({ applyAccept: false });
    await openEditor(user);
    await user.upload(screen.getByLabelText('Upload logo'), png(3 * 1024 * 1024));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'That file is 3.0 MiB. A logo can be at most 2 MiB.',
    );
    await user.upload(
      screen.getByLabelText('Upload logo'),
      new File(['<svg/>'], 'logo.svg', { type: 'image/svg+xml' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'A logo must be a PNG, JPEG or WebP image.',
    );
    expect(calls.puts).toEqual([]);
  });

  it('shows the sentence the server answers with', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useLogoApi({
      putStatus: 400,
      putMessage: 'the image could not be read: truncated',
    });
    const user = userEvent.setup({ applyAccept: false });
    await openEditor(user);
    await user.upload(screen.getByLabelText('Upload logo'), png(10));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'the image could not be read: truncated',
    );
    expect(calls.puts).toEqual([]);
    expect(screen.queryByRole('img', { name: 'Channel logo' })).not.toBeInTheDocument();
  });
});
