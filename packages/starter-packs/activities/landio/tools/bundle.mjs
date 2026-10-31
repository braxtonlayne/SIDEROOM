// @ts-check
// Builds landio.zip: the static Activity bundle SIDE ROOM serves from its own origin (from tools/pack.js of the
// standalone game, activity-landio 63c6c93d). The bundle rules are those of crates/sr-activities/src/bundle.rs, and
// the content checks follow the CSP the proxy sends. A bundle the server would refuse fails here first, and
// nothing is written. Differences from the standalone pack: host/ (the game engine that runs in the host's browser)
// is bundled too, the Embedded App SDK is vendored from packages/embedded-app-sdk as sdk/*.js (transpiled, no
// build step in the bundle), and activity.json names the activity and the `datachannel` capability it needs.
//
//   node tools/bundle.mjs            writes dist/landio.zip
//   node tools/bundle.mjs --dir out  also writes the files, as served, into ./out (for a static server / Vite on 5177)

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { crc32, deflateRawSync } from 'node:zlib';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** @typedef {{ path: string, data: Buffer }} BundleFile */

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** The Embedded App SDK's TypeScript sources, which the bundle carries transpiled. */
export const SDK_SRC = resolve(ROOT, '..', '..', '..', 'embedded-app-sdk', 'src');

const MAX_ENTRIES = 2000;
const MAX_BYTES = 33_554_432;
const MAX_PATH_BYTES = 256;
const MAX_SEGMENT_BYTES = 128;
/** 1980-01-01, so the archive is the same from one build to the next. */
const DOS_DATE = 0x21;

const ALLOWED = new Set([
  'html', 'js', 'mjs', 'css', 'json', 'map', 'wasm', 'png', 'jpg', 'jpeg', 'webp', 'gif', 'svg', 'ico',
  'woff2', 'mp3', 'ogg', 'webm', 'txt',
]);

const EXTERNAL_URL = /(?:https?|wss?):\/\/|url\(\s*['"]?\/\/|(?:src|href)\s*=\s*['"]?\/\//i;
/**
 * Absolute URLs the bundle may name as quoted strings: Discord's client origins, which the Discord platform
 * compares message origins with (nothing is loaded from them, and the CSP's connect-src 'self' would refuse it).
 */
const ORIGIN_LITERALS = [
  'https://discord.com',
  'https://ptb.discord.com',
  'https://canary.discord.com',
  'https://discordapp.com',
  'https://ptb.discordapp.com',
  'https://canary.discordapp.com',
  'https://staging.discord.co',
  'https://pax.discord.com',
].flatMap((o) => [`'${o}'`, `"${o}"`]);
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
const SEGMENT_OK = /^[A-Za-z0-9._~@+-]+$/;
const SCHEME = /^\s*(?:[a-z][a-z0-9+.-]*:|\/\/|\\)/i;
const EVENT_HANDLER = /[\s"'/]on[a-z]+\s*=/i;
const URL_ATTRS = ['src', 'href', 'poster', 'action', 'formaction', 'data'];

/** What activity.json says: the activity's identity and the data-channel capability it needs (ACT-CH-001). */
export const ACTIVITY_MANIFEST = {
  id: 'landio',
  name: 'Land.io',
  version: '1.0.0',
  entry: 'index.html',
  description: 'Draw a trail out of your land, come home to claim what you enclosed, and cut other players off. Hosted in the browser of the member who starts it.',
  capabilities: ['datachannel'],
  min_participants: 1,
  max_participants: 24,
};

/**
 * The Embedded App SDK as the bundle carries it: its TypeScript sources (tests excluded) transpiled to ES2020 with
 * the import specifiers given '.js', as e2e/support/activityBundle.ts does for the test activity.
 * @param {string} [srcDir]
 * @returns {BundleFile[]}
 */
export function vendorSdk(srcDir = SDK_SRC) {
  const ts = createRequire(join(srcDir, '..', 'package.json'))('typescript');
  /** @type {BundleFile[]} */
  const out = [];
  for (const name of readdirSync(srcDir).filter((n) => n.endsWith('.ts') && !n.endsWith('.test.ts')).sort()) {
    const js = ts.transpileModule(readFileSync(join(srcDir, name), 'utf8'), {
      compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ES2020, removeComments: true },
    }).outputText;
    const base = name.replace(/\.ts$/, '');
    out.push({ path: `sdk/${base}.js`, data: Buffer.from(js.replace(/(from\s+|import\s+)'\.\/(\w+)'/g, "$1'./$2.js'"), 'utf8') });
  }
  return out;
}

/**
 * client/ is served at the bundle root, so a client file's imports of ../shared/ or ../host/ would climb out of
 * the root on disk (browsers clamp that to the root, Vite and other tools do not). They are rewritten to the
 * path they have in the bundle.
 * @param {string} text the source of client/<rel>
 * @param {string} rel the file's path under client/
 */
export function rewriteClientImports(text, rel) {
  const depth = rel.split('/').length - 1;
  const up = depth === 0 ? './' : '../'.repeat(depth);
  return text.replace(
    /(\bfrom\s*|\bimport\s*\(?\s*)(['"])(?:\.\.\/)+(shared|host)\//g,
    (_m, kw, q, dir) => `${kw}${q}${up}${dir}/`,
  );
}

/**
 * Every file under client/ at the zip root, every *.js file under shared/ (cards, maps, modes too) under shared/,
 * the host engine under host/, the vendored SDK under sdk/, and activity.json.
 * Sorted by path so the zip is the same on every build.
 * @param {string} root
 * @param {{ sdk?: BundleFile[] }} [opt]
 * @returns {BundleFile[]}
 */
export function collectFiles(root, opt = {}) {
  /** @type {BundleFile[]} */
  const files = [];
  const clientDir = join(root, 'client');
  for (const rel of listFiles(clientDir, '')) {
    const data = readFileSync(join(clientDir, ...rel.split('/')));
    files.push({ path: rel, data: rel.endsWith('.js') ? Buffer.from(rewriteClientImports(data.toString('utf8'), rel), 'utf8') : data });
  }
  for (const dir of ['shared', 'host']) {
    const abs = join(root, dir);
    for (const rel of listFiles(abs, '')) {
      if (rel.endsWith('.js')) files.push({ path: `${dir}/${rel}`, data: readFileSync(join(abs, ...rel.split('/'))) });
    }
  }
  files.push(...(opt.sdk ?? vendorSdk()));
  files.push({ path: 'activity.json', data: Buffer.from(`${JSON.stringify(ACTIVITY_MANIFEST, null, 2)}
`, 'utf8') });
  files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return files;
}

/**
 * @param {string} dir
 * @param {string} prefix
 * @returns {string[]}
 */
function listFiles(dir, prefix) {
  if (!existsSync(dir)) return [];
  /** @type {string[]} */
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const rel = prefix === '' ? entry.name : `${prefix}/${entry.name}`;
    if (entry.isDirectory()) out.push(...listFiles(join(dir, entry.name), rel));
    else if (entry.isFile()) out.push(rel);
  }
  return out;
}

/**
 * The same checks as `path_problem` in bundle.rs, in the same order.
 * @param {string} path
 * @returns {string | null} why the path is refused, or null
 */
export function pathProblem(path) {
  if (path === '') return 'is empty';
  if (Buffer.byteLength(path) > MAX_PATH_BYTES) return 'is longer than 256 characters';
  if (path.startsWith('/')) return 'is an absolute path';
  if (path.includes('\\')) return 'uses a backslash';
  if (path.includes('%')) return 'contains an encoded character';
  if (CONTROL.test(path)) return 'contains a control character';
  for (const seg of path.split('/')) {
    if (seg === '') return 'has an empty path segment';
    if (seg === '.' || seg === '..') return 'leaves the bundle with . or ..';
    if (seg.startsWith('.')) return 'has a hidden file or directory';
    if (Buffer.byteLength(seg) > MAX_SEGMENT_BYTES) return 'has a segment longer than 128 characters';
    if (!SEGMENT_OK.test(seg)) return 'uses characters other than letters, digits and . _ - ~ @ +';
  }
  return null;
}

/**
 * @param {string} path
 * @returns {string} the lower-case extension, or '' when there is none
 */
function extensionOf(path) {
  const name = path.slice(path.lastIndexOf('/') + 1);
  const dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.slice(dot + 1).toLowerCase();
}

/**
 * Resolve a URL or specifier written in the file at `fromPath` to a bundle path, with URL semantics ('../'
 * above the root stays at the root). `null` when it cannot be decoded.
 * @param {string} fromPath
 * @param {string} ref
 * @returns {string | null}
 */
function resolveInBundle(fromPath, ref) {
  try {
    const url = new URL(ref, `https://bundle.invalid/${fromPath}`);
    return decodeURIComponent(url.pathname.slice(1));
  } catch {
    return null;
  }
}

/**
 * The value of attribute `name` in one start tag, or `undefined` when it is absent.
 * @param {string} tag
 * @param {string} name
 * @returns {string | undefined}
 */
function attrValue(tag, name) {
  const m = new RegExp(`(?:^|\\s)${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s"'>]+))`, 'i').exec(tag);
  return m ? (m[1] ?? m[2] ?? m[3] ?? '') : undefined;
}

/**
 * The CSP and reference checks for one HTML page: no inline script, no event handlers, no javascript: URLs,
 * no non-relative URLs, and every script src and stylesheet href must be in the bundle.
 * @param {BundleFile} file
 * @param {Set<string>} names
 * @param {string[]} problems
 */
function checkHtml(file, names, problems) {
  const path = file.path;
  const text = file.data.toString('utf8');
  const lower = text.toLowerCase();

  const scriptOpen = /<script\b([^>]*)>/gi;
  let m;
  while ((m = scriptOpen.exec(text)) !== null) {
    const bodyStart = scriptOpen.lastIndex;
    const close = lower.indexOf('</script', bodyStart);
    const body = text.slice(bodyStart, close < 0 ? text.length : close);
    const hasSrc = attrValue(m[1], 'src') !== undefined;
    if (!hasSrc || /\S/.test(body)) problems.push(`${path}: has an inline script`);
  }

  for (const tag of text.match(/<[a-zA-Z][^>]*>/g) ?? []) {
    if (EVENT_HANDLER.test(tag)) problems.push(`${path}: has an inline event handler`);
    for (const name of URL_ATTRS) {
      const value = attrValue(tag, name);
      if (value !== undefined && SCHEME.test(value)) {
        problems.push(`${path}: ${name}="${value}" is not a relative or same-origin path`);
      }
    }
    const tagName = /^<([a-zA-Z][\w-]*)/.exec(tag)?.[1]?.toLowerCase();
    const ref = tagName === 'script' ? attrValue(tag, 'src') : tagName === 'link' ? attrValue(tag, 'href') : undefined;
    if (ref !== undefined) {
      const target = resolveInBundle(path, ref);
      if (target === null || !names.has(target)) {
        problems.push(`${path}: ${tagName} ${ref} is not in the bundle`);
      }
    }
  }
  if (/javascript\s*:/i.test(text)) problems.push(`${path}: has a javascript: URL`);
}

/**
 * The eval and import checks for one module: no eval or new Function, and every import or export-from must be
 * a relative or root path that is in the bundle.
 * @param {BundleFile} file
 * @param {Set<string>} names
 * @param {string[]} problems
 */
function checkModule(file, names, problems) {
  const path = file.path;
  const text = file.data.toString('utf8');
  if (/\beval\s*\(/.test(text) || /\bnew\s+Function\s*\(/.test(text)) {
    problems.push(`${path}: calls eval or new Function`);
  }
  const specs = [
    /\bimport\b\s*(?:[\w$*{},\s]*\bfrom\s*)?(['"])([^'"\n]+)\1/g,
    /\bexport\b\s*[\w$*{},\s]*\bfrom\s*(['"])([^'"\n]+)\1/g,
    /\bimport\s*\(\s*(['"])([^'"\n]+)\1\s*\)/g,
  ];
  for (const re of specs) {
    for (const hit of text.matchAll(re)) {
      const spec = hit[2];
      if (!/^(?:\.\.?\/|\/)/.test(spec)) {
        problems.push(`${path}: imports ${spec}, which is not a './', '../' or '/' path`);
        continue;
      }
      const target = resolveInBundle(path, spec);
      if (target === null || !names.has(target)) {
        problems.push(`${path}: imports ${spec}, which is not in the bundle`);
      }
    }
  }
}

/**
 * Every rule of bundle.rs and the Activity CSP over a set of bundle files. An empty list means the bundle is
 * acceptable.
 * @param {BundleFile[]} files
 * @returns {string[]}
 */
export function checkBundle(files) {
  /** @type {string[]} */
  const problems = [];
  if (files.length > MAX_ENTRIES) {
    problems.push(`the bundle has ${files.length} entries, more than ${MAX_ENTRIES}`);
  }

  const names = new Set(files.map((f) => f.path));
  /** @type {Map<string, string>} */
  const lowered = new Map();
  let total = 0;
  for (const file of files) {
    total += file.data.length;
    const path = file.path;
    const problem = pathProblem(path);
    if (problem !== null) problems.push(`${path}: ${problem}`);

    const key = path.toLowerCase();
    const prior = lowered.get(key);
    if (prior === undefined) lowered.set(key, path);
    else if (prior === path) problems.push(`${path}: is listed twice`);
    else problems.push(`${path}: differs from ${prior} only by case`);

    const parts = path.split('/');
    for (let i = 1; i < parts.length; i++) {
      if (names.has(parts.slice(0, i).join('/'))) {
        problems.push(`${path}: sits below a path that is also a file`);
        break;
      }
    }

    const ext = extensionOf(path);
    if (!ALLOWED.has(ext)) problems.push(`${path}: has a file type that is not allowed`);
    if (ext === 'html') checkHtml(file, names, problems);
    if (ext === 'js' || ext === 'mjs') checkModule(file, names, problems);
    if (ext === 'html' || ext === 'js' || ext === 'mjs' || ext === 'css') {
      let text = file.data.toString('utf8');
      for (const lit of ORIGIN_LITERALS) text = text.split(lit).join("''");
      if (EXTERNAL_URL.test(text)) {
        problems.push(`${path}: contains an absolute external URL`);
      }
    }
  }

  if (!names.has('index.html')) problems.push('index.html is missing at the root of the bundle');
  if (total > MAX_BYTES) {
    problems.push(`the bundle unpacks to ${total} bytes, more than ${MAX_BYTES} (32 MiB)`);
  }
  const zipSize = buildZip(files).length;
  if (zipSize > MAX_BYTES) {
    problems.push(`the zip is ${zipSize} bytes, more than ${MAX_BYTES} (32 MiB)`);
  }
  return problems;
}

/**
 * A zip of the given files, deflated where that is smaller and stored otherwise. Regular files only, with
 * UTF-8 names, Unix modes and a fixed date. The archive starts at byte 0.
 * @param {BundleFile[]} files
 * @returns {Buffer}
 */
export function buildZip(files) {
  /** @type {Buffer[]} */
  const parts = [];
  /** @type {Buffer[]} */
  const centrals = [];
  let offset = 0;
  for (const file of files) {
    const name = Buffer.from(file.path, 'utf8');
    const crc = crc32(file.data);
    const deflated = deflateRawSync(file.data, { level: 9 });
    const method = deflated.length < file.data.length ? 8 : 0;
    const body = method === 8 ? deflated : file.data;

    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(0, 10);
    local.writeUInt16LE(DOS_DATE, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(file.data.length, 22);
    local.writeUInt16LE(name.length, 26);
    parts.push(local, name, body);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(0x031e, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(method, 10);
    central.writeUInt16LE(DOS_DATE, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(body.length, 20);
    central.writeUInt32LE(file.data.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE((0o100644 << 16) >>> 0, 38);
    central.writeUInt32LE(offset, 42);
    centrals.push(central, name);

    offset += 30 + name.length + body.length;
  }
  const directory = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, directory, end]);
}

export function main(argv = process.argv.slice(2)) {
  const files = collectFiles(ROOT);
  const zip = buildZip(files);
  const problems = checkBundle(files);
  let total = 0;
  for (const file of files) {
    total += file.data.length;
    console.log(`  ${file.path} (${file.data.length} bytes)`);
  }
  console.log(`entries: ${files.length}`);
  console.log(`unpacked bytes: ${total}`);
  console.log(`zip bytes: ${zip.length}`);
  console.log(`sha256: ${createHash('sha256').update(zip).digest('hex')}`);
  if (problems.length > 0) {
    for (const problem of problems) console.error(`problem: ${problem}`);
    console.error(`not written: ${problems.length} problem(s)`);
    process.exitCode = 1;
    return;
  }
  const dist = join(ROOT, 'dist');
  mkdirSync(dist, { recursive: true });
  const out = join(dist, 'landio.zip');
  writeFileSync(out, zip);
  console.log(`wrote ${out}`);
  const at = argv.indexOf('--dir');
  if (at >= 0 && argv[at + 1]) {
    const dir = resolve(argv[at + 1]);
    for (const f of files) {
      const dest = join(dir, ...f.path.split('/'));
      mkdirSync(dirname(dest), { recursive: true });
      writeFileSync(dest, f.data);
    }
    console.log(`wrote ${files.length} files into ${dir}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
