// ACT-EMU-001: everything that has to happen before a core can start, without touching the DOM: read the ROM from the
// bucket, decide the core by extension and header, hash the ROM (its identity for saves), pick up its save RAM from the
// member's own mount, and apply the BIOS rule. Dependencies are passed in so the order and the refusals are tested.
import { coreFor, CORES, isCoreId, isPlatform, type CoreInfo } from './cores';
import { detectPlatform, extOf, HEAD_BYTES } from './detect';
import { sha1Hex } from './digest';
import { cueFiles, type RomItem } from './romApi';
import { getSave, saveKey } from './saves';

export class PlayError extends Error {
  constructor(
    readonly kind: 'platform' | 'bios' | 'rom',
    detail?: string,
  ) {
    super(detail ?? kind);
  }
}

export interface PrepareDeps {
  /** Read an object of a mount (presigned, uncached). */
  read: (
    mountId: string,
    key: string,
    onProgress?: (done: number, total: number) => void,
  ) => Promise<Uint8Array>;
  /** The mount saves are kept on, or `null` (then nothing is saved and the UI says so). */
  saveMountId: string | null;
  loadSave: typeof getSave;
}

export interface Prepared {
  core: CoreInfo;
  rom: Uint8Array;
  /** SHA-1 of the ROM file: the folder name of its saves. */
  hash: string;
  extra: Array<{ name: string; bytes: Uint8Array }>;
  sram: Uint8Array | null;
}

const dirOf = (key: string) => key.slice(0, key.lastIndexOf('/') + 1);

export async function prepareGame(
  item: Pick<RomItem, 'mount_id' | 'key' | 'platform' | 'core'>,
  deps: PrepareDeps,
  onProgress?: (done: number, total: number) => void,
): Promise<Prepared> {
  const name = item.key.split('/').pop() ?? item.key;
  let rom: Uint8Array;
  try {
    rom = await deps.read(item.mount_id, item.key, onProgress);
  } catch (e) {
    throw new PlayError('rom', e instanceof Error ? e.message : undefined);
  }
  const platform = detectPlatform(name, rom.subarray(0, HEAD_BYTES));
  const hinted = isPlatform(item.platform) ? item.platform : null;
  const chosen = platform ?? hinted;
  const core = chosen ? coreFor(chosen) : isCoreId(item.core) ? CORES[item.core] : null;
  if (!core) throw new PlayError('platform');

  // A cue sheet names its tracks, which sit beside it in the same folder.
  const extra: Prepared['extra'] = [];
  if (extOf(name) === 'cue') {
    for (const track of cueFiles(new TextDecoder().decode(rom))) {
      try {
        extra.push({
          name: track,
          bytes: await deps.read(item.mount_id, `${dirOf(item.key)}${track}`),
        });
      } catch {
        throw new PlayError('rom', `missing ${track}`);
      }
    }
  }
  const hash = await sha1Hex(rom);
  let sram: Uint8Array | null = null;
  if (deps.saveMountId) {
    try {
      sram = await deps.loadSave(deps.saveMountId, saveKey(core.id, hash, 'sram'));
    } catch {
      sram = null; // an unreachable save is not a reason to refuse the game; the UI shows the save status
    }
  }
  return { core, rom, hash, extra, sram };
}

/** Whether two byte arrays hold the same bytes. */
export function sameBytes(a: Uint8Array | null, b: Uint8Array | null): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}
