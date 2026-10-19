import { http, HttpResponse, server } from '../test/server';
import {
  CHAT_ROW_CAP,
  CHAT_ROW_MAX_AGE_MS,
  ChatUploads,
  MemoryLedger,
  UnavailableLedger,
  clearUploadsOnSignOut,
  setChatUploads,
  type ChatUploadRow,
  type Ledger,
} from './uploadLedger';

const HOUR = 3600 * 1000;
const row = (id: string, savedAt: number, channelId = 'c1'): ChatUploadRow => ({
  id,
  channelId,
  uploadFilename: `${id}/film.bin`,
  size: 10,
  partSize: 4,
  partCount: 3,
  done: [],
  savedAt,
  file: new File(['0123456789'], 'film.bin'),
});

function setup(now = { t: 1_000_000_000 }) {
  const ledger = new MemoryLedger<ChatUploadRow>();
  const aborted: string[] = [];
  const store = new ChatUploads(
    ledger,
    (r) => aborted.push(r.id),
    () => now.t,
  );
  return { ledger, aborted, store, now };
}

describe('chat upload ledger rules (D-271)', () => {
  it('keeps at most five rows and drops the oldest first, aborting its slot', async () => {
    const { store, aborted, now } = setup();
    for (let i = 1; i <= CHAT_ROW_CAP; i++) {
      now.t += 1000;
      expect(await store.add(row(`u${i}`, now.t))).toBe(true);
    }
    expect(aborted).toEqual([]);
    now.t += 1000;
    await store.add(row('u6', now.t));
    expect(aborted).toEqual(['u1']);
    const ids = (await store.forChannel('c1')).map((r) => r.id);
    expect(ids).toEqual(['u2', 'u3', 'u4', 'u5', 'u6']);
  });

  it('drops rows older than 20 hours on open and on every write', async () => {
    const { store, aborted, now, ledger } = setup();
    await store.add(row('old', now.t));
    now.t += CHAT_ROW_MAX_AGE_MS + HOUR;
    expect(await store.forChannel('c1')).toEqual([]);
    expect(aborted).toEqual(['old']);

    const fresh = row('fresh', now.t);
    await store.add(fresh);
    await ledger.put(row('stale', now.t - CHAT_ROW_MAX_AGE_MS - 1));
    await store.setDone(fresh, [1]);
    expect(aborted).toEqual(['old', 'stale']);
    expect((await ledger.all()).map((r) => r.id)).toEqual(['fresh']);
  });

  it('records the confirmed parts and does not bring back a removed row', async () => {
    const { store, ledger, now } = setup();
    const r = row('u1', now.t);
    await store.add(r);
    await store.setDone(r, [1, 3]);
    expect((await ledger.all())[0]?.done).toEqual([1, 3]);
    await store.remove('u1');
    expect(await store.setDone(r, [1, 2, 3])).toBe(false);
    expect(await ledger.all()).toEqual([]);
  });

  it('lists only the rows of the channel, oldest first', async () => {
    const { store, now } = setup();
    await store.add(row('b', now.t + 2, 'c1'));
    await store.add(row('a', now.t + 1, 'c1'));
    await store.add(row('x', now.t, 'c2'));
    expect((await store.forChannel('c1')).map((r) => r.id)).toEqual(['a', 'b']);
  });

  it('reports a failed write so the caller can use its localStorage record', async () => {
    const quota: Ledger<ChatUploadRow> = {
      async put() {
        return false;
      },
      async all() {
        return [];
      },
      async remove() {},
    };
    expect(await new ChatUploads(quota, () => {}).add(row('u1', Date.now()))).toBe(false);
    expect(await new UnavailableLedger<ChatUploadRow>().put()).toBe(false);
  });
});

describe('sign-out', () => {
  afterEach(() => setChatUploads(null));
  it('aborts the open chat slots and clears the store', async () => {
    const ledger = new MemoryLedger<ChatUploadRow>();
    const store = new ChatUploads(ledger, () => {});
    setChatUploads(store);
    await store.add(row('77', Date.now()));
    const deleted: string[] = [];
    server.use(
      http.delete('*/api/v1/channels/c1/attachments/77', () => {
        deleted.push('77');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await clearUploadsOnSignOut();
    expect(deleted).toEqual(['77']);
    expect(await ledger.all()).toEqual([]);
  });
});
