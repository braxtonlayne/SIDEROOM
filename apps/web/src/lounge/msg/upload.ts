/**
 * Attachment upload (LNG-CMP-005/007): request one slot per file, PUT each file to its presigned URL with
 * progress and cancel, then confirm each upload so the server verifies the bytes. The ids that come back
 * are referenced by the message (see UploadedFile in ../data.ts).
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

interface Slot {
  id: string;
  upload_id: string;
  upload_url: string;
  upload_filename: string;
  max_size: number;
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
  file: File,
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
}

/**
 * Uploads every file and resolves with the references to put in the message.
 * Each file's state is reported through `onUpdate`. Throws UploadError when any file fails.
 */
export async function uploadFiles(
  channelId: string,
  files: PendingFile[],
  { onUpdate, signal }: UploadHandlers,
): Promise<UploadedFile[]> {
  let slots: Slot[];
  try {
    const res = await apiFetch<{ attachments: Slot[] }>(`/channels/${channelId}/attachments`, {
      method: 'POST',
      retries: 0,
      json: {
        files: files.map((f) => ({
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
    slots = res.attachments;
  } catch (e) {
    const msg = e instanceof Error ? e.message : 'Could not start the upload.';
    for (const f of files) onUpdate(f.id, { status: 'error', error: msg });
    throw new UploadError(msg, (e as { status?: number }).status);
  }

  const results: UploadedFile[] = [];
  let next = 0;
  const worker = async () => {
    for (;;) {
      const i = next++;
      const f = files[i];
      if (!f) return;
      const slot = slots.find((s) => s.id === f.id);
      if (!slot) {
        onUpdate(f.id, { status: 'error', error: 'No upload slot.' });
        throw new UploadError('No upload slot.');
      }
      onUpdate(f.id, { status: 'uploading', progress: 0, error: undefined });
      try {
        await putFile(slot.upload_url, f.file, (p) => onUpdate(f.id, { progress: p }), signal);
        await apiFetch(`/channels/${channelId}/attachments/${slot.upload_id}/complete`, {
          method: 'POST',
          json: {},
          retries: 0,
        });
      } catch (e) {
        const cancelled = signal.aborted;
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
