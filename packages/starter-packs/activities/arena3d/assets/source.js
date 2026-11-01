// @ts-check
// Where a game's models and textures come from, behind one interface. A game asks an `AssetSource` for bytes,
// JSON or an image by path and does not care whether they come from the bundle or, later, from a member's shelf.
//
// - BundledAssetSource: files shipped in the bundle, by relative URL (assets/... next to this file). Fetching
//   needs `connect-src 'self'`, which the manifest gets by declaring `"connect": ["self"]` (the sandbox CSP is
//   `connect-src 'none'` otherwise); images load through <img> (img-src 'self') and need nothing extra.
// - ShelfAssetSource: models from a member's shelf (`byos_read`, through short-lived per-object URLs). The SDK
//   has no command for that yet, so every call rejects with AssetNotAvailableError. It exists so a game can be
//   written against the interface now.

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
  'Reading models and textures from a member\'s shelf (byos_read) is not available yet: the Embedded App SDK has ' +
  'no command that hands an activity a short-lived URL for one shelf object. Ship the asset in the bundle for now.';

/** @implements {AssetSource} */
export class ShelfAssetSource {
  /** @param {{ sdk?: unknown }} [opt] the SDK the future command will go through */
  constructor(opt = {}) {
    this.kind = 'shelf';
    this.sdk = opt.sdk ?? null;
  }

  /** Always false until the SDK has the command. */
  static get available() {
    return false;
  }

  /** @param {string} _path @returns {Promise<Uint8Array>} */
  async bytes(_path) {
    throw new AssetNotAvailableError(SHELF_MESSAGE);
  }

  /** @param {string} _path @returns {Promise<any>} */
  async json(_path) {
    throw new AssetNotAvailableError(SHELF_MESSAGE);
  }

  /** @param {string} _path @returns {Promise<ImageBitmap>} */
  async image(_path) {
    throw new AssetNotAvailableError(SHELF_MESSAGE);
  }
}
