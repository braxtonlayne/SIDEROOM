// Wave 74 (SEC-AUD-293, D-781): the dock's row for an upload the server ended.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ApiError } from '../../api/client';
import type { MultipartUploadSlot } from '../apiP13';
import { UploadDock } from '../UploadDock';
import { MemoryLedger, UploadManager, type UploadApi } from '../uploadManager';

const ENDED = 'this upload is failed; start a new one';

function api(begun: string[]): UploadApi {
  let n = 0;
  return {
    async begin(_s, _c, b) {
      begun.push(b.filename);
      n += 1;
      return {
        upload: { id: `u${n}` },
        multipart: {
          part_size: 4,
          part_count: 1,
          parts: [{ part_number: 1, url: 'https://s/p1' }],
        },
      } as unknown as MultipartUploadSlot;
    },
    parts: async () => {
      throw new ApiError(409, ENDED);
    },
    complete: async (_s, id) => {
      if (id === 'u1') throw new ApiError(409, ENDED);
      return {};
    },
    abort: async () => ({}),
  };
}

describe('upload dock, an ended upload', () => {
  it('names the button "Upload {name} again", and one click starts a new upload', async () => {
    const begun: string[] = [];
    const m = new UploadManager(api(begun), new MemoryLedger(), { put: async () => undefined });
    render(<UploadDock manager={m} />);
    void m.enqueue('1', '2', new File(['abcd'], 'Big.cbz')).catch(() => undefined);
    const user = userEvent.setup();
    const again = await screen.findByRole('button', { name: 'Upload Big.cbz again' });
    expect(screen.queryByRole('button', { name: 'Retry Big.cbz' })).toBeNull();
    expect(screen.getByText(new RegExp(ENDED))).toBeInTheDocument();
    await user.click(again);
    await waitFor(() => expect(begun).toEqual(['Big.cbz', 'Big.cbz']));
    await waitFor(() => expect(screen.getByText('Uploaded')).toBeInTheDocument());
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
  });

  it('without the file, shows the sentence and only the remove button', async () => {
    const ledger = new MemoryLedger();
    await ledger.put({
      id: 'u9',
      submissionId: '1',
      chapterId: '2',
      uploadId: 'u9',
      filename: 'Big.cbz',
      size: 4,
      partSize: 4,
      partCount: 1,
      done: [],
      state: 'failed',
      error: ENDED,
      ended: true,
    });
    const m = new UploadManager(api([]), ledger, { put: async () => undefined });
    render(<UploadDock manager={m} />);
    expect(await screen.findByText('Choose the file again to upload it.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /again|Retry|Resume/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Cancel Big.cbz' })).toBeInTheDocument();
  });

  it('keeps "Retry {name}" for a failure that is not an ended upload', async () => {
    const m = new UploadManager({ ...api([]), complete: async () => ({}) }, new MemoryLedger(), {
      retries: 0,
      wait: async () => undefined,
      put: async () => {
        throw new Error('storage down');
      },
    });
    render(<UploadDock manager={m} />);
    void m.enqueue('1', '2', new File(['abcd'], 'Big.cbz')).catch(() => undefined);
    expect(await screen.findByRole('button', { name: 'Retry Big.cbz' })).toBeInTheDocument();
  });
});
