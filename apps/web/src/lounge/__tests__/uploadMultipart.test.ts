import { http, HttpResponse, server } from '../../test/server';
import { resumeKey, uploadFiles, type PendingFile, type UploadHandlers } from '../msg/upload';

// 10 bytes cut into parts of 4: 4 + 4 + 2.
const BYTES = '0123456789';
const mkFile = () => new File([BYTES], 'film.bin', { type: 'video/mp4', lastModified: 1234 });
const pending = (file: File): PendingFile => ({
  id: 'f1',
  file,
  name: file.name,
  spoiler: false,
  description: '',
  progress: 0,
  status: 'queued',
});
const readBlob = (b: Blob) =>
  new Promise<string>((res) => {
    const r = new FileReader();
    r.onload = () => res(String(r.result));
    r.readAsText(b);
  });
const url = (n: number) => `https://storage.test/p${n}`;
const slotBody = {
  attachments: [
    {
      id: 'f1',
      upload_id: '77',
      upload_url: '',
      upload_filename: '77/film.bin',
      max_size: 100,
      multipart: {
        part_size: 4,
        part_count: 3,
        parts: [1, 2, 3].map((n) => ({ part_number: n, url: url(n) })),
      },
    },
  ],
};

/** Records what the fake server saw. */
function fakeApi(parts?: () => Record<string, unknown>) {
  const calls: string[] = [];
  server.use(
    http.post('*/api/v1/channels/c1/attachments', () => {
      calls.push('slots');
      return HttpResponse.json(slotBody);
    }),
    http.get('*/api/v1/channels/c1/attachments/77/parts', () => {
      calls.push('parts');
      return HttpResponse.json(parts ? parts() : {});
    }),
    http.post('*/api/v1/channels/c1/attachments/77/complete', () => {
      calls.push('complete');
      return HttpResponse.json({ id: '77' });
    }),
    http.delete('*/api/v1/channels/c1/attachments/77', () => {
      calls.push('abort');
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

function handlers(
  putPart: UploadHandlers['putPart'],
  signal = new AbortController().signal,
): UploadHandlers {
  return { onUpdate: vi.fn(), signal, putPart, wait: async () => undefined };
}

const RESUME = 'sr.upload.resume';
const record = (file: File) =>
  JSON.stringify({
    [resumeKey('c1', file)]: {
      uploadId: '77',
      uploadFilename: '77/film.bin',
      maxSize: 100,
      savedAt: Date.now(),
    },
  });

beforeEach(() => localStorage.clear());

describe('Resumable multipart upload (LNG-CMP-007)', () => {
  it('cuts the file into the announced parts, completes, and forgets the resume record', async () => {
    const calls = fakeApi();
    const sent = new Map<string, string>();
    const put = vi.fn(async (u: string, body: Blob) => {
      sent.set(u, await readBlob(body));
    });
    const out = await uploadFiles('c1', [pending(mkFile())], handlers(put));
    expect(out).toEqual([{ id: 'f1', uploadedFilename: '77/film.bin' }]);
    expect(sent.get(url(1))).toBe('0123');
    expect(sent.get(url(2))).toBe('4567');
    expect(sent.get(url(3))).toBe('89');
    expect(calls).toEqual(['slots', 'complete']);
    expect(localStorage.getItem(RESUME)).toBe('{}');
  });

  it('retries a failed part, then asks the server which parts arrived when it keeps failing', async () => {
    const calls = fakeApi(() => ({
      part_size: 4,
      part_count: 3,
      received: [
        { part_number: 1, size: 4 },
        { part_number: 3, size: 2 },
      ],
      missing: [{ part_number: 2, url: 'https://storage.test/fresh2' }],
    }));
    let failures = 0;
    const put = vi.fn(async (u: string) => {
      if (u === url(2)) {
        failures++;
        throw new Error('network');
      }
    });
    const progress: number[] = [];
    const h = handlers(put);
    h.onUpdate = (_id, p) => {
      if (p.progress !== undefined) progress.push(p.progress);
    };
    await uploadFiles('c1', [pending(mkFile())], h);
    expect(failures).toBe(3); // PART_RETRIES attempts, then the resume round
    expect(calls).toEqual(['slots', 'parts', 'complete']);
    expect(put.mock.calls.map((c) => c[0])).toContain('https://storage.test/fresh2');
    expect(progress.at(-1)).toBe(1);
  });

  it('gives up with a clear message after the resume rounds and keeps the record for a retry', async () => {
    fakeApi(() => ({
      part_size: 4,
      part_count: 3,
      received: [],
      missing: [1, 2, 3].map((n) => ({ part_number: n, url: url(n) })),
    }));
    const put = vi.fn(async () => {
      throw new Error('offline');
    });
    const err = await uploadFiles('c1', [pending(mkFile())], handlers(put)).catch(
      (e: unknown) => e,
    );
    expect((err as Error).message).toMatch(/interrupted/);
    const saved = JSON.parse(localStorage.getItem(RESUME) ?? '{}') as Record<
      string,
      { uploadId: string }
    >;
    expect(saved[resumeKey('c1', mkFile())]?.uploadId).toBe('77');
  });

  it('continues an earlier upload of the same file after a reload instead of requesting a new slot', async () => {
    const file = mkFile();
    localStorage.setItem(RESUME, record(file));
    const calls = fakeApi(() => ({
      part_size: 4,
      part_count: 3,
      received: [
        { part_number: 1, size: 4 },
        { part_number: 2, size: 4 },
      ],
      missing: [{ part_number: 3, url: url(3) }],
    }));
    const put = vi.fn<(u: string) => Promise<void>>(async () => undefined);
    const out = await uploadFiles('c1', [pending(file)], handlers(put));
    expect(out[0]?.uploadedFilename).toBe('77/film.bin');
    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0]?.[0]).toBe(url(3));
    expect(calls).toEqual(['parts', 'complete']);
  });

  it('drops a stale resume record when the server no longer knows the upload', async () => {
    const file = mkFile();
    localStorage.setItem(RESUME, record(file));
    const calls: string[] = [];
    server.use(
      http.get('*/api/v1/channels/c1/attachments/77/parts', () => {
        calls.push('parts');
        return HttpResponse.json({ message: 'gone' }, { status: 404 });
      }),
      http.post('*/api/v1/channels/c1/attachments', () => {
        calls.push('slots');
        return HttpResponse.json(slotBody);
      }),
      http.post('*/api/v1/channels/c1/attachments/77/complete', () => {
        calls.push('complete');
        return HttpResponse.json({ id: '77' });
      }),
    );
    await uploadFiles('c1', [pending(file)], handlers(vi.fn(async () => undefined)));
    expect(calls).toEqual(['parts', 'slots', 'complete']);
  });

  it('frees the stored parts when the user cancels', async () => {
    const calls = fakeApi();
    const ctl = new AbortController();
    const put = vi.fn(async () => {
      ctl.abort();
      throw new Error('aborted');
    });
    await expect(
      uploadFiles('c1', [pending(mkFile())], handlers(put, ctl.signal)),
    ).rejects.toThrow();
    await vi.waitFor(() => expect(calls).toContain('abort'));
    expect(localStorage.getItem(RESUME)).toBe('{}');
  });
});
