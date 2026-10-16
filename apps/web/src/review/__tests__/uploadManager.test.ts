import { describe, expect, it } from 'vitest';
import { fromFileList, groupByFolder, readDropped, uploadable } from '../folderDrop';
import type { MultipartUploadSlot } from '../apiP13';
import { MemoryLedger, UploadManager, type UploadApi } from '../uploadManager';

const PART = 4;
const bytes = (n: number) => new File([new Uint8Array(n)], 'Ch 1.cbz');

/** A fake server: 10 byte file, parts of 4 bytes, remembers which parts it received. */
function fakeServer(size: number) {
  const received = new Set<number>();
  const count = Math.ceil(size / PART);
  const urls = (only?: number[]) =>
    Array.from({ length: count }, (_, i) => i + 1)
      .filter((n) => !only || only.includes(n))
      .map((n) => ({ part_number: n, url: `https://s.test/p${n}` }));
  const calls: string[] = [];
  const api: UploadApi = {
    async begin() {
      calls.push('begin');
      return {
        upload: { id: '77' },
        multipart: { part_size: PART, part_count: count, parts: urls() },
      } as unknown as MultipartUploadSlot;
    },
    async parts() {
      calls.push('parts');
      return {
        part_size: PART,
        part_count: count,
        received: [...received].map((n) => ({ part_number: n, size: PART })),
        missing: urls([...Array(count).keys()].map((i) => i + 1).filter((n) => !received.has(n))),
        expires_in_secs: 1,
      };
    },
    async complete() {
      calls.push('complete');
      if (received.size !== count) throw new Error('missing parts');
    },
    async abort() {
      calls.push('abort');
    },
  };
  return { api, received, calls, count };
}

describe('UploadManager', () => {
  it('sends every part, at most four at once, then completes', async () => {
    const s = fakeServer(30); // 8 parts
    let inflight = 0;
    let peak = 0;
    const m = new UploadManager(s.api, new MemoryLedger(), {
      wait: async () => undefined,
      put: async (url) => {
        inflight++;
        peak = Math.max(peak, inflight);
        await new Promise((r) => setTimeout(r, 2));
        s.received.add(Number(url.split('/p')[1]));
        inflight--;
      },
    });
    await m.enqueue('1', '2', bytes(30));
    expect(peak).toBeLessThanOrEqual(4);
    expect(peak).toBeGreaterThan(1);
    expect(s.received.size).toBe(8);
    expect(s.calls.at(-1)).toBe('complete');
    expect(m.snapshot()[0]?.state).toBe('done');
  });

  it('retries a failing part and fails the upload when the retries run out', async () => {
    const s = fakeServer(8);
    let flaky = 0;
    const ok = new UploadManager(s.api, new MemoryLedger(), {
      wait: async () => undefined,
      put: async (url) => {
        if (url.endsWith('p1') && flaky++ < 2) throw new Error('network');
        s.received.add(Number(url.split('/p')[1]));
      },
    });
    await ok.enqueue('1', '2', bytes(8));
    expect(flaky).toBe(3);

    const bad = new UploadManager(fakeServer(8).api, new MemoryLedger(), {
      retries: 1,
      wait: async () => undefined,
      put: async () => {
        throw new Error('storage down');
      },
    });
    await expect(bad.enqueue('1', '2', bytes(8))).rejects.toThrow('storage down');
    expect(bad.snapshot()[0]?.state).toBe('failed');
  });

  it('resumes after a refresh from the ledger and sends only the missing parts', async () => {
    const s = fakeServer(16); // 4 parts
    const ledger = new MemoryLedger();
    const sent1: number[] = [];
    const first = new UploadManager(s.api, ledger, {
      wait: async () => undefined,
      put: async (url, _b, signal) => {
        const n = Number(url.split('/p')[1]);
        if (n > 2) {
          // The page is closed while parts 3 and 4 are on the way.
          await new Promise((_, rej) =>
            signal.addEventListener('abort', () => rej(new Error('gone'))),
          );
        }
        s.received.add(n);
        sent1.push(n);
      },
      concurrency: 4,
    });
    const run = first.enqueue('1', '2', bytes(16));
    run.catch(() => undefined);
    await new Promise((r) => setTimeout(r, 20));
    await first.pause('77');
    expect(sent1.sort()).toEqual([1, 2]);

    // A new page load: a new manager, the same ledger.
    const sent2: number[] = [];
    const second = new UploadManager(s.api, ledger, {
      wait: async () => undefined,
      put: async (url) => {
        const n = Number(url.split('/p')[1]);
        s.received.add(n);
        sent2.push(n);
      },
    });
    const tasks = await second.resumeAll();
    // The paused upload waits for the person.
    expect(tasks[0]?.state).toBe('paused');
    await second.resume('77');
    await new Promise((r) => setTimeout(r, 20));
    expect(sent2.sort()).toEqual([3, 4]);
    expect(s.calls).toContain('complete');
    expect(await ledger.all()).toEqual([]);
  });

  it('cancels: the server frees the parts and the ledger forgets the upload', async () => {
    const s = fakeServer(8);
    const ledger = new MemoryLedger();
    const m = new UploadManager(s.api, ledger, {
      wait: async () => undefined,
      put: (_u, _b, signal) =>
        new Promise((_, rej) => signal.addEventListener('abort', () => rej(new Error('stop')))),
    });
    const run = m.enqueue('1', '2', bytes(8));
    run.catch(() => undefined);
    await new Promise((r) => setTimeout(r, 10));
    await m.cancel('77');
    await expect(run).rejects.toBeTruthy();
    expect(s.calls).toContain('abort');
    expect(await ledger.all()).toEqual([]);
    expect(m.snapshot()).toEqual([]);
  });
});

describe('folder drop', () => {
  const file = (name: string) => new File(['x'], name);
  const dirEntry = (name: string, children: unknown[]) => ({
    isFile: false,
    isDirectory: true,
    name,
    createReader: () => {
      let given = false;
      return {
        readEntries: (ok: (e: unknown[]) => void) => {
          ok(given ? [] : children);
          given = true;
        },
      };
    },
  });
  const fileEntry = (name: string) => ({
    isFile: true,
    isDirectory: false,
    name,
    file: (ok: (f: File) => void) => ok(file(name)),
  });
  const item = (entry: unknown) =>
    ({
      kind: 'file',
      webkitGetAsEntry: () => entry,
      getAsFile: () => null,
    }) as unknown as DataTransferItem;

  it('walks dropped folders and keeps paths', async () => {
    const tree = dirEntry('Blue Hour', [
      fileEntry('Blue Hour c001.cbz'),
      dirEntry('extras', [fileEntry('notes.txt'), fileEntry('c002.cbz')]),
    ]);
    const got = await readDropped([item(tree)]);
    expect(got.map((g) => g.path).sort()).toEqual([
      'Blue Hour/Blue Hour c001.cbz',
      'Blue Hour/extras/c002.cbz',
      'Blue Hour/extras/notes.txt',
    ]);
    expect(uploadable(got)).toHaveLength(2);
    expect([...groupByFolder(got).keys()]).toEqual(['Blue Hour']);
  });

  it('reads a directory pick from webkitRelativePath', () => {
    const f = file('c1.cbz');
    Object.defineProperty(f, 'webkitRelativePath', { value: 'Series/c1.cbz' });
    expect(fromFileList([f])[0]?.path).toBe('Series/c1.cbz');
    expect(fromFileList([file('loose.cbz')])[0]?.path).toBe('loose.cbz');
  });
});
