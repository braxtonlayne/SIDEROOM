// ACT-EMU-001: the five emulator cores the owner allowed (roadmap-p27 section 19) and what each plays. The ids are
// the directory names under `/emu/cores/<id>/` (fetched at install by pinned digest, ACT-EMU-003). Nothing here
// loads anything: it is the table the shell, the ROM list and the BIOS rule read.

export type Platform = 'nes' | 'snes' | 'gb' | 'gbc' | 'gba' | 'genesis' | 'ps1';
export type CoreId = 'fceumm' | 'supafaust' | 'mgba' | 'blastem' | 'pcsx_rearmed';

export interface CoreInfo {
  id: CoreId;
  /** Shown in the player's "about this core" line. */
  label: string;
  /** SPDX id of the core; the RetroArch frontend inside each build is GPL-3.0 on top of it. */
  licence: string;
  platforms: Platform[];
  /** The corresponding-source offer (Q23): the project the build comes from. */
  source: string;
  /**
   * EmulatorJS ships a `-legacy-wasm.data` twin for cores that do not need WebGL 2 and `-wasm.data` for those that do
   * (emu-ops handoff section 1.2). `legacy` cores run everywhere; the others need WebGL 2.
   */
  variant: 'legacy' | 'webgl2';
  /** The core cannot start without a BIOS the member supplies from their own mount. */
  needsBios: boolean;
}

export const CORES: Readonly<Record<CoreId, CoreInfo>> = {
  fceumm: {
    id: 'fceumm',
    label: 'FCEUmm',
    licence: 'GPL-2.0',
    platforms: ['nes'],
    source: 'https://github.com/EmulatorJS/libretro-fceumm',
    variant: 'legacy',
    needsBios: false,
  },
  supafaust: {
    id: 'supafaust',
    label: 'Beetle Supafaust',
    licence: 'GPL-2.0',
    platforms: ['snes'],
    source: 'https://github.com/libretro/beetle-supafaust',
    variant: 'legacy',
    needsBios: false,
  },
  mgba: {
    id: 'mgba',
    label: 'mGBA',
    licence: 'MPL-2.0',
    platforms: ['gba', 'gb', 'gbc'],
    source: 'https://github.com/EmulatorJS/mgba',
    variant: 'legacy',
    needsBios: false,
  },
  blastem: {
    id: 'blastem',
    label: 'BlastEm',
    licence: 'GPL-3.0',
    platforms: ['genesis'],
    source: 'https://github.com/libretro/blastem',
    variant: 'legacy',
    needsBios: false,
  },
  pcsx_rearmed: {
    id: 'pcsx_rearmed',
    label: 'PCSX-ReARMed',
    licence: 'GPL-2.0',
    platforms: ['ps1'],
    source: 'https://github.com/EmulatorJS/pcsx_rearmed',
    variant: 'webgl2',
    needsBios: true,
  },
};

export const PLATFORM_CORE: Readonly<Record<Platform, CoreId>> = {
  nes: 'fceumm',
  snes: 'supafaust',
  gb: 'mgba',
  gbc: 'mgba',
  gba: 'mgba',
  genesis: 'blastem',
  ps1: 'pcsx_rearmed',
};

export const PLATFORM_LABEL: Readonly<Record<Platform, string>> = {
  nes: 'NES',
  snes: 'SNES',
  gb: 'Game Boy',
  gbc: 'Game Boy Color',
  gba: 'Game Boy Advance',
  genesis: 'Genesis',
  ps1: 'PlayStation',
};

export const isCoreId = (s: string | null | undefined): s is CoreId =>
  !!s && Object.prototype.hasOwnProperty.call(CORES, s);

export const isPlatform = (s: string | null | undefined): s is Platform =>
  !!s && Object.prototype.hasOwnProperty.call(PLATFORM_CORE, s);

/** The core that plays a platform. */
export const coreFor = (p: Platform): CoreInfo => CORES[PLATFORM_CORE[p]];

/** Where a core's archive is served (the `.data` 7z of the EmulatorJS build set, emu-ops handoff section 1.2). */
export const coreDataUrl = (core: CoreInfo): string =>
  `/emu/cores/${core.id}/${core.id}${core.variant === 'legacy' ? '-legacy' : ''}-wasm.data`;

/** The 7z extraction helper the archives need (a self-contained worker script, served next to the cores). */
export const EXTRACT_7Z_URL = '/emu/cores/extract7z.js';
