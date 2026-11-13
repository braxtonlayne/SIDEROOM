import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryLedger } from '../../review/uploadManager';
import {
  PackageCancelled,
  PackageRefused,
  PackageUploader,
  choosePath,
  formatBytes,
  limitLine,
  type PackageLimits,
  type PackageProgress,
  type UploadStatus,
} from '../packageUpload';
import { http, HttpResponse, server, signedIn } from './helpers';

beforeEach(signedIn);

const MIB = 1024 * 1024;
const limits = (over: Partial<PackageLimits> = {}): PackageLimits => ({
  max_bytes: String(512 * MIB),
  max_files: 20000,
  tier: 'standard',
  resumable: true,
  part_size: 4,
  ...over,
});
const file = (n: number) => new File([new Uint8Array(n)], 'game.zip', { type: 'application/zip' });
const url = (n: number) => `https://bucket.test/p${n}`;
const A = '*/api/v1/applications/500/activity';

/** A tiny server: 10-byte file, 4-byte parts (3 parts), reports `statuses` in turn once completed. */
function serve(opts: { received?: number[]; statuses?: Partial<UploadStatus>[] } = {}) {
  const log: string[] = [];
  const received = new Set(opts.received ?? []);
  let completed = false;
  const statuses = [...(opts.statuses ?? [])];
  const missing = () =>
    [1, 2, 3].filter((n) => !received.has(n)).map((n) => ({ part_number: n, url: url(n) }));
  server.use(
    http.get(`${A}/package-limits`, () => HttpResponse.json(limits())),
    http.post(`${A}/uploads`, () => {
      log.push('begin');
      return HttpResponse.json(
        {
          upload_id: '9',
          part_size: 4,
          part_count: 3,
          parts: [1, 2, 3].map((n) => ({ part_number: n, url: url(n) })),
        },
        { status: 201 },
      );
    }),
    http.post(`${A}/uploads/9/complete`, () => {
      log.push('complete');
      completed = true;
      return HttpResponse.json(
        { state: 'checking', received: [1, 2, 3], checked_bytes: '0', total_bytes: '10' },
        { status: 202 },
      );
    }),
    http.get(`${A}/uploads/9`, () => {
      log.push('status');
      if (!completed)
        return HttpResponse.json({
          state: 'uploading',
          received: [...received],
          parts: missing(),
          checked_bytes: '0',
          total_bytes: '10',
        });
      const s = statuses.length > 1 ? statuses.shift()! : statuses[0]!;
      return HttpResponse.json({
        received: [1, 2, 3],
        checked_bytes: '0',
        total_bytes: '10',
        ...s,
      });
    }),
    http.delete(`${A}/uploads/9`, () => {
      log.push('delete');
      return new HttpResponse(null, { status: 204 });
    }),
    http.get(`${A}/bundles/77`, () =>
      HttpResponse.json({ bundle: { id: '77', entry_count: 2, byte_size: 10 }, entries: [] }),
    ),
  );
  return { log, received };
}

function uploader(put: (u: string, b: Blob) => Promise<void>, ledger = new MemoryLedger()) {
  return {
    ledger,
    up: new PackageUploader({
      ledger,
      put: async (u, b) => put(u, b),
      wait: async () => undefined,
      poll: async () => undefined,
      singleMax: 4,
    }),
  };
}

describe('the path a file takes', () => {
  it('chooses by size and by what the server can do', () => {
    expect(choosePath(10, limits())).toBe('single');
    expect(choosePath(33 * MIB, limits())).toBe('parts');
    expect(choosePath(33 * MIB, limits({ resumable: false }))).toBe('no-parts');
    expect(choosePath(600 * MIB, limits())).toBe('over-limit');
    expect(choosePath(600 * MIB, limits({ resumable: false }))).toBe('over-limit');
  });

  it('builds the limit line from the answer, not from a constant', () => {
    expect(limitLine(limits())).toContain('512 MiB');
    expect(limitLine(limits({ max_bytes: String(4096 * MIB), tier: 'reviewed' }))).toContain(
      '4 GiB',
    );
    expect(formatBytes(1536)).toBe('1.5 KiB');
  });

  it('refuses a file over the limit in the browser, naming the limit and the tier', async () => {
    serve();
    const { up } = uploader(async () => undefined);
    const big = file(4);
    Object.defineProperty(big, 'size', { value: 600 * MIB });
    const e = await up.upload('500', big, () => undefined).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(PackageRefused);
    expect((e as Error).message).toMatch(/512 MiB/);
    expect((e as Error).message).toMatch(/standard limit/);
    expect((e as Error).message).toMatch(/operator of this server can raise/);
  });

  it('says the storage cannot take large packages when parts are not available', async () => {
    server.use(
      http.get(`${A}/package-limits`, () => HttpResponse.json(limits({ resumable: false }))),
    );
    const { up } = uploader(async () => undefined);
    await expect(up.upload('500', file(10), () => undefined)).rejects.toThrow(
      /storage cannot take large packages/,
    );
  });

  it('sends a small file in one request, through the existing route', async () => {
    const log: string[] = [];
    server.use(
      http.get(`${A}/package-limits`, () => HttpResponse.json(limits())),
      http.post(`${A}/uploads`, () => {
        log.push('begin');
        return HttpResponse.json({}, { status: 500 });
      }),
    );
    const single = vi.fn(async () => ({ bundle: { id: '5' }, entries: [] }));
    vi.resetModules();
    vi.doMock('../api', async (orig) => ({
      ...(await orig<typeof import('../api')>()),
      uploadActivityBundle: single,
    }));
    const m = await import('../packageUpload');
    const up = new m.PackageUploader({ ledger: new MemoryLedger(), singleMax: 100 });
    await up.upload('500', file(10), () => undefined);
    expect(single).toHaveBeenCalledTimes(1);
    expect(log).toEqual([]);
    vi.doUnmock('../api');
  });
});

describe('the parts route', () => {
  it('sends the parts, completes, polls while checking and ends with the bundle', async () => {
    const s = serve({
      statuses: [
        { state: 'checking', checked_bytes: '4' },
        { state: 'ready', bundle_id: '77' },
      ],
    });
    const sent: string[] = [];
    const { up, ledger } = uploader(async (u) => void sent.push(u));
    const seen: PackageProgress[] = [];
    const detail = await up.upload('500', file(10), (p) => seen.push(p));
    expect(detail.bundle.id).toBe('77');
    expect(sent.sort()).toEqual([url(1), url(2), url(3)]);
    expect(s.log.filter((x) => x === 'complete')).toHaveLength(1);
    expect(seen.some((p) => p.phase === 'sending' && p.sent === 10)).toBe(true);
    expect(seen.some((p) => p.phase === 'checking' && p.checked === 4)).toBe(true);
    expect(await ledger.all()).toEqual([]);
  });

  it('retries a failed part, then goes on', async () => {
    serve({ statuses: [{ state: 'ready', bundle_id: '77' }] });
    let failed = false;
    const calls: string[] = [];
    const { up } = uploader(async (u) => {
      calls.push(u);
      if (u === url(2) && !failed) {
        failed = true;
        throw new Error('The storage answered 503.');
      }
    });
    await up.upload('500', file(10), () => undefined);
    expect(calls.filter((c) => c === url(2))).toHaveLength(2);
  });

  it('surfaces the server text of a refused package', async () => {
    serve({
      statuses: [{ state: 'refused', error: 'levels/a.exe: this file type is not allowed' }],
    });
    const { up } = uploader(async () => undefined);
    await expect(up.upload('500', file(10), () => undefined)).rejects.toThrow(
      'levels/a.exe: this file type is not allowed',
    );
  });

  it('resumes from the ledger and sends only the missing parts', async () => {
    const s = serve({ received: [1, 2], statuses: [{ state: 'ready', bundle_id: '77' }] });
    const ledger = new MemoryLedger();
    await ledger.put({
      id: '9',
      submissionId: '500',
      chapterId: 'package',
      uploadId: '9',
      filename: 'game.zip',
      size: 10,
      partSize: 4,
      partCount: 3,
      done: [1, 2],
      state: 'running',
      file: file(10),
    });
    const sent: string[] = [];
    const { up } = uploader(async (u) => void sent.push(u), ledger);
    const r = await up.resume('500', () => undefined);
    expect(r?.filename).toBe('game.zip');
    const detail = await r!.bundle;
    expect(sent).toEqual([url(3)]);
    expect(s.log).toContain('complete');
    expect(detail.bundle.id).toBe('77');
    expect(await ledger.all()).toEqual([]);
  });

  it('has nothing to resume without a ledger row, and drops a row the server forgot', async () => {
    const { up, ledger } = uploader(async () => undefined);
    expect(await up.resume('500', () => undefined)).toBeNull();
    server.use(http.get(`${A}/uploads/9`, () => HttpResponse.json({}, { status: 404 })));
    await ledger.put({
      id: '9',
      submissionId: '500',
      chapterId: 'package',
      uploadId: '9',
      filename: 'g.zip',
      size: 10,
      partSize: 4,
      partCount: 3,
      done: [],
      state: 'running',
      file: file(10),
    });
    expect(await up.resume('500', () => undefined)).toBeNull();
    expect(await ledger.all()).toEqual([]);
  });

  it('cancel aborts the upload at the server and forgets it', async () => {
    const s = serve({ statuses: [{ state: 'checking' }] });
    let release: () => void = () => undefined;
    const gate = new Promise<void>((r) => (release = r));
    const { up, ledger } = uploader(async () => gate);
    const run = up.upload('500', file(10), () => undefined).catch((e: unknown) => e);
    await vi.waitFor(() => expect(s.log).toContain('begin'));
    await vi.waitFor(() => expect(up.canCancel()).toBe(true));
    await up.cancel();
    release();
    expect(await run).toBeInstanceOf(PackageCancelled);
    expect(s.log).toContain('delete');
    expect(await ledger.all()).toEqual([]);
  });
});
