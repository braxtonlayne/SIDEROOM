// Wave 76 (SEC-AUD-298, D-794): one bar per upload; the overall bar only for two or more.
import { render, screen } from '@testing-library/react';
import type { MultipartUploadSlot } from '../apiP13';
import { UploadDock } from '../UploadDock';
import { MemoryLedger, UploadManager } from '../uploadManager';

/** A manager whose uploads never finish, so every task stays running. */
function slowManager() {
  let n = 0;
  return new UploadManager(
    {
      begin: async () => {
        n += 1;
        return {
          upload: { id: `u${n}` },
          multipart: { part_size: 4, part_count: 1, parts: [{ part_number: 1, url: 'u' }] },
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
    },
    new MemoryLedger(),
    {
      put: (_u, _b, signal) =>
        new Promise((_, rej) => signal.addEventListener('abort', () => rej(new Error('x')))),
    },
  );
}

describe('upload dock bars (D-794)', () => {
  it('draws exactly one bar for a single upload, and no "All uploads" line', async () => {
    const m = slowManager();
    render(<UploadDock manager={m} />);
    void m.enqueue('1', '2', new File(['abcd'], 'One.cbz')).catch(() => undefined);
    await screen.findByText('One.cbz');
    expect(document.querySelectorAll('progress')).toHaveLength(1);
    expect(screen.queryByText(/All uploads/)).toBeNull();
    expect(screen.getByRole('progressbar', { name: /One\.cbz, \d+%/ })).toBeInTheDocument();
  });

  it('draws the overall bar with its visible line for two uploads, named once', async () => {
    const m = slowManager();
    render(<UploadDock manager={m} />);
    void m.enqueue('1', '2', new File(['abcd'], 'One.cbz')).catch(() => undefined);
    void m.enqueue('1', '3', new File(['abcd'], 'Two.cbz')).catch(() => undefined);
    await screen.findByText('Two.cbz');
    expect(document.querySelectorAll('progress')).toHaveLength(3);
    expect(screen.getByText(/^All uploads: \d+%$/)).toBeVisible();
    // One accessible name for the overall bar, from the visible line.
    expect(screen.getAllByRole('progressbar', { name: /^All uploads: \d+%$/ })).toHaveLength(1);
  });
});
