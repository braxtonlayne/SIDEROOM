import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { afterEach, describe, expect, it } from 'vitest';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { MemoryLedger, type MountUploadRow } from '../../../lib/uploadLedger';
import { MountUploadManager, realMountUploadApi, setMountUploads } from '../mountUploads';

const open = (path: string) => {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
};

interface World {
  keys: Map<string, number>;
  calls: { method: string; path: string; body?: unknown }[];
  canUpload: boolean;
  canManage: boolean;
  /** Parts the "bucket" received, by URL. */
  parts: string[];
}

/** An in-memory server for mount 501: the read routes and every write route of BYOS-BRW-001. */
function world(over: Partial<World> = {}, keys: Record<string, number> = {}) {
  const w: World = {
    keys: new Map(Object.entries(keys)),
    calls: [],
    canUpload: true,
    canManage: true,
    parts: [],
    ...over,
  };
  const conflict = (code: string) =>
    HttpResponse.json(
      {
        code: 900010,
        message: 'Conflict',
        errors: { mount: { _errors: [{ code, message: 'x' }] } },
      },
      { status: 409 },
    );
  const base = '*/api/v1/storage/mounts/501';
  let upload = 0;
  let size = 0;
  let key = '';
  server.use(
    http.get(`${base}/objects`, ({ request }) => {
      const prefix = new URL(request.url).searchParams.get('prefix') ?? '';
      const folders = new Set<string>();
      const objects: { key: string; size: number; last_modified: null }[] = [];
      for (const [k, n] of w.keys) {
        if (!k.startsWith(prefix) || k === prefix) continue;
        const rest = k.slice(prefix.length);
        const i = rest.indexOf('/');
        if (i >= 0) folders.add(prefix + rest.slice(0, i + 1));
        else objects.push({ key: k, size: n, last_modified: null });
      }
      return HttpResponse.json({
        folders: [...folders],
        objects,
        next_cursor: null,
        can_upload: w.canUpload,
        can_manage: w.canManage,
      });
    }),
    http.post(`${base}/folders`, async ({ request }) => {
      const body = (await request.json()) as { key: string };
      w.calls.push({ method: 'POST', path: 'folders', body });
      w.keys.set(body.key, 0);
      return HttpResponse.json({ key: body.key }, { status: 201 });
    }),
    http.post(`${base}/move`, async ({ request }) => {
      const body = (await request.json()) as { from: string; to: string };
      w.calls.push({ method: 'POST', path: 'move', body });
      if ([...w.keys.keys()].some((k) => k.startsWith(body.to))) return conflict('TARGET_EXISTS');
      let count = 0;
      for (const [k, n] of [...w.keys]) {
        if (k === body.from || (body.from.endsWith('/') && k.startsWith(body.from))) {
          w.keys.delete(k);
          w.keys.set(body.to + k.slice(body.from.length), n);
          count += 1;
        }
      }
      return HttpResponse.json({ count });
    }),
    http.delete(`${base}/objects/*`, ({ request }) => {
      const key = decodeURIComponent(new URL(request.url).pathname.split('/objects/')[1] ?? '');
      w.calls.push({ method: 'DELETE', path: key });
      let count = 0;
      for (const k of [...w.keys.keys()]) {
        if (k === key || (key.endsWith('/') && k.startsWith(key))) {
          w.keys.delete(k);
          count += 1;
        }
      }
      return count ? HttpResponse.json({ count }) : new HttpResponse(null, { status: 404 });
    }),
    http.post(`${base}/uploads`, async ({ request }) => {
      const body = (await request.json()) as { key: string; size: number };
      w.calls.push({ method: 'POST', path: 'uploads', body });
      upload += 1;
      size = body.size;
      key = body.key;
      return HttpResponse.json(
        {
          id: `u${upload}`,
          key: body.key,
          size: body.size,
          part_size: 4,
          part_count: Math.ceil(body.size / 4),
          content_type: 'text/plain',
          created_at: '2026-10-27T00:00:00Z',
        },
        { status: 201 },
      );
    }),
    http.post(`${base}/uploads/:id/parts/:n`, ({ params }) =>
      HttpResponse.json({
        part_number: Number(params.n),
        size: 4,
        url: `https://bucket.example/part/${String(params.id)}/${String(params.n)}`,
        expires_at: '2030-01-01T00:00:00Z',
      }),
    ),
    http.put('https://bucket.example/part/:id/:n', ({ request }) => {
      w.parts.push(new URL(request.url).pathname);
      return new HttpResponse(null, { status: 200 });
    }),
    http.post(`${base}/uploads/:id/complete`, () => {
      w.calls.push({ method: 'POST', path: 'complete' });
      w.keys.set(key, size);
      return HttpResponse.json({ key, size });
    }),
    http.get(`${base}/uploads/:id`, () => new HttpResponse(null, { status: 404 })),
  );
  return w;
}

const instant = async () => undefined;
const freshManager = () => {
  const m = new MountUploadManager(realMountUploadApi, new MemoryLedger<MountUploadRow>(), {
    wait: instant,
  });
  setMountUploads(m);
  return m;
};

afterEach(() => setMountUploads(null));

describe('Bucket browser write side (BYOS-BRW-001, against an in-memory server)', () => {
  it('shows no write controls on a shelf the caller can only read', async () => {
    world({ canUpload: false, canManage: false }, { 'readme.txt': 5 });
    freshManager();
    open('/storage/501');
    expect(await screen.findByText('readme.txt')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'New folder' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Upload files' })).toBeNull();
    expect(screen.queryByRole('button', { name: /Delete readme/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /Rename or move readme/ })).toBeNull();
  });

  it('lets an uploader upload and make folders but not rename or delete', async () => {
    world({ canManage: false }, { 'readme.txt': 5 });
    freshManager();
    open('/storage/501');
    expect(await screen.findByRole('button', { name: 'Upload files' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'New folder' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Delete readme/ })).toBeNull();
  });

  it('makes a folder inside the folder being viewed and shows it', async () => {
    const user = userEvent.setup();
    const w = world({}, { 'Music/a.mp3': 5 });
    freshManager();
    open('/storage/501?prefix=Music%2F');
    await screen.findByText('a.mp3');
    await user.click(screen.getByRole('button', { name: 'New folder' }));
    const dialog = await screen.findByRole('dialog', { name: 'New folder' });
    await user.type(within(dialog).getByLabelText('Folder name'), 'Live');
    await user.click(within(dialog).getByRole('button', { name: 'Create' }));
    expect(await screen.findByRole('link', { name: 'Live/' })).toBeInTheDocument();
    expect(w.calls).toContainEqual({
      method: 'POST',
      path: 'folders',
      body: { key: 'Music/Live/' },
    });
  });

  it('refuses a folder name with a slash before anything is sent', async () => {
    const user = userEvent.setup();
    const w = world();
    freshManager();
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'New folder' }));
    const dialog = await screen.findByRole('dialog', { name: 'New folder' });
    await user.type(within(dialog).getByLabelText('Folder name'), 'a/b');
    await user.click(within(dialog).getByRole('button', { name: 'Create' }));
    expect(await within(dialog).findByText(/no slash/)).toBeInTheDocument();
    expect(w.calls).toEqual([]);
  });

  it('uploads a file in parts, shows the dock, and lists the new file', async () => {
    const user = userEvent.setup();
    const w = world({}, { 'old.txt': 1 });
    freshManager();
    open('/storage/501');
    await screen.findByText('old.txt');
    await user.upload(
      screen.getByTestId('sto-pick-files'),
      new File(['abcdefghij'], 'new.txt', { type: 'text/plain' }),
    );
    const dock = await screen.findByRole('region', { name: 'Uploads' });
    expect(within(dock).getByText('new.txt')).toBeInTheDocument();
    await waitFor(() => expect(within(dock).getByText('Done')).toBeInTheDocument());
    // 10 bytes at 4 per part: three parts went to the bucket, then the server completed.
    expect(w.parts.sort()).toEqual(['/part/u1/1', '/part/u1/2', '/part/u1/3']);
    expect(w.calls.map((c) => c.path)).toEqual(['uploads', 'complete']);
    expect(await screen.findByText('new.txt', { selector: 'td' })).toBeInTheDocument();
    await user.click(within(dock).getByRole('button', { name: 'Clear finished' }));
    expect(screen.queryByRole('region', { name: 'Uploads' })).toBeNull();
  });

  it('puts a picked folder under the folder being viewed and skips empty files', async () => {
    const user = userEvent.setup();
    const w = world();
    freshManager();
    open('/storage/501?prefix=Music%2F');
    await screen.findByRole('button', { name: 'Upload a folder' });
    const full = new File(['abcd'], 'one.flac');
    Object.defineProperty(full, 'webkitRelativePath', { value: 'Album/one.flac' });
    const empty = new File([], 'zero.flac');
    Object.defineProperty(empty, 'webkitRelativePath', { value: 'Album/zero.flac' });
    await user.upload(screen.getByTestId('sto-pick-folder'), [full, empty]);
    expect(await screen.findByText(/1 item was skipped/)).toBeInTheDocument();
    await waitFor(() => expect(w.calls.some((c) => c.path === 'complete')).toBe(true));
    const begin = w.calls.find((c) => c.path === 'uploads')?.body as { key: string };
    expect(begin.key).toBe('Music/Album/one.flac');
    expect(w.calls.filter((c) => c.path === 'uploads')).toHaveLength(1);
  });

  it('renames a file and moves a folder through one dialog', async () => {
    const user = userEvent.setup();
    const w = world({}, { 'readme.txt': 5, 'Live/set.mp3': 9 });
    freshManager();
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'Rename or move readme.txt' }));
    const dialog = await screen.findByRole('dialog', { name: 'Rename or move readme.txt' });
    const input = within(dialog).getByLabelText('New path');
    expect(input).toHaveValue('readme.txt');
    await user.clear(input);
    await user.type(input, 'docs/notes.txt');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('readme.txt was moved.')).toBeInTheDocument();
    expect(w.calls).toContainEqual({
      method: 'POST',
      path: 'move',
      body: { from: 'readme.txt', to: 'docs/notes.txt' },
    });
    // A folder keeps its trailing slash.
    await user.click(await screen.findByRole('button', { name: 'Rename or move Live' }));
    const d2 = await screen.findByRole('dialog', { name: 'Rename or move Live' });
    const input2 = within(d2).getByLabelText('New path');
    expect(input2).toHaveValue('Live');
    await user.clear(input2);
    await user.type(input2, 'Sets');
    await user.click(within(d2).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect([...w.keys.keys()]).toContain('Sets/set.mp3'));
    expect(w.calls).toContainEqual({
      method: 'POST',
      path: 'move',
      body: { from: 'Live/', to: 'Sets/' },
    });
  });

  it('says why a move was refused, in words, from the server code', async () => {
    const user = userEvent.setup();
    world({}, { 'a.txt': 1, 'b.txt': 2 });
    freshManager();
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'Rename or move a.txt' }));
    const dialog = await screen.findByRole('dialog', { name: 'Rename or move a.txt' });
    const input = within(dialog).getByLabelText('New path');
    await user.clear(input);
    await user.type(input, 'b.txt');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(
      await within(dialog).findByText('Something with that name is already there.'),
    ).toBeInTheDocument();
  });

  it('asks before deleting and says what was deleted', async () => {
    const user = userEvent.setup();
    const w = world({}, { 'a.txt': 1, 'Live/x.mp3': 2, 'Live/y.mp3': 3 });
    freshManager();
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'Delete a.txt' }));
    expect(w.calls).toEqual([]);
    const confirm = await screen.findByRole('dialog', { name: 'Delete a.txt?' });
    expect(within(confirm).getByText(/no undo/)).toBeInTheDocument();
    await user.click(within(confirm).getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText('Deleted 1 item.')).toBeInTheDocument();
    expect(w.calls).toContainEqual({ method: 'DELETE', path: 'a.txt' });

    await user.click(await screen.findByRole('button', { name: 'Delete Live' }));
    const c2 = await screen.findByRole('dialog', { name: 'Delete Live?' });
    expect(within(c2).getByText(/everything in it/)).toBeInTheDocument();
    await user.click(within(c2).getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText('Deleted 2 items.')).toBeInTheDocument();
    expect(w.calls).toContainEqual({ method: 'DELETE', path: 'Live/' });
    await waitFor(() => expect(screen.queryByRole('link', { name: 'Live/' })).toBeNull());
  });
});

describe('Bucket browser write side accessibility (axe)', () => {
  it('has no violations with the controls, the dock and a dialog open', async () => {
    const user = userEvent.setup();
    world({}, { 'readme.txt': 5, 'Live/x.mp3': 2 });
    freshManager();
    open('/storage/501');
    await screen.findByText('readme.txt');
    await user.upload(
      screen.getByTestId('sto-pick-files'),
      new File(['abcd'], 'new.txt', { type: 'text/plain' }),
    );
    await screen.findByRole('region', { name: 'Uploads' });
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Rename or move readme.txt' }));
    await screen.findByRole('dialog', { name: 'Rename or move readme.txt' });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
