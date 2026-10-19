/**
 * Attachment upload (LNG-CMP-005/007): request one slot per file, PUT each file to its presigned URL with
 * progress and cancel, then confirm each upload so the server verifies the bytes. The ids that come back
 * are referenced by the message (see UploadedFile in ../data.ts).
 *
 * Files over 64 MB come back from the server as a multipart slot (LNG-CMP-007): the file is cut into the
 * announced parts, up to three go out at once, a failed part is retried with a pause, and when a part keeps
 * failing (network loss) the client asks the server which parts arrived and sends only the rest. The slot is
 * remembered in localStorage, so after a reload, sending the same file again in the same channel continues
 * the old upload instead of starting over.
 */
import { apiFetch } from '../../api/client';
import type { UploadedFile } from '../data';

export const MAX_FILES = 10;

export interface PendingFile {
  /** Client id, echoed in the slot response. */
  id: string;
  file: File;
  /** Name sent to the server (SPOILER_ prefix applied by `uploadName`). */
  name: string;
  spoiler: boolean;
  description: string;
  progress: number;
  status: 'queued' | 'uploading' | 'done' | 'error' | 'cancelled';
  error?: string | undefined;
  /** Pixel size and BlurHash of an image, read in the browser (see blurhash.ts). */
  meta?: { width: number; height: number; blurhash?: string | undefined } | undefined;
}

/** Discord marks spoilers by a filename prefix; the renderer reads it back. */
export function uploadName(name: string, spoiler: boolean): string {
  const bare = name.replace(/^SPOILER_/, '');
  return spoiler ? `SPOILER_${bare}` : bare;
}

export interface PartUrl {
  part_number: number;
  url: string;
}

interface MultipartSlot {
  part_size: number;
  part_count: number;
  parts: PartUrl[];
}

interface UploadParts {
  part_size: number;
  part_count: number;
  received: { part_number: number; size: number }[];
  missing: PartUrl[];
}

interface Slot {
  id: string;
  upload_id: string;
  upload_url: string;
  upload_filename: string;
  max_size: number;
  multipart?: MultipartSlot | null;
}

export const PART_CONCURRENCY = 3;
export const PART_RETRIES = 3;
/** How many times a stalled upload asks the server what arrived before it gives up. */
export const RESUME_ROUNDS = 4;

const defaultWait = (attempt: number) =>
  new Promise<void>((r) => setTimeout(r, Math.min(500 * 2 ** attempt, 8000)));

/** Resolves when the browser is online again (at once when it already is or cannot tell). */
function whenOnline(signal: AbortSignal): Promise<void> {
  if (typeof navigator === 'undefined' || navigator.onLine !== false) return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      window.removeEventListener('online', done);
      signal.removeEventListener('abort', done);
      resolve();
    };
    window.addEventListener('online', done);
    signal.addEventListener('abort', done);
  });
}

// ---- Resume records -------------------------------------------------------------------------

const RESUME_KEY = 'sr.upload.resume';

interface ResumeRecord {
  uploadId: string;
  uploadFilename: string;
  maxSize: number;
  savedAt: number;
}

/** Identifies "the same file in the same channel" without keeping the file itself. */
export function resumeKey(channelId: string, file: File): string {
  return `${channelId}:${file.name}:${file.size}:${file.lastModified}`;
}

const RESUME_MAX_AGE_MS = 20 * 3600 * 1000; // the server sweeps abandoned slots after 24 h

function readResume(): Record<string, ResumeRecord> {
  try {
    const raw = localStorage.getItem(RESUME_KEY);
    const all = raw ? (JSON.parse(raw) as Record<string, ResumeRecord>) : {};
    const now = Date.now();
    return Object.fromEntries(
      Object.entries(all).filter(([, r]) => now - r.savedAt < RESUME_MAX_AGE_MS),
    );
  } catch {
    return {};
  }
}

function writeResume(all: Record<string, ResumeRecord>): void {
  try {
    localStorage.setItem(RESUME_KEY, JSON.stringify(all));
  } catch {
    /* storage blocked or full: the upload still works, it just cannot resume after a reload */
  }
}

function saveResume(key: string, r: Omit<ResumeRecord, 'savedAt'>): void {
  writeResume({ ...readResume(), [key]: { ...r, savedAt: Date.now() } });
}

function forgetResume(key: string): void {
  const all = readResume();
  if (key in all) {
    delete all[key];
    writeResume(all);
  }
}

export class UploadError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
  }
}

/** PUT one file with progress. Rejects with UploadError; aborts when `signal` fires. */
export function putFile(
  url: string,
  file: Blob,
  onProgress: (fraction: number) => void,
  signal: AbortSignal,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('PUT', url);
    if (file.type) xhr.setRequestHeader('content-type', file.type);
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable && e.total > 0) onProgress(e.loaded / e.total);
    };
    xhr.onload = () =>
      xhr.status >= 200 && xhr.status < 300
        ? resolve()
        : reject(new UploadError(`Upload failed (${xhr.status}).`, xhr.status));
    xhr.onerror = () => reject(new UploadError('The upload could not be completed.'));
    xhr.onabort = () => reject(new UploadError('Upload cancelled.'));
    const abort = () => xhr.abort();
    if (signal.aborted) return abort();
    signal.addEventListener('abort', abort, { once: true });
    xhr.send(file);
  });
}

export interface UploadHandlers {
  onUpdate: (id: string, patch: Partial<PendingFile>) => void;
  signal: AbortSignal;
  /** Sends one part (tests replace it). */
  putPart?: typeof putFile;
  /** Pause before retry number `attempt` (tests make it instant). */
  wait?: (attempt: number) => Promise<void>;
}

/** What is known about a multipart upload while its parts go out. */
interface PartState {
  size: number;
  count: number;
  done: Set<number>;
  urls: Map<number, string>;
}

const fetchParts = (channelId: string, uploadId: string) =>
  apiFetch<UploadParts>(`/channels/${channelId}/attachments/${uploadId}/parts`, { retries: 0 });

function stateFromParts(p: UploadParts): PartState {
  return {
    size: p.part_size,
    count: p.part_count,
    done: new Set(p.received.map((r) => r.part_number)),
    urls: new Map(p.missing.map((m) => [m.part_number, m.url])),
  };
}

/**
 * Sends the parts that are not yet at the server. A part is retried with a pause; when parts keep failing
 * the server is asked what it holds (and for fresh URLs) and only the rest is sent, up to RESUME_ROUNDS times.
 */
async function sendParts(
  channelId: string,
  uploadId: string,
  file: Blob,
  st: PartState,
  h: UploadHandlers,
  report: (fraction: number) => void,
): Promise<void> {
  const { signal } = h;
  const put = h.putPart ?? putFile;
  const wait = h.wait ?? defaultWait;
  const sizeOf = (n: number) => (n === st.count ? file.size - st.size * (st.count - 1) : st.size);
  const inflight = new Map<number, number>();
  const emit = () => {
    let bytes = 0;
    for (const n of st.done) bytes += sizeOf(n);
    for (const v of inflight.values()) bytes += v;
    report(file.size > 0 ? Math.min(1, bytes / file.size) : 1);
  };
  const cancelled = () => new UploadError('Upload cancelled.');
  const sendOne = async (n: number): Promise<boolean> => {
    for (let attempt = 0; ; attempt++) {
      if (signal.aborted) throw cancelled();
      await whenOnline(signal);
      const url = st.urls.get(n);
      if (!url) return false;
      const start = (n - 1) * st.size;
      try {
        await put(
          url,
          file.slice(start, start + sizeOf(n)),
          (f) => {
            inflight.set(n, f * sizeOf(n));
            emit();
          },
          signal,
        );
        inflight.delete(n);
        st.done.add(n);
        emit();
        return true;
      } catch (e) {
        inflight.delete(n);
        if (signal.aborted) throw e;
        if (attempt + 1 >= PART_RETRIES) return false;
        await wait(attempt);
      }
    }
  };
  emit();
  for (let round = 0; round < RESUME_ROUNDS; round++) {
    const todo = [...st.urls.keys()].filter((n) => !st.done.has(n)).sort((a, b) => a - b);
    if (todo.length === 0) return;
    let next = 0;
    let failed = 0;
    const lane = async () => {
      for (;;) {
        const n = todo[next++];
        if (n === undefined) return;
        if (!(await sendOne(n))) failed++;
      }
    };
    await Promise.all(Array.from({ length: Math.min(PART_CONCURRENCY, todo.length) }, lane));
    if (failed === 0) return;
    if (signal.aborted) throw cancelled();
    await wait(round);
    try {
      const fresh = stateFromParts(await fetchParts(channelId, uploadId));
      st.done = fresh.done;
      st.urls = fresh.urls;
      emit();
    } catch {
      /* still offline: the next round tries the old URLs again */
    }
  }
  throw new UploadError(
    'The upload was interrupted. Send the file again to continue where it stopped.',
  );
}

/** A file's slot as the upload loop uses it. */
interface Plan {
  slot: Pick<Slot, 'id' | 'upload_id' | 'upload_filename'> & Partial<Slot>;
  parts?: PartState;
}

/**
 * Uploads every file and resolves with the references to put in the message.
 * Each file's state is reported through `onUpdate`. Throws UploadError when any file fails.
 */
export async function uploadFiles(
  channelId: string,
  files: PendingFile[],
  h: UploadHandlers,
): Promise<UploadedFile[]> {
  const { onUpdate, signal } = h;
  const plans = new Map<string, Plan>();

  // A multipart upload of the same file that was interrupted earlier continues where it stopped.
  const saved = readResume();
  for (const f of files) {
    const rec = saved[resumeKey(channelId, f.file)];
    if (!rec) continue;
    try {
      const parts = stateFromParts(await fetchParts(channelId, rec.uploadId));
      plans.set(f.id, {
        slot: {
          id: f.id,
          upload_id: rec.uploadId,
          upload_filename: rec.uploadFilename,
          max_size: rec.maxSize,
        },
        parts,
      });
    } catch {
      forgetResume(resumeKey(channelId, f.file)); // finished, swept or never real: start fresh
    }
  }

  const fresh = files.filter((f) => !plans.has(f.id));
  if (fresh.length > 0) {
    try {
      const res = await apiFetch<{ attachments: Slot[] }>(`/channels/${channelId}/attachments`, {
        method: 'POST',
        retries: 0,
        json: {
          files: fresh.map((f) => ({
            id: f.id,
            filename: uploadName(f.name, f.spoiler),
            file_size: f.file.size,
            content_type: f.file.type || undefined,
            spoiler: f.spoiler || undefined,
            description: f.description || undefined,
            width: f.meta?.width,
            height: f.meta?.height,
            blurhash: f.meta?.blurhash,
          })),
        },
      });
      for (const s of res.attachments) {
        const f = fresh.find((x) => x.id === s.id);
        if (!f) continue;
        const mp = s.multipart;
        plans.set(s.id, {
          slot: s,
          ...(mp
            ? {
                parts: {
                  size: mp.part_size,
                  count: mp.part_count,
                  done: new Set<number>(),
                  urls: new Map(mp.parts.map((p) => [p.part_number, p.url])),
                },
              }
            : {}),
        });
        if (mp) {
          saveResume(resumeKey(channelId, f.file), {
            uploadId: s.upload_id,
            uploadFilename: s.upload_filename,
            maxSize: s.max_size,
          });
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Could not start the upload.';
      for (const f of files) onUpdate(f.id, { status: 'error', error: msg });
      throw new UploadError(msg, (e as { status?: number }).status);
    }
  }

  const results: UploadedFile[] = [];
  let next = 0;
  const worker = async () => {
    for (;;) {
      const i = next++;
      const f = files[i];
      if (!f) return;
      const plan = plans.get(f.id);
      if (!plan) {
        onUpdate(f.id, { status: 'error', error: 'No upload slot.' });
        throw new UploadError('No upload slot.');
      }
      const { slot } = plan;
      const key = resumeKey(channelId, f.file);
      onUpdate(f.id, { status: 'uploading', progress: 0, error: undefined });
      try {
        if (plan.parts) {
          await sendParts(channelId, slot.upload_id, f.file, plan.parts, h, (p) =>
            onUpdate(f.id, { progress: p }),
          );
        } else if (slot.upload_url) {
          await putFile(slot.upload_url, f.file, (p) => onUpdate(f.id, { progress: p }), signal);
        }
        await apiFetch(`/channels/${channelId}/attachments/${slot.upload_id}/complete`, {
          method: 'POST',
          json: {},
          retries: 0,
        });
        forgetResume(key);
      } catch (e) {
        const cancelled = signal.aborted;
        if (cancelled && plan.parts) {
          // Cancel frees the parts already stored; a network failure keeps them for the resume.
          forgetResume(key);
          void apiFetch(`/channels/${channelId}/attachments/${slot.upload_id}`, {
            method: 'DELETE',
            retries: 0,
          }).catch(() => undefined);
        }
        onUpdate(f.id, {
          status: cancelled ? 'cancelled' : 'error',
          error: e instanceof Error ? e.message : 'Upload failed.',
        });
        throw e;
      }
      onUpdate(f.id, { status: 'done', progress: 1 });
      results[i] = {
        id: slot.id,
        uploadedFilename: slot.upload_filename,
        ...(f.description ? { description: f.description } : {}),
      };
    }
  };
  const pool = Math.min(3, files.length);
  await Promise.all(Array.from({ length: pool }, worker));
  return results;
}
