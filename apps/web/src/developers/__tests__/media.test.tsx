import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { suggestName } from '../tabs/Media';
import type { AppAsset, AppEmoji } from '../types';
import { APP, http, HttpResponse, openAt, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

const png = () =>
  new File([new Uint8Array([137, 80, 78, 71])], 'Party Time.png', { type: 'image/png' });

describe('application media names', () => {
  it('suggests a valid name from a file name', () => {
    expect(suggestName('Party Time.png', true)).toBe('Party_Time');
    expect(suggestName('Big Logo.PNG', false)).toBe('big_logo');
    expect(suggestName('x.png', true)).toBe('');
    expect(suggestName(`${'a'.repeat(40)}.png`, true)).toHaveLength(32);
  });
});

describe('application: emojis tab (DEV-PORT-003)', () => {
  it('uploads, renames and deletes an emoji', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let items: AppEmoji[] = [];
    const posted: { name: string; image: string }[] = [];
    const renamed: string[] = [];
    const removed: string[] = [];
    server.use(
      http.get('*/api/v1/applications/500/emojis', () => HttpResponse.json({ items })),
      http.post('*/api/v1/applications/500/emojis', async ({ request }) => {
        const body = (await request.json()) as { name: string; image: string };
        posted.push(body);
        const e: AppEmoji = {
          id: '900',
          name: body.name,
          animated: false,
          url: '/cdn/emoji/app/500/900.png',
          created_at: '2026-10-02T00:00:00Z',
        };
        items = [e];
        return HttpResponse.json(e, { status: 201 });
      }),
      http.patch('*/api/v1/applications/500/emojis/:id', async ({ request }) => {
        const body = (await request.json()) as { name: string };
        renamed.push(body.name);
        items = items.map((e) => ({ ...e, name: body.name }));
        return HttpResponse.json(items[0]);
      }),
      http.delete('*/api/v1/applications/500/emojis/:id', ({ params }) => {
        removed.push(String(params.id));
        items = [];
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAt('/developers/applications/500?tab=emojis');
    expect(await screen.findByText('No emojis yet.')).toBeInTheDocument();

    await user.upload(screen.getByLabelText('Image'), png());
    // The name is suggested from the file name.
    expect(screen.getByLabelText('Emoji name')).toHaveValue('Party_Time');
    await user.click(screen.getByRole('button', { name: 'Upload' }));
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(posted[0]?.name).toBe('Party_Time');
    expect(posted[0]?.image.startsWith('data:image/png;base64,')).toBe(true);
    expect(await screen.findByText(':Party_Time:')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Rename Party_Time' }));
    const field = screen.getAllByLabelText('Emoji name').at(-1)!;
    await user.clear(field);
    await user.type(field, 'party');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(renamed).toEqual(['party']));
    expect(await screen.findByText(':party:')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Delete party' }));
    await waitFor(() => expect(removed).toEqual(['900']));
    expect(await screen.findByText('No emojis yet.')).toBeInTheDocument();
  });

  it('refuses a file over the size limit without calling the server', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    const posted: unknown[] = [];
    server.use(
      http.get('*/api/v1/applications/500/emojis', () => HttpResponse.json({ items: [] })),
      http.post('*/api/v1/applications/500/emojis', async ({ request }) => {
        posted.push(await request.json());
        return new HttpResponse(null, { status: 500 });
      }),
    );
    openAt('/developers/applications/500?tab=emojis');
    await screen.findByText('No emojis yet.');
    const big = new File([new Uint8Array(300 * 1024)], 'big.png', { type: 'image/png' });
    await user.upload(screen.getByLabelText('Image'), big);
    expect(await screen.findByText('That file is larger than 256 KB.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Upload' })).toBeDisabled();
    expect(posted).toHaveLength(0);
  });
});

describe('application: Rich Presence tab (DEV-PORT-003)', () => {
  it('uploads a small image asset and shows server errors', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let items: AppAsset[] = [];
    const posted: { name: string; type: number }[] = [];
    server.use(
      http.get('*/api/v1/applications/500/assets', () => HttpResponse.json(items)),
      http.post('*/api/v1/applications/500/assets', async ({ request }) => {
        const body = (await request.json()) as { name: string; type: number };
        posted.push(body);
        if (posted.length === 1)
          return HttpResponse.json(
            {
              code: 50035,
              message: 'Invalid Form Body',
              errors: { name: { _errors: [{ code: 'X', message: 'That asset name is taken.' }] } },
            },
            { status: 400 },
          );
        const a: AppAsset = {
          id: '800',
          name: body.name,
          type: 2,
          url: '/cdn/emoji/app-assets/500/800.png',
        };
        items = [a];
        return HttpResponse.json(a, { status: 201 });
      }),
    );
    openAt('/developers/applications/500?tab=richpresence');
    expect(await screen.findByText('No assets yet.')).toBeInTheDocument();
    await user.upload(screen.getByLabelText('Image'), png());
    expect(screen.getByLabelText('Asset name')).toHaveValue('party_time');
    await user.selectOptions(screen.getByLabelText('Type'), 'Small image');
    await user.click(screen.getByRole('button', { name: 'Upload' }));
    expect(await screen.findByText('That asset name is taken.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Upload' }));
    await waitFor(() => expect(posted).toHaveLength(2));
    expect(posted[1]).toMatchObject({ name: 'party_time', type: 2 });
    expect(await screen.findByText('party_time')).toBeInTheDocument();
  });
});
