import { prepareGame, type PrepareDeps } from '../play';

// SEC-AUD-008: a save that could not be read is not "no save yet".
const rom = new Uint8Array(40976);
rom.set([0x4e, 0x45, 0x53, 0x1a]);
const item = { mount_id: '7', key: 'emu/roms/a.nes', platform: 'nes', core: 'fceumm' };
const deps = (loadSave: PrepareDeps['loadSave']): PrepareDeps => ({
  read: () => Promise.resolve(rom),
  saveMountId: '7',
  loadSave,
});

describe('prepareGame and the save read (SEC-AUD-008)', () => {
  it('a missing save (null) is a fresh start with saving on', async () => {
    const p = await prepareGame(
      item,
      deps(() => Promise.resolve(null)),
    );
    expect(p.sram).toBeNull();
    expect(p.saveReadFailed).toBe(false);
  });
  it('a save that was read is handed to the core', async () => {
    const p = await prepareGame(
      item,
      deps(() => Promise.resolve(new Uint8Array([1, 2]))),
    );
    expect(Array.from(p.sram ?? [])).toEqual([1, 2]);
    expect(p.saveReadFailed).toBe(false);
  });
  it('a read error is flagged so the session starts with saving off', async () => {
    const p = await prepareGame(
      item,
      deps(() => Promise.reject(new Error('the bucket did not answer (500)'))),
    );
    expect(p.sram).toBeNull();
    expect(p.saveReadFailed).toBe(true);
  });
  it('no save mount is not a read failure', async () => {
    const p = await prepareGame(item, {
      ...deps(() => Promise.reject(new Error('x'))),
      saveMountId: null,
    });
    expect(p.saveReadFailed).toBe(false);
  });
});
