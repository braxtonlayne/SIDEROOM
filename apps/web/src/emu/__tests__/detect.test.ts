import { chooseCore, detectPlatform, isRomName } from '../detect';
import { CORES, coreDataUrl, coreFor } from '../cores';
import { cueFiles } from '../romApi';

const buf = (n: number, set: Record<number, number[]> = {}) => {
  const b = new Uint8Array(n);
  for (const [at, vs] of Object.entries(set)) b.set(vs, Number(at));
  return b;
};

describe('the core is chosen by extension and header', () => {
  it('reads the extension when the header says nothing', () => {
    expect(detectPlatform('Game.NES', new Uint8Array(16))).toBe('nes');
    expect(detectPlatform('x.sfc', new Uint8Array(16))).toBe('snes');
    expect(detectPlatform('x.gba', new Uint8Array(16))).toBe('gba');
    expect(detectPlatform('x.md', new Uint8Array(16))).toBe('genesis');
    expect(detectPlatform('disc.cue', new Uint8Array(16))).toBe('ps1');
    expect(detectPlatform('notes.txt', new Uint8Array(16))).toBeNull();
  });
  it('lets the header beat the extension', () => {
    expect(detectPlatform('x.bin', buf(64, { 0: [0x4e, 0x45, 0x53, 0x1a] }))).toBe('nes');
    expect(detectPlatform('x.bin', buf(0x200, { 0x100: [0x53, 0x45, 0x47, 0x41] }))).toBe('genesis');
    expect(detectPlatform('x.bin', buf(0xc0, { 4: [0x24, 0xff, 0xae, 0x51], 0xb2: [0x96] }))).toBe(
      'gba',
    );
    const logo = [0xce, 0xed, 0x66, 0x66];
    expect(detectPlatform('x.dat', buf(0x150, { 0x104: logo }))).toBe('gb');
    expect(detectPlatform('x.dat', buf(0x150, { 0x104: logo, 0x143: [0xc0] }))).toBe('gbc');
    const sync = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];
    expect(detectPlatform('track.bin', buf(0x930, { 0: sync }))).toBe('ps1');
  });
  it('leaves a headerless .bin undecided', () => {
    expect(detectPlatform('mystery.bin', new Uint8Array(64))).toBeNull();
    expect(isRomName('a.bin')).toBe(true);
    expect(isRomName('a.txt')).toBe(false);
  });
  it('finds a SNES image by its checksum', () => {
    const b = buf(0x8000, { [0x7fc0 + 0x1d]: [0xff], [0x7fc0 + 0x1e]: [0xff] });
    expect(detectPlatform('zelda.rom', b)).toBe('snes');
  });
  it('maps a platform to one of the five allowed cores', () => {
    expect(chooseCore('x.nes', new Uint8Array(4))?.id).toBe('fceumm');
    expect(chooseCore('x.gbc', new Uint8Array(4))?.id).toBe('mgba');
    expect(chooseCore('x.pbp', new Uint8Array(4))?.id).toBe('pcsx_rearmed');
    expect(Object.keys(CORES).sort()).toEqual([
      'blastem',
      'fceumm',
      'mgba',
      'pcsx_rearmed',
      'supafaust',
    ]);
    expect(coreFor('ps1').needsBios).toBe(true);
    expect(coreFor('nes').needsBios).toBe(false);
  });
  it('asks for the build variant EmulatorJS ships for each core', () => {
    expect(coreDataUrl(CORES.fceumm)).toBe('/emu/cores/fceumm/fceumm-legacy-wasm.data');
    expect(coreDataUrl(CORES.pcsx_rearmed)).toBe('/emu/cores/pcsx_rearmed/pcsx_rearmed-wasm.data');
  });
  it('reads the tracks a cue sheet names', () => {
    const cue =
      'FILE "Game (Track 1).bin" BINARY\n  TRACK 01 MODE2/2352\nFILE track2.bin BINARY\nFILE "dir/x.bin" BINARY';
    expect(cueFiles(cue)).toEqual(['Game (Track 1).bin', 'track2.bin', 'x.bin']);
  });
});
