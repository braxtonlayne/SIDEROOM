// ACT-EMU-001: the PS1 BIOS rule (roadmap-p27 note 6, Q23, D-380).
//
// The BIOS is Sony's copyright. The instance never ships, stores or caches one. The shell accepts a BIOS only when
//   1. it comes from `emu/bios/` of the member's OWN bucket (never a server's shelf, never the instance bucket),
//   2. it is exactly 512 KiB, and
//   3. its digest is on the list of known retail BIOS dumps below (a file the list does not know is refused).
// The bytes live in memory for the length of one session: written into the core's in-memory file system, never to
// localStorage, IndexedDB, the Cache API or the server. [fetchBios] asks the bucket with `cache: 'no-store'`.
import { md5Hex, sha1Hex } from './digest';

export const BIOS_PREFIX = 'emu/bios/';
export const BIOS_SIZE = 512 * 1024;

export interface BiosEntry {
  /** The file name the core looks for. */
  file: string;
  description: string;
  region: 'NTSC-J' | 'NTSC-U' | 'PAL';
  /** MD5 from the libretro and DuckStation lists (two independent sources agree). */
  md5: string;
  /**
   * SHA-1, when it has been derived from a dump the owner holds. None are filled in yet (the lists the agents could
   * read publish MD5 only); a file is recognised by either digest.
   */
  sha1?: string;
}

const e = (
  file: string,
  region: BiosEntry['region'],
  description: string,
  md5: string,
): BiosEntry => ({ file, region, description, md5 });

/** Retail PS1 BIOS dumps (not the PS2 ones, which carry a different file). */
export const KNOWN_PS1_BIOS: readonly BiosEntry[] = [
  e('scph1000.bin', 'NTSC-J', 'SCPH-1000 v1.0', '239665b1a3dade1b5a52c06338011044'),
  e('scph1001.bin', 'NTSC-U', 'SCPH-1001 v2.0', 'dc2b9bf8da62ec93e868cfd29f0d067d'),
  e('scph1001.bin', 'NTSC-U', 'SCPH-1001 v2.1', 'da27e8b6dab242d8f91a9b25d80c63b8'),
  e('scph1001.bin', 'NTSC-U', 'SCPH-1001 v2.2', '924e392ed05558ffdb115408c263dccf'),
  e('scph1002.bin', 'PAL', 'SCPH-1002 v2.0', '54847e693405ffeb0359c6287434cbef'),
  e('scph1002.bin', 'PAL', 'SCPH-1002 v2.1', '417b34706319da7cf001e76e40136c23'),
  e('scph1002.bin', 'PAL', 'SCPH-1002 v2.2', 'e2110b8a2b97a8e0b857a45d32f7e187'),
  e('scph3000.bin', 'NTSC-J', 'SCPH-3000 v1.1', '849515939161e62f6b866f6853006780'),
  e('scph3500.bin', 'NTSC-J', 'SCPH-3500 v2.1', 'cba733ceeff5aef5c32254f1d617fa62'),
  e('scph5000.bin', 'NTSC-J', 'SCPH-5000 v2.2', '57a06303dfa9cf9351222dfcbb4a29d9'),
  e('scph5500.bin', 'NTSC-J', 'SCPH-5500 v3.0', '8dd7d5296a650fac7319bce665a6a53c'),
  e('scph5501.bin', 'NTSC-U', 'SCPH-5501 v3.0', '490f666e1afb15b7362b406ed1cea246'),
  e('scph5502.bin', 'PAL', 'SCPH-5502 v3.0', '32736f17079d0b2b7024407c39bd3050'),
  e('scph7000.bin', 'NTSC-J', 'SCPH-7000 v4.0', '8e4c14f567745eff2f0408c8129f72a6'),
  e('scph7000w.bin', 'NTSC-J', 'SCPH-7000W v4.1', 'b84be139db3ee6cbd075630aa20a6553'),
  e('scph7001.bin', 'NTSC-U', 'SCPH-7001 v4.1', '1e68c231d0896b7eadcad1d7d8e76129'),
  e('scph7002.bin', 'PAL', 'SCPH-7002 v4.1', 'b9d9a0286c33dc6b7237bb13cd46fdee'),
  e('scph100.bin', 'NTSC-J', 'SCPH-100 v4.3', '8abc1b549a4a80954addc48ef02c4521'),
  e('scph101.bin', 'NTSC-U', 'SCPH-101 v4.4', '9a09ab7e49b422c007e6d54d7c49b965'),
  e('scph101.bin', 'NTSC-U', 'SCPH-101 v4.5', '6e3735ff4c7dc899ee98981385f6f3d0'),
  e('scph102.bin', 'PAL', 'SCPH-102 v4.4', 'b10f5e0e3d9eb60e5159690680b1e774'),
  e('scph102.bin', 'PAL', 'SCPH-102 v4.5', 'de93caec13d1a141a40a79f5c86168d6'),
  e('scph1000r.bin', 'NTSC-J', 'SCPH-1000R v4.5', '476d68a94ccec3b9c8303bbd1daf2810'),
];

export type BiosVerdict =
  | { ok: true; entry: BiosEntry; sha1: string; md5: string }
  | { ok: false; reason: 'size' | 'unknown' | 'source'; sha1?: string; md5?: string };

/**
 * Check a BIOS file's bytes against the list. `list` is a parameter so a test can use its own entries; the shell
 * uses [KNOWN_PS1_BIOS]. The digests are returned on a refusal too, so the member (and the owner who maintains the
 * list) can see what the file is.
 */
export async function checkBios(
  bytes: Uint8Array,
  list: readonly BiosEntry[] = KNOWN_PS1_BIOS,
): Promise<BiosVerdict> {
  if (bytes.length !== BIOS_SIZE) return { ok: false, reason: 'size' };
  const [sha1, md5] = [await sha1Hex(bytes), md5Hex(bytes)];
  const entry = list.find((x) => (x.sha1 !== undefined && x.sha1 === sha1) || x.md5 === md5);
  return entry ? { ok: true, entry, sha1, md5 } : { ok: false, reason: 'unknown', sha1, md5 };
}

export interface BiosSource {
  mountOwnerKind: string;
  mountOwnerId: string | null;
  key: string;
}

/** Whether a file may be offered as a BIOS at all: in `emu/bios/` of a mount the member owns. */
export function biosSourceAllowed(src: BiosSource, userId: string): boolean {
  return (
    src.mountOwnerKind === 'user' &&
    src.mountOwnerId === userId &&
    src.key.startsWith(BIOS_PREFIX) &&
    !src.key.slice(BIOS_PREFIX.length).includes('..') &&
    src.key.length > BIOS_PREFIX.length
  );
}

export interface BiosIo {
  presignGet: (mountId: string, key: string) => Promise<{ url: string }>;
  fetch: typeof fetch;
}

/**
 * Read a BIOS from the member's own bucket and check it. The request is `no-store` so the browser's HTTP cache does
 * not keep it either; the service worker never sees a cross-origin bucket URL. Nothing is returned for a refused file.
 */
export async function fetchBios(
  mountId: string,
  src: BiosSource,
  userId: string,
  io: BiosIo,
  list: readonly BiosEntry[] = KNOWN_PS1_BIOS,
): Promise<{ verdict: BiosVerdict; bytes: Uint8Array | null }> {
  if (!biosSourceAllowed(src, userId)) {
    return { verdict: { ok: false, reason: 'source' }, bytes: null };
  }
  const { url } = await io.presignGet(mountId, src.key);
  const res = await io.fetch(url, { cache: 'no-store', credentials: 'omit' });
  if (!res.ok) throw new Error(`the bucket did not answer (${res.status})`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const verdict = await checkBios(bytes, list);
  return { verdict, bytes: verdict.ok ? bytes : null };
}
