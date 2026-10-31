// ACT-EMU-001: where saves live and how they travel. Save RAM and save states go to the member's OWN mount under
// `emu/saves/<core>/<rom-hash>/` by presigned PUT (the instance never sees the bytes); the server signs a PUT only for
// exactly such a key on the caller's own mount (`crates/sr-api/src/routes/storage/objects.rs`, `is_save_key`).
import { apiPost } from '../api/client';
import type { CoreId } from './cores';

export const SAVES_PREFIX = 'emu/saves/';
/** Slots of save states a member can use besides the automatic one. */
export const STATE_SLOTS = [1, 2, 3, 4] as const;
export type StateSlot = (typeof STATE_SLOTS)[number];
export type SaveName = 'sram' | `state${StateSlot}`;

const HASH = /^[0-9a-f]{40}$/;
export const isRomHash = (s: string): boolean => HASH.test(s);

/** The folder of one ROM's saves. */
export function savePrefix(core: CoreId, romHash: string): string {
  if (!isRomHash(romHash)) throw new Error('the ROM hash must be 40 lower-case hex digits');
  return `${SAVES_PREFIX}${core}/${romHash}/`;
}

export function saveKey(core: CoreId, romHash: string, name: SaveName): string {
  return `${savePrefix(core, romHash)}${name === 'sram' ? 'sram.srm' : `${name}.state`}`;
}

export interface ParsedSaveKey {
  core: string;
  romHash: string;
  name: SaveName;
}

/** The inverse of [saveKey]; `null` for anything that is not a save key of this shape. */
export function parseSaveKey(key: string): ParsedSaveKey | null {
  const m = /^emu\/saves\/([a-z0-9_]+)\/([0-9a-f]{40})\/(sram\.srm|state([1-4])\.state)$/.exec(key);
  if (!m) return null;
  const name = (m[3] === 'sram.srm' ? 'sram' : `state${m[4]}`) as SaveName;
  return { core: m[1] ?? '', romHash: m[2] ?? '', name };
}

export interface MountLike {
  id: string;
  owner_kind: string;
  status: string;
  cors_ok_at: string | null;
}

/**
 * The mount saves go to: the member's own (never a server's shelf, never the instance bucket), working, with its CORS
 * check passed. A saved choice wins when it is still one of those.
 */
export function pickSaveMount<M extends MountLike>(mounts: readonly M[], preferred?: string | null): M | null {
  const ok = mounts.filter((m) => m.owner_kind === 'user' && m.status === 'active' && m.cors_ok_at);
  return ok.find((m) => m.id === preferred) ?? ok[0] ?? null;
}

export interface SaveIo {
  presign: (mountId: string, key: string, method: 'GET' | 'PUT') => Promise<{ url: string }>;
  fetch: typeof fetch;
}

export const realSaveIo: SaveIo = {
  presign: (mountId, key, method) =>
    apiPost<{ url: string }>(`/storage/mounts/${encodeURIComponent(mountId)}/presign`, { key, method }),
  fetch: (...a) => fetch(...a),
};

/** Write a save. Throws when the bucket refuses. */
export async function putSave(
  mountId: string,
  key: string,
  bytes: Uint8Array,
  io: SaveIo = realSaveIo,
): Promise<void> {
  if (!parseSaveKey(key)) throw new Error('not a save key');
  const { url } = await io.presign(mountId, key, 'PUT');
  const body = new Blob([bytes as BlobPart], { type: 'application/octet-stream' });
  const res = await io.fetch(url, { method: 'PUT', body, cache: 'no-store' });
  if (!res.ok) throw new Error(`the bucket refused the save (${res.status})`);
}

/** Read a save; `null` when there is none yet. */
export async function getSave(
  mountId: string,
  key: string,
  io: SaveIo = realSaveIo,
): Promise<Uint8Array | null> {
  if (!parseSaveKey(key)) throw new Error('not a save key');
  const { url } = await io.presign(mountId, key, 'GET');
  const res = await io.fetch(url, { cache: 'no-store' });
  if (res.status === 404 || res.status === 403) return null;
  if (!res.ok) throw new Error(`the bucket did not answer (${res.status})`);
  return new Uint8Array(await res.arrayBuffer());
}
