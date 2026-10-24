// The upload manager of the Cinema submit screen (RDY-MED-002): the same resumable manager and
// IndexedDB ledger as the Library (review/uploadManager.ts), pointed at the Cinema routes. The ledger
// lives in its own IndexedDB database so the Library manager never resumes a video upload (or the
// other way round) after a refresh.
import {
  IdbLedger,
  MemoryLedger,
  UploadManager,
  putPartWithFetch,
  type UploadApi,
} from '../../review/uploadManager';
import type { MultipartUploadSlot } from '../../review/apiP13';
import * as api from './api';

/** Where a file goes, in the one string the manager keeps per upload (`chapterId` in its ledger). */
export type Target =
  /** Let the server read the name (a film, or `Show - s01e02.mkv`). */
  | { kind: 'auto' }
  | { kind: 'numbers'; season: number; episode: number }
  | { kind: 'episode'; id: string };

export const encodeTarget = (t: Target): string =>
  t.kind === 'auto' ? 'auto' : t.kind === 'episode' ? `ep:${t.id}` : `se:${t.season}:${t.episode}`;

export function decodeTarget(s: string): Target {
  if (s.startsWith('ep:')) return { kind: 'episode', id: s.slice(3) };
  const m = /^se:(\d+):(\d+)$/.exec(s);
  if (m) return { kind: 'numbers', season: Number(m[1]), episode: Number(m[2]) };
  return { kind: 'auto' };
}

/** The manager's view of the Cinema routes. The upload id is the media asset id. */
export const cinemaUploadApi: UploadApi = {
  async begin(submissionId, target, b) {
    const t = decodeTarget(target);
    const slot = await api.beginFile(submissionId, {
      ...b,
      ...(t.kind === 'numbers' ? { season: t.season, episode: t.episode } : {}),
      ...(t.kind === 'episode' ? { episode_id: t.id } : {}),
    });
    return {
      upload: { id: slot.asset_id },
      multipart: { part_size: slot.part_size, part_count: slot.part_count, parts: slot.parts },
    } as unknown as MultipartUploadSlot;
  },
  parts: (submissionId, uploadId) => api.uploadParts(submissionId, uploadId),
  complete: (submissionId, uploadId) => api.completeUpload(submissionId, uploadId),
  abort: (submissionId, uploadId) => api.abortUpload(submissionId, uploadId),
};

export const LEDGER_DB = 'sideroom-cinema-uploads';

let shared: UploadManager | null = null;

/** The page-wide manager. Browsers without IndexedDB keep the ledger in memory (no resume after a refresh). */
export function getCinemaUploadManager(): UploadManager {
  if (!shared) {
    const ledger = typeof indexedDB === 'undefined' ? new MemoryLedger() : new IdbLedger(LEDGER_DB);
    shared = new UploadManager(cinemaUploadApi, ledger, { put: putPartWithFetch });
  }
  return shared;
}

/** Replace the manager (tests). */
export function setCinemaUploadManager(m: UploadManager | null) {
  shared = m;
}
