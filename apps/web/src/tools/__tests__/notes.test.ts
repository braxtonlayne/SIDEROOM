import { describe, expect, it } from 'vitest';
import { ApiError } from '../../api/client';
import {
  NotesCryptoError,
  createNotesKey,
  decryptNote,
  encryptNote,
  fromBase64,
  memoryKeys,
  openRecoveryBundle,
} from '../notes/crypto';
import { type NotesApi, type RemoteNote, createNotesStore } from '../notes/notesStore';

const ROUNDS = 1000; // the real thing is 600 000; the test is about the plumbing.
const apiError = (status: number) => new ApiError(status, 'x');

describe('notes cryptography', () => {
  it('encrypts and decrypts, and the ciphertext is not the text', async () => {
    const { key } = await createNotesKey('correct horse battery', ROUNDS);
    const ct = await encryptNote(key, 'buy milk and a secret', 'n1');
    expect(new TextDecoder('latin1').decode(ct)).not.toContain('secret');
    expect(await decryptNote(key, ct, 'n1')).toBe('buy milk and a secret');
    // Two encryptions of the same text differ (fresh IV).
    const again = await encryptNote(key, 'buy milk and a secret', 'n1');
    expect(Array.from(again)).not.toEqual(Array.from(ct));
  });

  it('a ciphertext opens only for its own note id, with its own key, undamaged', async () => {
    const { key } = await createNotesKey('passphrase one', ROUNDS);
    const other = await createNotesKey('passphrase two', ROUNDS);
    const ct = await encryptNote(key, 'text', 'n1');
    await expect(decryptNote(key, ct, 'n2')).rejects.toMatchObject({ problem: 'bad-ciphertext' });
    await expect(decryptNote(other.key, ct, 'n1')).rejects.toBeInstanceOf(NotesCryptoError);
    const bent = ct.slice();
    bent[bent.length - 1] = bent[bent.length - 1]! ^ 1;
    await expect(decryptNote(key, bent, 'n1')).rejects.toBeInstanceOf(NotesCryptoError);
    await expect(decryptNote(key, new Uint8Array(5), 'n1')).rejects.toBeInstanceOf(
      NotesCryptoError,
    );
    const wrongVersion = ct.slice();
    wrongVersion[0] = 9;
    await expect(decryptNote(key, wrongVersion, 'n1')).rejects.toBeInstanceOf(NotesCryptoError);
  });

  it('the key cannot be read back out of the browser', async () => {
    const { key } = await createNotesKey('passphrase one', ROUNDS);
    expect(key.extractable).toBe(false);
    await expect(crypto.subtle.exportKey('raw', key)).rejects.toBeDefined();
  });

  it('the recovery bundle opens with the passphrase and gives the same key', async () => {
    const { key, bundle } = await createNotesKey('open sesame please', ROUNDS);
    expect(JSON.stringify(bundle)).not.toContain('open sesame');
    const reopened = await openRecoveryBundle(
      JSON.parse(JSON.stringify(bundle)),
      'open sesame please',
    );
    expect(reopened.extractable).toBe(false);
    const ct = await encryptNote(key, 'carried over', 'n1');
    expect(await decryptNote(reopened, ct, 'n1')).toBe('carried over');
  });

  it('a wrong passphrase and a damaged bundle are refused, distinguishably from a bad file', async () => {
    const { bundle } = await createNotesKey('right one right one', ROUNDS);
    await expect(openRecoveryBundle(bundle, 'wrong one wrong one')).rejects.toMatchObject({
      problem: 'bad-passphrase',
    });
    await expect(openRecoveryBundle({ ...bundle, v: 2 }, 'x')).rejects.toMatchObject({
      problem: 'bad-bundle',
    });
    await expect(openRecoveryBundle(null, 'x')).rejects.toMatchObject({ problem: 'bad-bundle' });
    await expect(openRecoveryBundle({ ...bundle, rounds: 10 ** 9 }, 'x')).rejects.toMatchObject({
      problem: 'bad-bundle',
    });
    // A bundle whose wrapped key was tampered with fails its tag check.
    const w = fromBase64(bundle.wrapped);
    w[0] = w[0]! ^ 1;
    const tampered = { ...bundle, wrapped: btoa(String.fromCharCode(...w)) };
    await expect(openRecoveryBundle(tampered, 'right one right one')).rejects.toMatchObject({
      problem: 'bad-passphrase',
    });
  });
});

/** An in-memory stand-in for the server: it stores base64 strings and checks revisions like the real one. */
function fakeServer() {
  const rows = new Map<string, RemoteNote>();
  let clock = 0;
  const api: NotesApi & { rows: typeof rows; seen: string[] } = {
    rows,
    seen: [],
    list: async () => [...rows.values()],
    put: async (id, ciphertext, base) => {
      api.seen.push(ciphertext);
      const cur = rows.get(id);
      if (base === null) {
        if (cur) throw apiError(409);
      } else if (!cur) throw apiError(404);
      else if (cur.revision !== base) throw apiError(409);
      const next: RemoteNote = {
        note_id: id,
        ciphertext,
        revision: (cur?.revision ?? 0) + 1,
        updated_at: new Date(1_700_000_000_000 + clock++ * 1000).toISOString(),
      };
      rows.set(id, next);
      return next;
    },
    remove: async (id) => {
      if (!rows.delete(id)) throw apiError(404);
    },
  };
  return api;
}

let n = 0;
const uuid = () => `00000000-0000-4000-8000-${String(++n).padStart(12, '0')}`;

describe('notes store', () => {
  it('first use: no key, then setup, create, edit and reload through the server', async () => {
    const api = fakeServer();
    const keys = memoryKeys();
    const store = createNotesStore({ keys, api, uuid });
    await store.init();
    expect(store.getState().status).toBe('no-key');

    const bundle = await store.setup('a long enough passphrase', ROUNDS);
    expect(bundle).not.toBeNull();
    expect(store.getState().status).toBe('ready');

    const id = (await store.create('first draft'))!;
    await store.save(id, 'second draft with a secret');
    expect(store.getState().notes[0]).toMatchObject({
      id,
      text: 'second draft with a secret',
      revision: 2,
    });
    // Nothing the server saw contains the plaintext.
    expect(api.seen.length).toBe(2);
    for (const wire of api.seen) expect(atob(wire)).not.toContain('secret');

    // A reload (new store, same key storage and server) reads it back.
    const again = createNotesStore({ keys, api, uuid });
    await again.init();
    expect(again.getState().status).toBe('ready');
    expect(again.getState().notes.map((x) => x.text)).toEqual(['second draft with a secret']);
  });

  it('another browser needs the recovery file and the passphrase', async () => {
    const api = fakeServer();
    const a = createNotesStore({ keys: memoryKeys(), api, uuid });
    await a.init();
    await a.setup('shared secret phrase', ROUNDS);
    await a.create('for later');
    const file = a.exportFile()!;
    expect(file).not.toContain('for later');

    const b = createNotesStore({ keys: memoryKeys(), api, uuid });
    await b.init();
    expect(b.getState()).toMatchObject({ status: 'no-key', remoteCount: 1 });
    expect(await b.restore(file, 'not the phrase')).toBe(false);
    expect(b.getState().problem).toBe('badPassphrase');
    expect(await b.restore('not json', 'x')).toBe(false);
    expect(b.getState().problem).toBe('badFile');
    expect(await b.restore(file, 'shared secret phrase')).toBe(true);
    expect(b.getState().notes.map((x) => x.text)).toEqual(['for later']);
  });

  it('an edit made elsewhere is not overwritten: the local text becomes a copy', async () => {
    const api = fakeServer();
    const keys = memoryKeys();
    const one = createNotesStore({ keys, api, uuid });
    await one.init();
    await one.setup('conflict phrase here', ROUNDS);
    const id = (await one.create('v1'))!;
    // A second tab (same key) loads, then edits.
    const two = createNotesStore({ keys, api, uuid });
    await two.init();
    await two.save(id, 'v2 from the other tab');
    // The first tab, unaware, saves its own edit.
    await one.save(id, 'v2 from this tab');
    const texts = one
      .getState()
      .notes.map((x) => x.text)
      .sort();
    expect(texts).toEqual(['v2 from the other tab', 'v2 from this tab']);
    expect(api.rows.size).toBe(2);
  });

  it('delete, start over and lock', async () => {
    const api = fakeServer();
    const keys = memoryKeys();
    const store = createNotesStore({ keys, api, uuid });
    await store.init();
    await store.setup('delete me phrase', ROUNDS);
    const a = (await store.create('a'))!;
    await store.create('b');
    await store.remove(a);
    expect(store.getState().notes.map((x) => x.text)).toEqual(['b']);
    expect(api.rows.size).toBe(1);

    await store.lock();
    expect(store.getState().status).toBe('no-key');
    expect(await keys.load()).toBeNull();
    expect(api.rows.size).toBe(1);

    await store.startOver();
    expect(api.rows.size).toBe(0);
    expect(store.getState()).toMatchObject({ status: 'no-key', remoteCount: 0 });
  });

  it('a note the key cannot open is shown as unreadable, not as garbage', async () => {
    const api = fakeServer();
    const keys = memoryKeys();
    const store = createNotesStore({ keys, api, uuid });
    await store.init();
    await store.setup('unreadable phrase', ROUNDS);
    api.rows.set('x', {
      note_id: 'x',
      ciphertext: btoa('x'.repeat(40)),
      revision: 1,
      updated_at: '2030-01-01T00:00:00.000Z',
    });
    await store.init();
    expect(store.getState().notes[0]).toMatchObject({ id: 'x', unreadable: true, text: '' });
  });

  it('reports a too-long note without sending it, and an offline server without losing the text', async () => {
    const api = fakeServer();
    const store = createNotesStore({ keys: memoryKeys(), api, uuid });
    await store.init();
    await store.setup('size limit phrase', ROUNDS);
    const id = (await store.create('small'))!;
    await store.save(id, 'x'.repeat(70 * 1024));
    expect(store.getState().problem).toBe('tooBig');
    expect(api.seen).toHaveLength(1);

    api.put = async () => {
      throw new Error('offline');
    };
    await store.save(id, 'changed');
    expect(store.getState().problem).toBe('offline');
    expect(store.getState().notes[0]!.text).toBe('small');
  });
});
