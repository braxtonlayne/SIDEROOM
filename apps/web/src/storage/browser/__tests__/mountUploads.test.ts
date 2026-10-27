import { describe, expect, it } from 'vitest';
import { ApiError } from '../../../api/client';
import { MemoryLedger, type MountUploadRow } from '../../../lib/uploadLedger';
import type { PartUrl, UploadSlot, UploadStatus } from '../../api';
import { MountUploadManager, serverCode, type MountUploadApi } from '../mountUploads';

const PART = 4;
const MIB_FILE = (n: number, name = 'a.bin') => new File([new Uint8Array(n)], name);

/** A bucket and a server in memory: the upload ids it hands out, the parts that arrived, the objects made. */
function fake(opts: { partSize?: number; failPartOnce?: number; deny?: number } = {}) {
  const partSize = opts.partSize ?? PART;
  const state = {
    begun: [] as UploadSlot[],
    parts: new Map<string, Set<number>>(),
    sent: [] as { url: string; size: number }[],
    completed: [] as string[],
    aborted: [] as string[],
    urlsAsked: 0,
    open: 0,
    maxOpen: 0,
    failOnce: opts.failPartOnce,
    gone: new Set<string>(),
  };
  let n = 0;
  const api: MountUploadApi = {
    async begin(_m, b) {
      state.open += 1;
      state.maxOpen = Math.max(state.maxOpen, state.open);
      const id = `up${++n}`;
      const slot: UploadSlot = {
        id,
        key: b.key,
        size: b.size,
        part_size: partSize,
        part_count: Math.max(1, Math.ceil(b.size / partSize)),
        content_type: b.content_type ?? 'application/octet-stream',
        created_at: '2026-10-27T00:00:00Z',
      };
      state.begun.push(slot);
      state.parts.set(id, new Set());
      return slot;
    },
    async status(_m, id): Promise<UploadStatus> {
      if (state.gone.has(id)) throw new ApiError(404, 'Unknown resource');
      const slot = state.begun.find((s) => s.id === id)!;
      return {
        upload: slot,
        parts: [...(state.parts.get(id) ?? [])].map((number) => ({
          number,
          size: partSize,
          etag: `"e${number}"`,
        })),
      };
    },
    async partUrl(_m, id, part): Promise<PartUrl> {
      state.urlsAsked += 1;
      if (opts.deny) throw new ApiError(opts.deny, 'no');
      return {
        part_number: part,
        size: partSize,
        url: `https://bucket.test/${id}/${part}`,
        expires_at: 'x',
      };
    },
    async complete(_m, id) {
      const slot = state.begun.find((s) => s.id === id)!;
      if ((state.parts.get(id)?.size ?? 0) !== slot.part_count) {
        throw new ApiError(409, 'incomplete', {
          errors: { mount: { _errors: [{ code: 'UPLOAD_INCOMPLETE', message: 'x' }] } },
        });
      }
      state.completed.push(id);
      state.open -= 1;
    },
    async abort(_m, id) {
      state.aborted.push(id);
      state.open -= 1;
    },
  };
  const put = async (url: string, body: Blob) => {
    if (state.failOnce !== undefined && url.endsWith(`/${state.failOnce}`)) {
      state.failOnce = undefined;
      throw new Error('network');
    }
    const [, id, part] = /\/(up\d+)\/(\d+)$/.exec(url)!;
    state.parts.get(id!)!.add(Number(part));
    state.sent.push({ url, size: body.size });
  };
  return { api, state, put };
}

const instant = async () => undefined;

describe('serverCode', () => {
  it('reads the stable code from the field error or the flat body', () => {
    const nested = new ApiError(409, 'x', {
      code: 900010,
      errors: { mount: { _errors: [{ code: 'CORS_NOT_READY', message: 'x' }] } },
    } as never);
    expect(serverCode(nested)).toBe('CORS_NOT_READY');
    expect(
      serverCode(new ApiError(409, 'x', { message: 'x', code: 'TARGET_EXISTS' } as never)),
    ).toBe('TARGET_EXISTS');
    expect(serverCode(new Error('plain'))).toBeUndefined();
  });
});

describe('MountUploadManager (BYOS-BRW-001)', () => {
  it('cuts a file into parts, asks for each part URL when it is sent, and completes', async () => {
    const f = fake();
    const ledger = new MemoryLedger<MountUploadRow>();
    const m = new MountUploadManager(f.api, ledger, { put: f.put, wait: instant });
    await m.enqueue('9', 'Music/a.bin', MIB_FILE(10));
    expect(f.state.begun[0]).toMatchObject({ key: 'Music/a.bin', size: 10, part_count: 3 });
    expect(f.state.sent.map((s) => s.size).sort()).toEqual([2, 4, 4]);
    expect(f.state.urlsAsked).toBe(3);
    expect(f.state.completed).toEqual(['up1']);
    expect(m.snapshot()[0]).toMatchObject({ state: 'done', progress: 1, filename: 'a.bin' });
    expect(await ledger.all()).toEqual([]);
  });

  it('retries a failed part after a pause and still completes', async () => {
    const f = fake({ failPartOnce: 2 });
    const m = new MountUploadManager(f.api, new MemoryLedger(), { put: f.put, wait: instant });
    await m.enqueue('9', 'a.bin', MIB_FILE(10));
    expect(f.state.completed).toEqual(['up1']);
    // The URL is asked for again with the retry (a new one, not an expired one).
    expect(f.state.urlsAsked).toBe(4);
  });

  it('fails when the server refuses a part URL, and does not retry what retrying cannot fix', async () => {
    const f = fake({ deny: 429 });
    const m = new MountUploadManager(f.api, new MemoryLedger(), { put: f.put, wait: instant });
    await expect(m.enqueue('9', 'a.bin', MIB_FILE(10))).rejects.toThrow();
    expect(f.state.urlsAsked).toBeLessThanOrEqual(4);
    expect(m.snapshot()[0]?.state).toBe('failed');
  });

  it('keeps at most two files open at the server and runs a whole queue', async () => {
    const f = fake();
    const m = new MountUploadManager(f.api, new MemoryLedger(), { put: f.put, wait: instant });
    await Promise.all(
      Array.from({ length: 7 }, (_, i) => m.enqueue('9', `f/${i}.bin`, MIB_FILE(9, `${i}.bin`))),
    );
    expect(f.state.completed).toHaveLength(7);
    expect(f.state.maxOpen).toBeLessThanOrEqual(2);
  });

  it('resumes after a refresh from the ledger: only the missing parts are sent', async () => {
    const f = fake();
    const ledger = new MemoryLedger<MountUploadRow>();
    // First page: the upload begins and is cut off after part 1 (the put hangs until aborted).
    const slot = await f.api.begin('9', { key: 'big.bin', size: 12 });
    f.state.parts.get(slot.id)!.add(1);
    await ledger.put({
      id: slot.id,
      mountId: '9',
      key: 'big.bin',
      size: 12,
      partSize: PART,
      partCount: 3,
      done: [1],
      state: 'running',
      savedAt: 1,
      file: MIB_FILE(12, 'big.bin'),
    });
    // Second page: a new manager over the same ledger.
    const m = new MountUploadManager(f.api, ledger, { put: f.put, wait: instant });
    await m.resumeAll('9');
    await new Promise((r) => setTimeout(r, 20));
    expect(f.state.sent.map((s) => s.url.split('/').pop()).sort()).toEqual(['2', '3']);
    expect(f.state.completed).toEqual([slot.id]);
    expect(await ledger.all()).toEqual([]);
    expect(f.state.begun).toHaveLength(1);
  });

  it('forgets an upload the server no longer knows', async () => {
    const f = fake();
    const ledger = new MemoryLedger<MountUploadRow>();
    const slot = await f.api.begin('9', { key: 'old.bin', size: 8 });
    f.state.gone.add(slot.id);
    await ledger.put({
      id: slot.id,
      mountId: '9',
      key: 'old.bin',
      size: 8,
      partSize: PART,
      partCount: 2,
      done: [],
      state: 'running',
      savedAt: 1,
      file: MIB_FILE(8),
    });
    const m = new MountUploadManager(f.api, ledger, { put: f.put, wait: instant });
    await m.resumeAll('9');
    await new Promise((r) => setTimeout(r, 20));
    expect(m.snapshot()[0]?.state).toBe('failed');
    expect(await ledger.all()).toEqual([]);
  });

  it('only resumes the rows of the shelf being viewed', async () => {
    const f = fake();
    const ledger = new MemoryLedger<MountUploadRow>();
    await ledger.put({
      id: 'x1',
      mountId: '77',
      key: 'k',
      size: 4,
      partSize: PART,
      partCount: 1,
      done: [],
      state: 'paused',
      savedAt: 1,
      file: MIB_FILE(4),
    });
    const m = new MountUploadManager(f.api, ledger, { put: f.put, wait: instant });
    await m.resumeAll('9');
    expect(m.snapshot()).toEqual([]);
    await m.resumeAll('77');
    expect(m.snapshot()[0]).toMatchObject({ id: 'x1', state: 'paused' });
  });

  it('cancel aborts at the server and forgets the row; clearFinished drops done tasks', async () => {
    const f = fake();
    const ledger = new MemoryLedger<MountUploadRow>();
    let release: () => void = () => undefined;
    const gate = new Promise<void>((r) => (release = r));
    const slow = async (url: string, body: Blob, signal: AbortSignal) => {
      await Promise.race([
        gate,
        new Promise((_, rej) => signal.addEventListener('abort', () => rej(new Error('aborted')))),
      ]);
      await f.put(url, body);
    };
    const m = new MountUploadManager(f.api, ledger, { put: slow, wait: instant });
    const p = m.enqueue('9', 'a.bin', MIB_FILE(9)).catch(() => 'cancelled');
    await new Promise((r) => setTimeout(r, 10));
    const id = m.snapshot()[0]!.id;
    expect(id).toBe('up1');
    await m.cancel(id);
    expect(await p).toBe('cancelled');
    expect(f.state.aborted).toEqual(['up1']);
    expect(await ledger.all()).toEqual([]);
    expect(m.snapshot()).toEqual([]);
    release();

    const g = fake();
    const m2 = new MountUploadManager(g.api, new MemoryLedger(), { put: g.put, wait: instant });
    await m2.enqueue('9', 'b.bin', MIB_FILE(4));
    expect(m2.snapshot()).toHaveLength(1);
    m2.clearFinished();
    expect(m2.snapshot()).toEqual([]);
  });

  it('shows a refused completion in words through its code', async () => {
    const f = fake();
    // The bucket loses a part between the last PUT and the completion.
    const api: MountUploadApi = {
      ...f.api,
      complete: async () => {
        throw new ApiError(409, 'x', {
          errors: { mount: { _errors: [{ code: 'UPLOAD_INCOMPLETE', message: 'x' }] } },
        } as never);
      },
    };
    const m = new MountUploadManager(api, new MemoryLedger(), { put: f.put, wait: instant });
    await expect(m.enqueue('9', 'a.bin', MIB_FILE(4))).rejects.toThrow();
    expect(m.snapshot()[0]).toMatchObject({ state: 'failed', code: 'UPLOAD_INCOMPLETE' });
  });
});
