// Keys the browser builds from what the person picked (BYOS-BRW-001). The server checks every key again
// (`sr_storage::path::safe_key`: no empty or dot segments, no leading slash, no control characters or
// backslashes, at most 1024 bytes); these helpers keep a bad name from ever being sent.

const MAX_KEY_BYTES = 1024;
const bytes = (s: string) => new TextEncoder().encode(s).length;
const CONTROL = /[\u0000-\u001f\u007f\\]/;

/** One name (a file or folder, no slash in it): non-empty, not `.` or `..`, no control characters. */
export function validName(name: string): boolean {
  return name !== '' && name !== '.' && name !== '..' && !name.includes('/') && !CONTROL.test(name);
}

/**
 * The key for a picked file: `prefix` (empty, or ending in `/`) plus the file's path, or `null` when the
 * name cannot be used. `path` uses `/` and may carry a leading `./`.
 */
export function uploadKey(prefix: string, path: string): string | null {
  const parts = path.replace(/^\.\//, '').split('/');
  if (parts.length === 0 || !parts.every(validName)) return null;
  const key = `${prefix}${parts.join('/')}`;
  return bytes(key) <= MAX_KEY_BYTES ? key : null;
}

/** The folder key for a new folder called `name` inside `prefix`, or `null` when the name is unusable. */
export function folderKey(prefix: string, name: string): string | null {
  const n = name.trim();
  if (!validName(n)) return null;
  const key = `${prefix}${n}/`;
  return bytes(key) <= MAX_KEY_BYTES ? key : null;
}

/** The path a person edits in the rename/move box: a folder's trailing slash is hidden. */
export const editablePath = (key: string) => (key.endsWith('/') ? key.slice(0, -1) : key);

/**
 * The target key for a move from what was typed: a folder stays a folder (trailing slash), leading and trailing
 * spaces and slashes go, empty segments collapse. `null` when it cannot be a key.
 */
export function moveTarget(from: string, typed: string): string | null {
  const folder = from.endsWith('/');
  const parts = typed
    .trim()
    .split('/')
    .filter((p) => p !== '');
  if (parts.length === 0 || !parts.every(validName)) return null;
  const key = parts.join('/') + (folder ? '/' : '');
  return bytes(key) <= MAX_KEY_BYTES ? key : null;
}

/** The last part of a key, for names on screen. */
export const baseName = (key: string) => key.split('/').filter(Boolean).pop() ?? key;
