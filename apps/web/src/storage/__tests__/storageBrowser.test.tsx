import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { byosMockHandlers } from '../../settings/user/storage/mocks';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { previewKind } from '../StorageBrowser';

const open = (path: string) => {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
};
const OBJECTS = {
  '501': ['cover.png', 'readme.txt', 'music/track.flac', 'music/live/set.mp3'],
};

describe('Bucket browser (BYOS-UI-001, against MSW mocks)', () => {
  it('classifies previewable files', () => {
    expect(previewKind('a/b.PNG')).toBe('image');
    expect(previewKind('x.flac')).toBe('audio');
    expect(previewKind('notes.txt')).toBeNull();
  });

  it('lists folders and files, descends into a folder, and shows a breadcrumb', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ objects: OBJECTS }));
    open('/storage/501');
    expect(await screen.findByText('cover.png')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'music/' })).toBeInTheDocument();
    expect(screen.queryByText('track.flac')).toBeNull();
    await user.click(screen.getByRole('link', { name: 'music/' }));
    expect(await screen.findByText('track.flac')).toBeInTheDocument();
    const crumbs = screen.getByRole('navigation', { name: 'Folder path' });
    expect(within(crumbs).getByRole('link', { name: 'music' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'live/' })).toBeInTheDocument();
  });

  it('previews an image and an audio file with the presigned URL and its expiry', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ objects: OBJECTS }));
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'Preview cover.png' }));
    const img = await screen.findByRole('img', { name: 'cover.png' });
    expect(img.getAttribute('src')).toContain('https://bucket.example/cover.png');
    expect(screen.getByText(/Link expires/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Preview readme.txt' })).toBeNull();
  });

  it('copies a presigned link and tells when it stops working', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ objects: OBJECTS }));
    open('/storage/501');
    const link = await screen.findByRole('button', { name: 'Copy link to readme.txt' });
    await user.click(link);
    expect(
      await screen.findByText(/Link to readme.txt copied. It stops working at/),
    ).toBeInTheDocument();
    expect(await navigator.clipboard.readText()).toContain('X-Amz-Signature=mock');
  });

  it('explains a bucket whose CORS is not ready', async () => {
    server.use(...byosMockHandlers({ corsNotReady: true }));
    open('/storage/501');
    expect(await screen.findByText(/owner must apply CORS first/)).toBeInTheDocument();
  });

  it('pages with the cursor', async () => {
    const user = userEvent.setup();
    server.use(
      http.get('*/api/v1/storage/mounts/501/objects', ({ request }) => {
        const cursor = new URL(request.url).searchParams.get('cursor');
        return HttpResponse.json(
          cursor
            ? {
                folders: [],
                objects: [{ key: 'b.txt', size: 1, last_modified: null }],
                next_cursor: null,
              }
            : {
                folders: [],
                objects: [{ key: 'a.txt', size: 1, last_modified: null }],
                next_cursor: 'c1',
              },
        );
      }),
    );
    open('/storage/501');
    await screen.findByText('a.txt');
    await user.click(screen.getByRole('button', { name: 'Load more' }));
    await waitFor(() => expect(screen.getByText('b.txt')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();
  });
});

describe('Bucket browser accessibility (axe)', () => {
  it('has no violations with a listing and a preview', async () => {
    const user = userEvent.setup();
    server.use(...byosMockHandlers({ objects: OBJECTS }));
    open('/storage/501');
    await user.click(await screen.findByRole('button', { name: 'Preview cover.png' }));
    await screen.findByRole('img', { name: 'cover.png' });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
