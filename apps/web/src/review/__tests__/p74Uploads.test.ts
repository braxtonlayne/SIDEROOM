// Wave 74 (SEC-AUD-293, D-781): an upload the server ended (409 at `parts` or `complete`) is not asked about
// again, by itself or from the button; the button starts a new upload of the file the page still holds.
import { describe, expect, it } from 'vitest';
import { ApiError } from '../../api/client';
import type { MultipartUploadSlot } from '../apiP13';
import { MemoryLedger, UploadManager, type LedgerEntry, type UploadApi } from '../uploadManager';

const ENDED = 'this upload is failed; start a new one';
const ended409 = () => new ApiError(409, ENDED, { message: ENDED });
const settle = () => new Promise((r) => setTimeout(r, 15));

/** A fake server: every slot is one 4 byte part. `ended` ids answer 409 at `parts`, `completeEnds` at `complete`. */
function fake(opts: { ended?: Set<string>; completeEnds?: Set<string> } = {}) {
  const calls: string[] = [];
  let n = 0;
  const control = { beginFails: null as Error | null };
  const api: UploadApi = {
    async begin(_s, _c, b) {
      calls.push('begin');
      if (control.beginFails) throw control.beginFails;
      n += 1;
      return {
        upload: { id: `u${n}` },
        multipart: {
          part_size: 4,
          part_count: 1,
          parts: [{ part_number: 1, url: `https://s.test/${b.filename}` }],
        },
      } as unknown as MultipartUploadSlot;
    },
    async parts(_s, id) {
      calls.push(`parts:${id}`);
      if (opts.ended?.has(id)) throw ended409();
      return { part_size: 4, part_count: 1, received: [], missing: [], expires_in_secs: 1 };
    },
    async complete(_s, id) {
      calls.push(`complete:${id}`);
      if (opts.completeEnds?.has(id)) throw ended409();
    },
    async abort(_s, id) {
      calls.push(`abort:${id}`);
    },
  };
  return { api, calls, control };
}

const file = () => new File(['abcd'], 'Ch 1.cbz');
const mk = (api: UploadApi, ledger = new MemoryLedger(), put = async () => undefined) =>
  new UploadManager(api, ledger, { wait: async () => undefined, put, retries: 0 });

const oldEntry = (over: Partial<LedgerEntry>): LedgerEntry => ({
  id: 'u9',
  submissionId: '1',
  chapterId: '2',
  uploadId: 'u9',
  filename: 'Ch 1.cbz',
  size: 4,
  partSize: 4,
  partCount: 1,
  done: [],
  state: 'failed',
  ...over,
});

describe('an upload the server ended (D-781)', () => {
  it('is not asked about again by resume; the button starts a new upload and the old row goes', async () => {
    const s = fake({ completeEnds: new Set(['u1']) });
    const m = mk(s.api);
    await expect(m.enqueue('1', '2', file())).rejects.toThrow(ENDED);
    const t = m.snapshot()[0]!;
    expect(t).toMatchObject({ state: 'failed', ended: true, hasFile: true, error: ENDED });
    s.calls.length = 0;
    await m.resume(t.id);
    await settle();
    expect(s.calls).not.toContain('parts:u1');
    expect(s.calls).not.toContain('abort:u1');
    expect(s.calls[0]).toBe('begin');
    expect(s.calls).toContain('complete:u2');
    const rows = m.snapshot();
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: 'u2', state: 'done' });
  });

  it('after a refresh, resumeAll leaves the ended entry failed, sends nothing, and cancel does not abort', async () => {
    const s = fake({ ended: new Set(['u9']) });
    const ledger = new MemoryLedger();
    await ledger.put(oldEntry({ error: ENDED, ended: true }));
    const m = mk(s.api, ledger);
    const tasks = await m.resumeAll();
    expect(s.calls).toEqual([]);
    expect(tasks).toHaveLength(1);
    expect(tasks[0]).toMatchObject({ state: 'failed', ended: true, hasFile: false, error: ENDED });
    await m.resume('u9');
    expect(s.calls).toEqual([]);
    expect(m.snapshot()).toHaveLength(1);
    await m.cancel('u9');
    expect(s.calls).toEqual([]);
    expect(m.snapshot()).toHaveLength(0);
    expect(await ledger.all()).toHaveLength(0);
  });

  it('marks an old entry ended when parts answers 409, and the next load does not ask again', async () => {
    const s = fake({ ended: new Set(['u5']) });
    const ledger = new MemoryLedger();
    await ledger.put(oldEntry({ id: 'u5', uploadId: 'u5', state: 'running', file: file() }));
    const m = mk(s.api, ledger);
    await m.resumeAll();
    await settle();
    expect(m.snapshot()[0]).toMatchObject({ state: 'failed', ended: true, error: ENDED });
    expect(s.calls.filter((c) => c === 'parts:u5')).toHaveLength(1);
    const m2 = mk(s.api, ledger);
    await m2.resumeAll();
    await settle();
    expect(s.calls.filter((c) => c === 'parts:u5')).toHaveLength(1);
    expect(m2.snapshot()[0]).toMatchObject({ ended: true, hasFile: false });
  });

  it('keeps the old row with the new error when the new upload cannot start', async () => {
    const s = fake({ completeEnds: new Set(['u1']) });
    const m = mk(s.api);
    await expect(m.enqueue('1', '2', file())).rejects.toThrow(ENDED);
    s.control.beginFails = new ApiError(409, 'this chapter already has an upload being converted');
    await m.resume('u1');
    const rows = m.snapshot();
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      id: 'u1',
      state: 'failed',
      ended: true,
      hasFile: true,
      error: 'this chapter already has an upload being converted',
    });
  });

  it('still retries the same upload for a failure that is not a 409', async () => {
    const s = fake();
    let fail = true;
    const m = mk(s.api, new MemoryLedger(), async () => {
      if (fail) throw new Error('storage down');
    });
    await expect(m.enqueue('1', '2', file())).rejects.toThrow('storage down');
    expect(m.snapshot()[0]).toMatchObject({ state: 'failed' });
    expect(m.snapshot()[0]!.ended).toBeUndefined();
    fail = false;
    await m.resume('u1');
    await settle();
    expect(s.calls).toContain('parts:u1');
    expect(s.calls.filter((c) => c === 'begin')).toHaveLength(1);
    expect(m.snapshot()[0]).toMatchObject({ id: 'u1', state: 'done' });
  });
});
