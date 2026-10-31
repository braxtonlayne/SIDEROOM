// ACT-EMU-001: what the shell asks the server. The ROM list is `GET /users/@me/games/roms` (the member's own mounts and
// the servers' shelves where a role holds `view_games`); bytes never pass through the instance: a presigned GET goes to
// the bucket and the browser reads it itself.
import { apiFetch, apiPost } from '../api/client';
import { listObjects } from '../storage/api';
import { listMounts } from '../settings/user/storage/api';
import type { Mount } from '../settings/user/storage/types';
import { BIOS_PREFIX } from './bios';
import { isPlatform, type Platform } from './cores';
import { isRomName } from './detect';

export interface RomItem {
  id: string;
  mount_id: string;
  key: string;
  size: number;
  title: string;
  platform?: string | null;
  core?: string | null;
  /** `own` or `guild`. */
  scope: 'own' | 'guild';
  mount_label: string;
  guild_id?: string | null;
}

export const romPlatform = (r: RomItem): Platform | null => (isPlatform(r.platform) ? r.platform : null);

export const listRoms = () =>
  apiFetch<{ roms: RomItem[] }>('/users/@me/games/roms', { retries: 0 }).then((r) => r.roms);

export const listOwnMounts = (): Promise<Mount[]> => listMounts({ kind: 'user' });

export const presignRead = (mountId: string, key: string) =>
  apiPost<{ url: string }>(`/storage/mounts/${encodeURIComponent(mountId)}/presign`, {
    key,
    method: 'GET',
  });

export interface ReadIo {
  presign: (mountId: string, key: string) => Promise<{ url: string }>;
  fetch: typeof fetch;
}

export const realReadIo: ReadIo = {
  presign: presignRead,
  fetch: (...a) => fetch(...a),
};

/** Read one object with a presigned GET, reporting progress. Uncached: a ROM or BIOS is not kept by the browser. */
export async function readObject(
  mountId: string,
  key: string,
  io: ReadIo = realReadIo,
  onProgress?: (done: number, total: number) => void,
): Promise<Uint8Array> {
  const { url } = await io.presign(mountId, key);
  const res = await io.fetch(url, { cache: 'no-store', credentials: 'omit' });
  if (!res.ok) throw new Error(`the bucket did not answer (${res.status})`);
  const total = Number(res.headers.get('content-length') ?? 0);
  if (!res.body || !onProgress) return new Uint8Array(await res.arrayBuffer());
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let done = 0;
  for (;;) {
    const { value, done: end } = await reader.read();
    if (end) break;
    if (value) {
      chunks.push(value);
      done += value.length;
      onProgress(done, total);
    }
  }
  const out = new Uint8Array(done);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

/** The files a `.cue` sheet names (`FILE "track 01.bin" BINARY`), without any folder part. */
export function cueFiles(cue: string): string[] {
  const out: string[] = [];
  for (const m of cue.matchAll(/^\s*FILE\s+(?:"([^"]+)"|(\S+))/gim)) {
    const name = (m[1] ?? m[2] ?? '').split(/[\\/]/).pop() ?? '';
    if (name && !out.includes(name)) out.push(name);
  }
  return out;
}

const dirOf = (key: string) => key.slice(0, key.lastIndexOf('/') + 1);

/** The siblings of a `.cue` (its tracks), read from the same folder of the same mount. */
export async function readCueSiblings(
  rom: Pick<RomItem, 'mount_id' | 'key'>,
  cueText: string,
  io: ReadIo = realReadIo,
): Promise<Array<{ name: string; bytes: Uint8Array }>> {
  const out: Array<{ name: string; bytes: Uint8Array }> = [];
  for (const name of cueFiles(cueText)) {
    out.push({ name, bytes: await readObject(rom.mount_id, `${dirOf(rom.key)}${name}`, io) });
  }
  return out;
}

export interface BiosCandidate {
  mountId: string;
  mountLabel: string;
  key: string;
  size: number;
}

/** The files in `emu/bios/` of the member's OWN mounts (the only place a BIOS may come from). */
export async function listBiosFiles(mounts: readonly Mount[]): Promise<BiosCandidate[]> {
  const out: BiosCandidate[] = [];
  for (const m of mounts) {
    if (m.owner_kind !== 'user' || m.status !== 'active' || !m.cors_ok_at) continue;
    try {
      const page = await listObjects(m.id, BIOS_PREFIX);
      for (const o of page.objects) {
        if (o.size === 512 * 1024) out.push({ mountId: m.id, mountLabel: m.label, key: o.key, size: o.size });
      }
    } catch {
      /* a mount that cannot be listed offers no BIOS */
    }
  }
  return out;
}

/** The ROMs of one folder of an own mount that the catalog has not indexed yet (a fresh upload), by name. */
export async function listRomsInBucket(mountId: string): Promise<RomItem[]> {
  const page = await listObjects(mountId, 'emu/roms/');
  return page.objects
    .filter((o) => isRomName(o.key))
    .map((o) => ({
      id: `${mountId}:${o.key}`,
      mount_id: mountId,
      key: o.key,
      size: o.size,
      title: (o.key.split('/').pop() ?? o.key).replace(/\.[^.]+$/, '').replace(/[_.]+/g, ' '),
      scope: 'own' as const,
      mount_label: '',
    }));
}
