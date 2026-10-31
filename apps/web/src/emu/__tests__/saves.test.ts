import {
  getSave,
  isRomHash,
  parseSaveKey,
  pickSaveMount,
  putSave,
  saveKey,
  savePrefix,
  type SaveIo,
} from '../saves';

const HASH = 'a'.repeat(40);

describe('the save path', () => {
  it('is emu/saves/<core>/<rom-hash>/<file>', () => {
    expect(savePrefix('fceumm', HASH)).toBe(`emu/saves/fceumm/${HASH}/`);
    expect(saveKey('mgba', HASH, 'sram')).toBe(`emu/saves/mgba/${HASH}/sram.srm`);
    expect(saveKey('pcsx_rearmed', HASH, 'state3')).toBe(
      `emu/saves/pcsx_rearmed/${HASH}/state3.state`,
    );
  });
  it('refuses a hash that is not 40 lower-case hex digits', () => {
    expect(isRomHash(HASH)).toBe(true);
    expect(isRomHash('A'.repeat(40))).toBe(false);
    expect(isRomHash('../x')).toBe(false);
    expect(() => savePrefix('fceumm', '../../etc')).toThrow();
  });
  it('parses what it writes and nothing else', () => {
    expect(parseSaveKey(saveKey('mgba', HASH, 'state2'))).toEqual({
      core: 'mgba',
      romHash: HASH,
      name: 'state2',
    });
    expect(parseSaveKey(saveKey('mgba', HASH, 'sram'))?.name).toBe('sram');
    expect(parseSaveKey(`emu/saves/mgba/${HASH}/state9.state`)).toBeNull();
    expect(parseSaveKey(`emu/saves/mgba/${HASH}/../x.srm`)).toBeNull();
    expect(parseSaveKey(`emu/roms/${HASH}`)).toBeNull();
    expect(parseSaveKey('emu/saves/mgba/sram.srm')).toBeNull();
  });
});

describe('the mount saves go to', () => {
  const m = (id: string, owner_kind: string, status = 'active', cors: string | null = 'x') => ({
    id,
    owner_kind,
    status,
    cors_ok_at: cors,
  });
  it('is the member own working mount, never a server shelf or the instance bucket', () => {
    expect(pickSaveMount([m('1', 'platform'), m('2', 'guild'), m('3', 'user')])?.id).toBe('3');
    expect(pickSaveMount([m('1', 'guild'), m('2', 'platform')])).toBeNull();
  });
  it('skips a mount that fails or has not passed its CORS check, and honours a saved choice', () => {
    const mounts = [m('1', 'user', 'failing'), m('2', 'user', 'active', null), m('3', 'user')];
    expect(pickSaveMount(mounts)?.id).toBe('3');
    expect(pickSaveMount([m('1', 'user'), m('2', 'user')], '2')?.id).toBe('2');
    expect(pickSaveMount([m('1', 'user')], '9')?.id).toBe('1');
  });
});

describe('putSave and getSave', () => {
  const calls: string[] = [];
  const io = (status: number, body = new Uint8Array([1, 2, 3])): SaveIo => ({
    presign: async (mount, key, method) => {
      calls.push(`presign ${mount} ${key} ${method}`);
      return { url: 'https://bucket.example/obj?sig=1' };
    },
    fetch: (async (url: string, init?: RequestInit) => {
      calls.push(`${init?.method ?? 'GET'} ${url} ${init?.cache ?? ''}`);
      return new Response(status === 200 ? (body as BlobPart) : null, { status });
    }) as typeof fetch,
  });
  beforeEach(() => void (calls.length = 0));

  it('PUTs through a presigned URL, uncached', async () => {
    await putSave('7', saveKey('fceumm', HASH, 'sram'), new Uint8Array([9]), io(200));
    expect(calls).toEqual([
      `presign 7 emu/saves/fceumm/${HASH}/sram.srm PUT`,
      'PUT https://bucket.example/obj?sig=1 no-store',
    ]);
  });
  it('will not sign anything that is not a save key', async () => {
    await expect(putSave('7', 'emu/roms/x.nes', new Uint8Array(1), io(200))).rejects.toThrow(
      'not a save key',
    );
    await expect(getSave('7', 'Movies/x.mp4', io(200))).rejects.toThrow('not a save key');
    expect(calls).toEqual([]);
  });
  it('reports a refusal, and reads a missing save as none', async () => {
    const key = saveKey('mgba', HASH, 'sram');
    await expect(putSave('7', key, new Uint8Array(1), io(403))).rejects.toThrow('403');
    expect(await getSave('7', key, io(404))).toBeNull();
    expect([...((await getSave('7', key, io(200))) ?? [])]).toEqual([1, 2, 3]);
  });
});
