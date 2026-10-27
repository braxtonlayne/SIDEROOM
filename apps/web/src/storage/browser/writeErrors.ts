import { ApiError } from '../../api/client';
import { t } from '../../i18n';
import { storageErrorText } from '../../settings/user/storage/errors';
import { serverCode } from './mountUploads';

const BY_CODE: Record<string, string> = {
  CORS_NOT_READY: 'storage.write.err.corsNotReady',
  TARGET_EXISTS: 'storage.write.err.targetExists',
  OBJECT_TOO_LARGE: 'storage.write.err.objectTooLarge',
  FOLDER_TOO_LARGE: 'storage.write.err.folderTooLarge',
  MOVE_INTO_ITSELF: 'storage.write.err.moveIntoItself',
  MOVE_SAME: 'storage.write.err.moveSame',
  KIND_MISMATCH: 'storage.write.err.kindMismatch',
  BUCKET_UNREACHABLE: 'storage.write.err.bucketUnreachable',
  MOUNT_ENDPOINT_BLOCKED: 'storage.write.err.bucketUnreachable',
  TOO_MANY_UPLOADS: 'storage.write.err.tooManyUploads',
  UPLOAD_INCOMPLETE: 'storage.write.err.incomplete',
  FILE_TOO_LARGE: 'storage.write.err.fileTooLarge',
  EMPTY_FILE: 'storage.write.err.emptyFile',
  KEY_INVALID: 'storage.write.err.keyInvalid',
};

/**
 * Words for what the write routes answer. The stable code sits at `errors.<field>._errors[0].code` (409 and 400
 * answers of the storage routes), so it is read from there; anything unknown falls back to the read side's wording.
 */
export function writeErrorText(e: unknown): string {
  const code = serverCode(e);
  const key = code ? BY_CODE[code] : undefined;
  if (key) return t(key as Parameters<typeof t>[0]);
  if (e instanceof ApiError && e.status === 429) return t('storage.write.err.tooMany');
  if (e instanceof ApiError && e.status === 403) return t('storage.write.err.notAllowed');
  return storageErrorText(e);
}

/** The line under a failed upload in the dock: the server's code in words, else what the browser said. */
export function taskErrorText(task: { code?: string; error?: string }): string {
  const key = task.code ? BY_CODE[task.code] : undefined;
  if (key) return t(key as Parameters<typeof t>[0]);
  return task.error ?? '';
}
