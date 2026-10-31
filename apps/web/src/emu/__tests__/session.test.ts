import { CORES } from '../cores';
import {
  CoreUnavailableError,
  isSevenZip,
  loadCore,
  pickMembers,
  type CoreModule,
  type LoaderDeps,
} from '../loader';
import { contentFileName, EmuSession, RETROARCH_CFG } from '../session';

const sevenZip = new Uint8Array([0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c, 0, 4, 0, 0]);

function fakeModule() {
  const files = new Map<string, Uint8Array | string>();
  const calls: Array<[string, unknown[]]> = [];
  const heap = new Uint8Array(64).map((_, i) => i);
  return {
    files,
    calls,
    HEAPU8: heap,
    FS: {
      writeFile: (p: string, d: Uint8Array | string) => void files.set(p, d),
      readFile: (p: string) => {
        const d = files.get(p);
        if (!(d instanceof Uint8Array)) throw new Error('ENOENT');
        return d;
      },
      mkdir: (p: string) => void files.set(`${p}/`, ''),
      unlink: (p: string) => void files.delete(p),
      analyzePath: (p: string) => ({ exists: files.has(p) || files.has(`${p}/`) }),
    },
    cwrap:
      (name: string) =>
      (...a: unknown[]) => {
        calls.push([name, a]);
        if (name === 'save_state_info') return '8|10|1';
        if (name === 'save_file_path') return '/data/saves/game.srm';
        if (name === 'supports_states') return 1;
        return undefined;
      },
    callMain: (a: string[]) => void calls.push(['callMain', a]),
    resumeMainLoop: () => void calls.push(['resumeMainLoop', []]),
    abort: () => void calls.push(['abort', []]),
  };
}

describe('loading a core', () => {
  const members = new Map<string, Uint8Array>([
    ['fceumm_libretro.js', new Uint8Array([1])],
    ['fceumm_libretro.wasm', new Uint8Array([2])],
    ['core.json', new TextEncoder().encode('{"name":"fceumm","extensions":["nes"]}')],
    ['license.txt', new TextEncoder().encode('GPL')],
    ['build.json', new Uint8Array([3])],
  ]);
  const deps = (over: Partial<LoaderDeps> = {}): LoaderDeps => ({
    fetch: (async () => new Response(sevenZip as BlobPart)) as typeof fetch,
    extract: async () => members,
    runGlue: async () => async (o) => ({ ...fakeModule(), opts: o }) as unknown as CoreModule,
    supportsWebgl2: () => true,
    ...over,
  });
  const canvas = document.createElement('canvas');

  beforeAll(() => {
    // jsdom has no object URLs
    URL.createObjectURL = () => 'blob:test';
  });

  it('recognises the 7z signature', () => {
    expect(isSevenZip(sevenZip)).toBe(true);
    expect(isSevenZip(new TextEncoder().encode('<!doctype html>'))).toBe(false);
  });
  it('picks the glue, the wasm, the licence and the metadata by suffix', () => {
    const f = pickMembers(members);
    expect(f?.js[0]).toBe(1);
    expect(f?.wasm[0]).toBe(2);
    expect(f?.license).toBe('GPL');
    expect(f?.meta).toEqual({ name: 'fceumm', extensions: ['nes'] });
    expect(pickMembers(new Map([['x.js', new Uint8Array(1)]]))).toBeNull();
  });
  it('starts a core from its archive', async () => {
    const stages: string[] = [];
    const urls: string[] = [];
    const loaded = await loadCore(
      CORES.fceumm,
      { canvas, onProgress: (s) => stages.push(s) },
      deps({
        fetch: (async (u: string) => {
          urls.push(u);
          return new Response(sevenZip as BlobPart);
        }) as typeof fetch,
      }),
    );
    expect(urls).toEqual(['/emu/cores/fceumm/fceumm-legacy-wasm.data']);
    expect(stages).toEqual(['download', 'unpack', 'start']);
    const opts = (loaded.module as unknown as { opts: Record<string, unknown> }).opts;
    expect(opts).toMatchObject({ noInitialRun: true, canvas });
  });
  it('says the core is missing for a 404 and for an index page served in its place', async () => {
    const notFound = deps({
      fetch: (async () => new Response(null, { status: 404 })) as typeof fetch,
    });
    await expect(loadCore(CORES.blastem, { canvas }, notFound)).rejects.toMatchObject({
      why: 'missing',
    });
    const html = deps({
      fetch: (async () => new Response('<!doctype html><html>')) as typeof fetch,
    });
    await expect(loadCore(CORES.supafaust, { canvas }, html)).rejects.toBeInstanceOf(
      CoreUnavailableError,
    );
  });
  it('needs WebGL 2 for the cores that use it, and a complete archive', async () => {
    const noGl = deps({ supportsWebgl2: () => false });
    await expect(loadCore(CORES.pcsx_rearmed, { canvas }, noGl)).rejects.toMatchObject({
      why: 'webgl2',
    });
    const empty = deps({ extract: async () => new Map() });
    await expect(loadCore(CORES.fceumm, { canvas }, empty)).rejects.toMatchObject({
      why: 'format',
    });
  });
});

describe('a running game', () => {
  const setup = () => {
    const m = fakeModule();
    return { m, s: new EmuSession(m as unknown as CoreModule) };
  };

  it('names the content game.<ext>', () => {
    expect(contentFileName('Super Thing (USA).NES')).toBe('game.nes');
    expect(contentFileName('noext')).toBe('game.bin');
  });
  it('writes the config, the ROM and the SRAM, then starts the main loop', () => {
    const { m, s } = setup();
    s.start({ romName: 'a.nes', rom: new Uint8Array([1, 2]), sram: new Uint8Array([9]) });
    expect(m.files.get('/home/web_user/.config/retroarch/retroarch.cfg')).toBe(RETROARCH_CFG);
    expect(m.files.get('/game.nes')).toEqual(new Uint8Array([1, 2]));
    expect(m.files.get('/data/saves/game.srm')).toEqual(new Uint8Array([9]));
    const order = m.calls.map((c) => c[0]);
    expect(order.indexOf('callMain')).toBeLessThan(order.indexOf('resumeMainLoop'));
    expect(m.calls.find((c) => c[0] === 'callMain')?.[1]).toEqual(['/game.nes']);
    expect(s.isRunning).toBe(true);
  });
  it('puts a BIOS only into the in-memory system folders and cue tracks beside the ROM', () => {
    const { m, s } = setup();
    s.start({
      romName: 'disc.cue',
      rom: new Uint8Array(1),
      extra: [
        { name: 'track1.bin', bytes: new Uint8Array(2) },
        { name: '../evil.bin', bytes: new Uint8Array(2) },
      ],
      bios: { file: 'scph1001.bin', bytes: new Uint8Array([7]) },
    });
    expect(m.files.has('/home/web_user/retroarch/userdata/system/scph1001.bin')).toBe(true);
    expect(m.files.has('/scph1001.bin')).toBe(true);
    expect(m.files.has('/track1.bin')).toBe(true);
    expect([...m.files.keys()].some((k) => k.includes('evil'))).toBe(false);
  });
  it('sends RetroPad indices to the core', () => {
    const { m, s } = setup();
    s.setButton(0, 'a', true); // not running yet: ignored
    s.start({ romName: 'a.nes', rom: new Uint8Array(1) });
    s.setButton(1, 'a', true);
    s.setButton(0, 'start', false);
    const input = m.calls.filter((c) => c[0] === 'simulate_input').map((c) => c[1]);
    expect(input).toEqual([
      [1, 8, 1],
      [0, 3, 0],
    ]);
  });
  it('reads a save state out of the heap and the SRAM out of the save file', () => {
    const { m, s } = setup();
    s.start({ romName: 'a.nes', rom: new Uint8Array(1) });
    expect([...s.saveState()]).toEqual([10, 11, 12, 13, 14, 15, 16, 17]);
    expect(s.readSram()).toBeNull();
    m.files.set('/data/saves/game.srm', new Uint8Array([4, 5]));
    expect([...(s.readSram() ?? [])]).toEqual([4, 5]);
    s.loadState(new Uint8Array([1]));
    const loaded = m.calls.some(
      (c) => c[0] === 'load_state' && (c[1] as unknown[])[0] === 'game.state',
    );
    expect(loaded).toBe(true);
  });
  it('stops on dispose', () => {
    const { m, s } = setup();
    s.start({ romName: 'a.nes', rom: new Uint8Array(1) });
    s.dispose();
    expect(s.isRunning).toBe(false);
    expect(m.calls.some((c) => c[0] === 'abort')).toBe(true);
  });
});
