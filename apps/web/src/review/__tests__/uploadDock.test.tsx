import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import type { MultipartUploadSlot } from '../apiP13';
import { FolderImport, UploadDock } from '../UploadDock';
import { MemoryLedger, UploadManager, type UploadApi } from '../uploadManager';

function manager(onEnqueue: (chapter: string, name: string) => void) {
  const api: UploadApi = {
    async begin(_s, chapter, b) {
      onEnqueue(chapter, b.filename);
      return {
        upload: { id: `u${chapter}` },
        multipart: {
          part_size: 4,
          part_count: 1,
          parts: [{ part_number: 1, url: 'https://s.test/p1' }],
        },
      } as unknown as MultipartUploadSlot;
    },
    parts: async () => ({
      part_size: 4,
      part_count: 1,
      received: [],
      missing: [],
      expires_in_secs: 1,
    }),
    complete: async () => ({}),
    abort: async () => ({}),
  };
  return new UploadManager(api, new MemoryLedger(), { put: async () => undefined });
}

describe('folder import', () => {
  it('reads the names into chapters, then creates and uploads them', async () => {
    const user = userEvent.setup();
    const begun: string[] = [];
    const created: unknown[] = [];
    server.use(
      http.post('*/api/v1/submissions/naming/parse', () =>
        HttpResponse.json({
          items: [
            {
              name: 'a',
              cinema: null,
              library: {
                role: 'archive',
                series: 'S',
                volume: null,
                chapter: 1,
                title: null,
                group: null,
                page: null,
                warnings: [],
              },
            },
            {
              name: 'b',
              cinema: null,
              library: {
                role: 'archive',
                series: 'S',
                volume: null,
                chapter: null,
                title: null,
                group: null,
                page: null,
                warnings: [],
              },
            },
          ],
        }),
      ),
      http.post('*/api/v1/submissions/5/library/chapters', async ({ request }) => {
        created.push(await request.json());
        return HttpResponse.json({ id: '90', number: 1 }, { status: 201 });
      }),
    );
    const added = vi.fn();
    render(
      <FolderImport
        submissionId="5"
        onAdded={added}
        manager={manager((c, n) => begun.push(`${c}:${n}`))}
      />,
    );
    const one = new File(['12345678'], 'S c001.cbz');
    const two = new File(['x'], 'S extras.cbz');
    Object.defineProperty(one, 'webkitRelativePath', { value: 'S/S c001.cbz' });
    Object.defineProperty(two, 'webkitRelativePath', { value: 'S/S extras.cbz' });
    await user.upload(screen.getByLabelText('Choose a folder'), [
      one,
      two,
      new File(['t'], 'notes.txt'),
    ]);
    expect(await screen.findByLabelText('Include S/S c001.cbz')).toBeChecked();
    expect(screen.getByLabelText('Chapter number of S/S c001.cbz')).toHaveValue('1');
    expect(screen.getByText('No chapter number in the name; it is left out')).toBeInTheDocument();
    expect(screen.queryByText('notes.txt')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Create 1 chapter and upload' }));
    await waitFor(() => expect(added).toHaveBeenCalled());
    expect(created).toEqual([{ number: 1, title: null, format: 'images' }]);
    await waitFor(() => expect(begun).toEqual(['90:S c001.cbz']));
  });

  it('lets the person adjust a row: number, title and exclusion (REV-ED-002)', async () => {
    const user = userEvent.setup();
    const created: unknown[] = [];
    const lib = (chapter: number | null) => ({
      name: 'x',
      cinema: null,
      library: {
        role: 'archive',
        series: 'S',
        volume: null,
        chapter,
        title: null,
        group: null,
        page: null,
        warnings: [],
      },
    });
    server.use(
      http.post('*/api/v1/submissions/naming/parse', () =>
        HttpResponse.json({ items: [lib(1), lib(null), lib(1)] }),
      ),
      http.post('*/api/v1/submissions/5/library/chapters', async ({ request }) => {
        created.push(await request.json());
        return HttpResponse.json({ id: String(90 + created.length), number: 1 }, { status: 201 });
      }),
    );
    render(<FolderImport submissionId="5" onAdded={() => undefined} manager={manager(() => 0)} />);
    const files = ['a.cbz', 'b.cbz', 'c.cbz'].map((n) => {
      const f = new File(['1'], n);
      Object.defineProperty(f, 'webkitRelativePath', { value: `S/${n}` });
      return f;
    });
    await user.upload(screen.getByLabelText('Choose a folder'), files);
    await screen.findByLabelText('Include S/a.cbz');
    // Two files claim chapter 1: the clash is reported and Create is held back.
    expect(screen.getByRole('alert')).toHaveTextContent('Two files share chapter number 1.');
    expect(screen.getByRole('button', { name: /Create 2 chapters/ })).toBeDisabled();
    // Number the unplaced file, title the first, leave the third out.
    await user.type(screen.getByLabelText('Chapter number of S/b.cbz'), '2');
    await user.type(screen.getByLabelText('Title of S/a.cbz'), 'Start');
    await user.click(screen.getByLabelText('Include S/c.cbz'));
    expect(screen.queryByRole('alert')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Create 2 chapters and upload' }));
    await waitFor(() => expect(created).toHaveLength(2));
    expect(created).toEqual([
      { number: 1, title: 'Start', format: 'images' },
      { number: 2, title: null, format: 'images' },
    ]);
  });
});

describe('upload dock', () => {
  it('shows progress with pause and cancel, and nothing when idle', async () => {
    const m = manager(() => undefined);
    const { container } = render(<UploadDock manager={m} />);
    expect(container).toBeEmptyDOMElement();
    // A put that never finishes keeps the task running.
    const slow = new UploadManager(
      {
        begin: async () =>
          ({
            upload: { id: '1' },
            multipart: { part_size: 4, part_count: 1, parts: [{ part_number: 1, url: 'u' }] },
          }) as unknown as MultipartUploadSlot,
        parts: async () => ({
          part_size: 4,
          part_count: 1,
          received: [],
          missing: [],
          expires_in_secs: 1,
        }),
        complete: async () => ({}),
        abort: async () => ({}),
      },
      new MemoryLedger(),
      {
        put: (_u, _b, signal) =>
          new Promise((_, rej) => signal.addEventListener('abort', () => rej(new Error('x')))),
      },
    );
    render(<UploadDock manager={slow} />);
    void slow.enqueue('1', '2', new File(['abcd'], 'Big.cbz')).catch(() => undefined);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Pause Big.cbz' }));
    expect(await screen.findByText('Paused')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Resume Big.cbz' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel Big.cbz' }));
    await waitFor(() => expect(screen.queryByText('Big.cbz')).toBeNull());
  });
});
