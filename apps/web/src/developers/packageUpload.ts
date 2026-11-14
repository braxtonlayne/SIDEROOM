// SEC-AUD-177 (D-654, upload half): how the developer page sends an Activity package. The page reads the
// application's limits, then picks one of three paths: a file of at most 32 MiB goes in one request
// (`uploadActivityBundle`, the only path on a store that cannot sign parts); a bigger file is cut into parts
// that go straight to the bucket (the Library's UploadManager does the sending: four parts at once, three
// retries with a pause, a ledger in IndexedDB so a refresh sends only the missing parts); the server then
// checks the stored zip in a job and the page polls it every 2 s until the bundle is ready or refused.
import { ApiError, apiFetch, apiPost } from '../api/client';
import { t } from '../i18n/runtime';
import { IdbLedger as SharedIdbLedger } from '../lib/uploadLedger';
import {
  MemoryLedger,
  PART_CONCURRENCY,
  PART_RETRIES,
  UploadManager,
  putPartWithFetch,
  type Ledger,
  type LedgerEntry,
  type UploadApi,
} from '../review/uploadManager';
import { getActivityBundle, MAX_BUNDLE_BYTES, uploadActivityBundle } from './api';
import type { ActivityBundleDetail } from './types';

export type PackageTier = 'standard' | 'reviewed' | 'platform';

/** `GET /applications/{id}/activity/package-limits` (byte counts are decimal strings on the wire). */
export interface PackageLimits {
  max_bytes: string;
  max_files: number;
  tier: PackageTier;
  resumable: boolean;
  part_size: number;
}

interface PartUrl {
  part_number: number;
  url: string;
}

export type UploadState = 'uploading' | 'checking' | 'ready' | 'refused' | 'expired';

/** `GET .../uploads/{id}` and the answer of `complete`. */
export interface UploadStatus {
  state: UploadState;
  received: number[];
  parts?: PartUrl[];
  checked_bytes: string;
  total_bytes: string;
  bundle_id?: string;
  error?: string;
}

interface UploadSlot {
  upload_id: string;
  part_size: number;
  part_count: number;
  parts: PartUrl[];
}

/** Below this the single-request route is used; above it the parts route (the server's body limit for the former). */
export const SINGLE_REQUEST_MAX = MAX_BUNDLE_BYTES;
/** The status poll while the server checks the package. */
export const POLL_MS = 2000;
/** The ledger row `submissionId` holds the application id; the manager's chapter is always this. */
const CHAPTER = 'package';

export type PackagePhase = 'sending' | 'checking';
export interface PackageProgress {
  phase: PackagePhase;
  /** Bytes sent so far. */
  sent: number;
  total: number;
  /** Bytes the checker has read (phase `checking`). */
  checked: number;
}

/** The server (or this page) says the package cannot be taken; the text is the reason, to show as it is. */
export class PackageRefused extends Error {}
/** The developer cancelled. */
export class PackageCancelled extends Error {}

const id = encodeURIComponent;
const ro = { retries: 0 } as const;
const base = (app: string) => `/applications/${id(app)}/activity`;

export const getPackageLimits = (app: string) =>
  apiFetch<PackageLimits>(`${base(app)}/package-limits`, ro);
const beginUpload = (app: string, body: { size: string; filename: string }) =>
  apiPost<UploadSlot>(`${base(app)}/uploads`, body, ro);
export const getUploadStatus = (app: string, upload: string) =>
  apiFetch<UploadStatus>(`${base(app)}/uploads/${id(upload)}`, ro);
const completeUpload = (app: string, upload: string) =>
  apiPost<UploadStatus>(`${base(app)}/uploads/${id(upload)}/complete`, undefined, ro);
export const abortPackageUpload = (app: string, upload: string) =>
  apiFetch<void>(`${base(app)}/uploads/${id(upload)}`, { method: 'DELETE', ...ro });

/** "512 MiB", "4 GiB": whole numbers print without a decimal. */
export function formatBytes(n: number): string {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let v = n;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u += 1;
  }
  return `${Number.isInteger(v) ? v : v.toFixed(1)} ${units[u]}`;
}

const tierWord = (tier: PackageTier) =>
  tier === 'platform'
    ? t('developers.packageUpload.tierPlatform')
    : tier === 'reviewed'
      ? t('developers.packageUpload.tierReviewed')
      : t('developers.packageUpload.tierStandard');

/** The one line under the upload button, built from the limits answer. */
export function limitLine(l: PackageLimits): string {
  const max = formatBytes(Number(l.max_bytes));
  return l.tier === 'standard'
    ? t('developers.packageUpload.limitStandard', { max })
    : t('developers.packageUpload.limitTier', { max, tier: tierWord(l.tier) });
}

/** The browser's own refusal of a file over the limit: names the limit and the tier. */
export function overLimitText(size: number, l: PackageLimits): string {
  return t('developers.packageUpload.overLimit', {
    size: formatBytes(size),
    max: formatBytes(Number(l.max_bytes)),
    tier: tierWord(l.tier),
  });
}

/** Which way a file goes, decided from the limits alone. */
export type PackagePath = 'single' | 'parts' | 'over-limit' | 'no-parts';
export function choosePath(
  size: number,
  l: PackageLimits,
  singleMax = SINGLE_REQUEST_MAX,
): PackagePath {
  if (size > Number(l.max_bytes)) return 'over-limit';
  if (size <= singleMax) return 'single';
  return l.resumable ? 'parts' : 'no-parts';
}

/** The ledger of package uploads: IndexedDB `package` store of the shared database. */
export class PackageIdbLedger extends SharedIdbLedger<LedgerEntry> {
  constructor(name?: string) {
    super('package', name);
  }
}

export interface UploaderOptions {
  ledger?: Ledger;
  /** Sends one part (tests replace it). */
  put?: (url: string, body: Blob, signal: AbortSignal) => Promise<void>;
  /** Pause between part attempts. */
  wait?: (attempt: number) => Promise<void>;
  /** Pause between status polls. */
  poll?: () => Promise<void>;
  concurrency?: number;
  retries?: number;
  /** Tests: the size up to which the single request is used. */
  singleMax?: number;
}

export class PackageUploader {
  private readonly ledger: Ledger;
  private readonly manager: UploadManager;
  private readonly pollWait: () => Promise<void>;
  /** The open upload, for cancel. */
  private current: { app: string; upload: string | null } | null = null;
  private cancelled = false;
  private readonly singleMax: number;
  private onProgress: (p: PackageProgress) => void = () => undefined;

  constructor(o: UploaderOptions = {}) {
    this.ledger =
      o.ledger ?? (typeof indexedDB === 'undefined' ? new MemoryLedger() : new PackageIdbLedger());
    this.singleMax = o.singleMax ?? SINGLE_REQUEST_MAX;
    this.pollWait = o.poll ?? (() => new Promise((r) => setTimeout(r, POLL_MS)));
    const api: UploadApi = {
      begin: async (app, _chapter, b) => {
        let slot: UploadSlot;
        try {
          slot = await beginUpload(app, { size: String(b.size_bytes), filename: b.filename });
        } catch (e) {
          // 409: the store cannot sign parts (a stable answer; the page says so rather than guessing).
          if (e instanceof ApiError && e.status === 409)
            throw new PackageRefused(t('developers.packageUpload.noParts'));
          throw e;
        }
        this.current = { app, upload: slot.upload_id };
        return {
          upload: { id: slot.upload_id },
          multipart: {
            part_size: slot.part_size,
            part_count: slot.part_count,
            parts: slot.parts,
          },
        } as unknown as Awaited<ReturnType<UploadApi['begin']>>;
      },
      parts: async (app, upload) => {
        const s = await getUploadStatus(app, upload);
        return {
          part_size: 0,
          part_count: 0,
          received: s.received.map((part_number) => ({ part_number, size: 0 })),
          missing: s.parts ?? [],
          expires_in_secs: 0,
        };
      },
      complete: (app, upload) => completeUpload(app, upload),
      abort: (app, upload) => abortPackageUpload(app, upload),
    };
    this.manager = new UploadManager(api, this.ledger, {
      concurrency: o.concurrency ?? PART_CONCURRENCY,
      retries: o.retries ?? PART_RETRIES,
      put: o.put ?? putPartWithFetch,
      ...(o.wait ? { wait: o.wait } : {}),
    });
    this.manager.subscribe(() => {
      const task = this.manager.snapshot().find((x) => x.id === this.current?.upload);
      if (task && task.state !== 'done')
        this.onProgress({
          phase: 'sending',
          sent: Math.round(task.progress * task.size),
          total: task.size,
          checked: 0,
        });
    });
  }

  /** Upload `file` for application `app`; resolves with the stored draft bundle. */
  async upload(
    app: string,
    file: File,
    onProgress: (p: PackageProgress) => void,
    limits?: PackageLimits,
  ): Promise<ActivityBundleDetail> {
    this.onProgress = onProgress;
    this.cancelled = false;
    this.current = { app, upload: null };
    const l = limits ?? (await getPackageLimits(app));
    const path = choosePath(file.size, l, this.singleMax);
    if (path === 'over-limit') throw new PackageRefused(overLimitText(file.size, l));
    if (path === 'no-parts') throw new PackageRefused(t('developers.packageUpload.noParts'));
    if (path === 'single') {
      return uploadActivityBundle(app, file, (f) =>
        onProgress({
          phase: 'sending',
          sent: Math.round(f * file.size),
          total: file.size,
          checked: 0,
        }),
      );
    }
    try {
      await this.manager.enqueue(app, CHAPTER, file);
    } catch (e) {
      if (this.cancelled) throw new PackageCancelled(t('developers.packageUpload.cancelled'));
      throw e;
    }
    return this.finish(app, this.current.upload!);
  }

  /**
   * After a refresh: continue the upload the ledger remembers for `app` (sending only the parts the server
   * lacks), or pick up its check. Resolves null when there is nothing to continue.
   */
  async resume(
    app: string,
    onProgress: (p: PackageProgress) => void,
  ): Promise<{ filename: string; bundle: Promise<ActivityBundleDetail> } | null> {
    this.onProgress = onProgress;
    this.cancelled = false;
    const row = (await this.ledger.all()).find((r) => r.submissionId === app);
    if (!row) return null;
    let status: UploadStatus;
    try {
      status = await getUploadStatus(app, row.uploadId);
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        await this.ledger.remove(row.id);
        return null;
      }
      throw e;
    }
    if (status.state === 'expired') {
      await this.ledger.remove(row.id);
      return null;
    }
    this.current = { app, upload: row.uploadId };
    const run = async (): Promise<ActivityBundleDetail> => {
      if (status.state === 'uploading') {
        const done = new Promise<void>((resolve, reject) => {
          const off = this.manager.subscribe(() => {
            const task = this.manager.snapshot().find((x) => x.id === row.id);
            if (task?.state === 'done') {
              off();
              resolve();
            } else if (task?.state === 'failed') {
              off();
              reject(new Error(task.error ?? t('developers.packageUpload.failed')));
            }
          });
        });
        await this.manager.resumeAll();
        try {
          await done;
        } catch (e) {
          if (this.cancelled) throw new PackageCancelled(t('developers.packageUpload.cancelled'));
          throw e;
        }
      }
      await this.ledger.remove(row.id);
      return this.finish(app, row.uploadId);
    };
    return { filename: row.filename, bundle: run() };
  }

  /** Poll the check to its end. */
  private async finish(app: string, upload: string): Promise<ActivityBundleDetail> {
    this.current = { app, upload };
    let failures = 0;
    for (;;) {
      let s: UploadStatus;
      try {
        s = await getUploadStatus(app, upload);
        failures = 0;
      } catch (e) {
        if (this.cancelled) throw new PackageCancelled(t('developers.packageUpload.cancelled'));
        if (e instanceof ApiError && e.status === 404)
          throw new PackageRefused(t('developers.packageUpload.gone'));
        if (++failures >= 5) throw e;
        await this.pollWait();
        continue;
      }
      if (s.state === 'ready' && s.bundle_id) return getActivityBundle(app, s.bundle_id);
      if (s.state === 'refused')
        throw new PackageRefused(s.error ?? t('developers.packageUpload.checkFailed'));
      if (s.state === 'expired') throw new PackageRefused(t('developers.packageUpload.expired'));
      this.onProgress({
        phase: 'checking',
        sent: Number(s.total_bytes),
        total: Number(s.total_bytes),
        checked: Number(s.checked_bytes),
      });
      await this.pollWait();
      if (this.cancelled) throw new PackageCancelled(t('developers.packageUpload.cancelled'));
    }
  }

  /** True once the server holds an upload that Cancel can abort (the parts route). */
  canCancel(): boolean {
    return this.current?.upload != null;
  }

  /** Give the upload up: the parts are aborted at the server and the ledger forgets it. */
  async cancel(): Promise<void> {
    this.cancelled = true;
    const cur = this.current;
    if (!cur?.upload) return;
    await this.manager.cancel(cur.upload);
    try {
      await abortPackageUpload(cur.app, cur.upload);
    } catch {
      /* the server expires an abandoned upload on its own */
    }
    await this.ledger.remove(cur.upload);
  }
}
