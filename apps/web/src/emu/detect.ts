// ACT-EMU-001: which platform (and so which core) a ROM is for, from its name and its first bytes. Mirrors
// `crates/sr-storage/src/index/rom.rs`: the server's catalog knows the extension, the browser also reads the header.
import { coreFor, type CoreInfo, type Platform } from './cores';

/** Bytes of a ROM that [detectPlatform] reads (a SNES header sits at 0xFFC0 plus an optional 512-byte copier header). */
export const HEAD_BYTES = 0x10200;

const EXT_PLATFORM: Record<string, Platform> = {
  nes: 'nes',
  fds: 'nes',
  unf: 'nes',
  unif: 'nes',
  sfc: 'snes',
  smc: 'snes',
  swc: 'snes',
  fig: 'snes',
  gb: 'gb',
  gbc: 'gbc',
  gba: 'gba',
  md: 'genesis',
  gen: 'genesis',
  smd: 'genesis',
  cue: 'ps1',
  iso: 'ps1',
  img: 'ps1',
  pbp: 'ps1',
  chd: 'ps1',
  m3u: 'ps1',
  ccd: 'ps1',
  mdf: 'ps1',
  toc: 'ps1',
  cbn: 'ps1',
};

export const extOf = (name: string): string => {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
};

/** Whether a file name can be a ROM at all (the list under `emu/roms/` keeps only these). */
export const isRomName = (name: string): boolean => {
  const e = extOf(name);
  return e === 'bin' || e in EXT_PLATFORM;
};

const startsWith = (b: Uint8Array, at: number, bytes: readonly number[]) =>
  bytes.every((v, i) => b[at + i] === v);
const ascii = (s: string) => [...s].map((c) => c.charCodeAt(0));

const GB_LOGO = [0xce, 0xed, 0x66, 0x66];
const GBA_LOGO = [0x24, 0xff, 0xae, 0x51];
const CD_SYNC = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];

function snesHeaderAt(b: Uint8Array, at: number): boolean {
  if (b.length < at + 0x20) return false;
  const comp = (b[at + 0x1c] ?? 0) | ((b[at + 0x1d] ?? 0) << 8);
  const sum = (b[at + 0x1e] ?? 0) | ((b[at + 0x1f] ?? 0) << 8);
  return ((comp + sum) & 0xffff) === 0xffff && ((b[at + 0x15] ?? 0) & 0x0f) <= 5;
}

/**
 * The platform of a file. The header wins over the extension; a `.bin` with no recognisable header is `null` (the
 * member picks a core by hand rather than the shell guessing). `head` is the start of the file.
 */
export function detectPlatform(name: string, head: Uint8Array): Platform | null {
  const ext = extOf(name);
  if (
    startsWith(head, 0, ascii('NES\x1a')) ||
    startsWith(head, 0, ascii('UNIF')) ||
    startsWith(head, 0, ascii('FDS\x1a'))
  ) {
    return 'nes';
  }
  if (head.length > 0xb2 && startsWith(head, 4, GBA_LOGO) && head[0xb2] === 0x96) return 'gba';
  if (head.length >= 0x108 && startsWith(head, 0x104, GB_LOGO)) {
    const cgb = head[0x143] ?? 0;
    return cgb === 0x80 || cgb === 0xc0 ? 'gbc' : 'gb';
  }
  if (head.length >= 0x108) {
    for (let i = 0x100; i + 4 <= 0x108; i++) {
      if (startsWith(head, i, ascii('SEGA'))) return 'genesis';
    }
  }
  if (startsWith(head, 0, CD_SYNC) || startsWith(head, 0, [0, 0x50, 0x42, 0x50])) return 'ps1';
  const byExt = EXT_PLATFORM[ext];
  if (byExt) return byExt;
  if (ext === 'bin') return null;
  const snes = [0x7fc0, 0xffc0, 0x7fc0 + 512, 0xffc0 + 512].some((at) => snesHeaderAt(head, at));
  return snes ? 'snes' : null;
}

/** The core for a file, or `null` when nothing recognises it. */
export function chooseCore(name: string, head: Uint8Array): CoreInfo | null {
  const p = detectPlatform(name, head);
  return p ? coreFor(p) : null;
}
