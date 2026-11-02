// @ts-check
// Where a game's models and textures come from, behind one interface. A game asks an `AssetSource` for bytes,
// JSON or an image by path and does not care whether they come from the bundle or, later, from a member's shelf.
//
// - BundledAssetSource: files shipped in the bundle, by relative URL (assets/... next to this file). Fetching
//   needs `connect-src 'self'`, which the manifest gets by declaring `"connect": ["self"]` (the sandbox CSP is
//   `connect-src 'none'` otherwise); images load through <img> (img-src 'self') and need nothing extra.
// - ShelfAssetSource: models from a member's shelf (`byos_read`, D-389): SIDE ROOM's `shelfObjectUrl` command gives a
//   short-lived URL for one object, which this fetches. Outside SIDE ROOM, or for a guest, it rejects with
//   AssetNotAvailableError.

/**
 * @typedef {object} AssetSource
 * @property {string} kind
 * @property {(path: string) => Promise<Uint8Array>} bytes
 * @property {(path: string) => Promise<any>} json
 * @property {(path: string) => Promise<ImageBitmap>} image
 */

export class AssetNotAvailableError extends Error {
  /** @param {string} message */
  constructor(message) {
    super(message);
    this.name = 'AssetNotAvailableError';
  }
}

/**
 * A path inside the bundle's asset folder: relative, no scheme, no '..', no backslash, no leading slash.
 * @param {string} path @returns {string | null} why it is refused, or null
 */
export function assetPathProblem(path) {
  if (typeof path !== 'string' || path === '') return 'is empty';
  if (/^[a-z][a-z0-9+.-]*:/i.test(path) || path.startsWith('//')) return 'is an absolute URL';
  if (path.startsWith('/')) return 'is an absolute path';
  if (path.includes('\\')) return 'uses a backslash';
  if (path.split('/').some((s) => s === '..' || s === '.' || s === '')) return 'has an empty, . or .. segment';
  return null;
}

/** @implements {AssetSource} */
export class BundledAssetSource {
  /**
   * @param {{ base?: string | URL, fetch?: typeof fetch, doc?: Document | null }} [opt]
   *   base: the folder the paths are relative to (default: this file's folder, assets/ in the bundle)
   */
  constructor(opt = {}) {
    this.kind = 'bundle';
    this.base = new URL(String(opt.base ?? new URL('./', import.meta.url)));
    this.fetchImpl = opt.fetch ?? ((/** @type {any} */ u, /** @type {any} */ o) => fetch(u, o));
    this.doc = opt.doc === undefined ? (typeof document === 'undefined' ? null : document) : opt.doc;
  }

  /** @param {string} path */
  url(path) {
    const why = assetPathProblem(path);
    if (why) throw new Error(`asset path ${JSON.stringify(path)} ${why}`);
    return new URL(path, this.base);
  }

  /** @param {string} path */
  async bytes(path) {
    const res = await this.fetchImpl(this.url(path).href, { credentials: 'omit' });
    if (!res.ok) throw new Error(`asset ${path}: HTTP ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }

  /** @param {string} path */
  async json(path) {
    return JSON.parse(new TextDecoder().decode(await this.bytes(path)));
  }

  /** @param {string} path @returns {Promise<ImageBitmap>} */
  async image(path) {
    const url = this.url(path).href;
    if (this.doc) {
      // <img> is covered by img-src 'self'; crossOrigin keeps the pixels readable for WebGL/WebGPU (the bundle
      // host answers with Access-Control-Allow-Origin: *).
      const img = this.doc.createElement('img');
      img.crossOrigin = 'anonymous';
      img.src = url;
      await img.decode();
      return createImageBitmap(img);
    }
    const res = await this.fetchImpl(url, { credentials: 'omit' });
    if (!res.ok) throw new Error(`asset ${path}: HTTP ${res.status}`);
    return createImageBitmap(await res.blob());
  }
}

const SHELF_MESSAGE =
  "Reading from a member's shelf needs SIDE ROOM's shelfObjectUrl command (a member, not a guest, in an Activity that " +
  'declares byos_read). Ship the asset in the bundle, or run inside SIDE ROOM.';

/**
 * A shelf object's path: '<shelf id>/<object hash>' (the hash from the shelf's catalog).
 * @param {string} path @returns {{ shelf_id: string, object_hash: string } | null}
 */
export function shelfRef(path) {
  const m = /^([0-9]{1,20})\/([0-9a-f]{64})$/.exec(typeof path === 'string' ? path : '');
  return m ? { shelf_id: m[1], object_hash: m[2] } : null;
}

/**
 * Models and textures from a member's shelf (D-389, `byos_read`). Each read asks SIDE ROOM for a short-lived URL of
 * that one object (`sdk.commands.shelfObjectUrl`; the member is asked once per object) and fetches it. The URL is on
 * the shelf's storage host, so the fetch passes the sandbox CSP only when the manifest's `connect` list names that
 * host (CSP is per host; docs/dev/design/byos-read-activities.md). Guests never get shelf objects (D-397).
 * @implements {AssetSource}
 */
export class ShelfAssetSource {
  /**
   * @param {{ sdk?: { config?: { guest?: boolean, capabilities?: string[] } | null,
   *   commands?: { shelfObjectUrl?: (a: { shelf_id: string, object_hash: string }) => Promise<{ url: string }> } } | null,
   *   fetch?: typeof fetch }} [opt]
   */
  constructor(opt = {}) {
    this.kind = "shelf";
    this.sdk = opt.sdk ?? null;
    this.fetchImpl = opt.fetch ?? ((/** @type {any} */ u, /** @type {any} */ o) => fetch(u, o));
  }

  /** Whether this page can read shelf objects: a member's SDK with the command, and the byos_read capability. */
  get available() {
    const sdk = this.sdk;
    return !!(
      sdk &&
      typeof sdk.commands?.shelfObjectUrl === "function" &&
      sdk.config &&
      !sdk.config.guest &&
      sdk.config.capabilities?.includes("byos_read")
    );
  }

  /** @param {string} path "<shelf id>/<object hash>" @returns {Promise<Uint8Array>} */
  async bytes(path) {
    if (!this.available) throw new AssetNotAvailableError(SHELF_MESSAGE);
    const ref = shelfRef(path);
    if (!ref) throw new Error(`shelf path ${JSON.stringify(path)} is not "<shelf id>/<object hash>"`);
    const { url } = await /** @type {any} */ (this.sdk).commands.shelfObjectUrl(ref);
    const res = await this.fetchImpl(url, { credentials: "omit" });
    if (!res.ok) throw new Error(`shelf object ${path}: HTTP ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }

  /** @param {string} path @returns {Promise<any>} */
  async json(path) {
    return JSON.parse(new TextDecoder().decode(await this.bytes(path)));
  }

  /** @param {string} path @returns {Promise<ImageBitmap>} */
  async image(path) {
    const data = await this.bytes(path);
    return createImageBitmap(new Blob([data]));
  }
}
