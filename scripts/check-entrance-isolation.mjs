#!/usr/bin/env node
// Entrance isolation check (SEC-ENT-004, D-385; docs/dev/entrance-isolation.md section 8).
//
//   node scripts/check-entrance-isolation.mjs [distDir] [--quiet] [--repo=<checkout the dist was built from>]
//
// The owner's rule: a visitor without a login receives only the code their page needs, and that code tells them
// nothing about what is inside. The Web job runs this after the build, beside the bundle budget. For every public
// surface in dist/surfaces.json it checks the shell's closure against:
//   1. the surface's gzip budget (JavaScript, static imports),
//   2. the surface's allowed-module list (src/public/<surface>/ALLOWED.ts, read through the public build's Vite manifest),
//   3. the generated denylist (chunk names, route paths and API paths of the app, message-key prefixes of the
//      non-core catalogs) plus the hand-kept scripts/entrance-denylist.json,
//   4. no source map, no preload or dynamic import outside /entrance/ and /fonts/, no reference to the app,
//   5. and it scans the plain-HTML surfaces (no script, no handler, no fetch) and the public static files.
// scripts/entrance-allow.json lists tolerated matches; every entry needs a reason. Exit 1 on any failure.
//
// The contract with the build (SEC-ENT-001), fixed by the design:
//   dist/surfaces.json          copy of apps/web/public-surfaces.json
//   dist/index.html, auth.html, invite.html   public shells (script/link tags point at /entrance/...)
//   dist/entrance/**            the public build (its manifest at dist/entrance/.vite/manifest.json or dist/.vite/manifest.json)
//   dist/app.html, dist/app/**  the app build; dist/app/routes.json (route paths), dist/app/csp.txt
//   dist/legal/*.html           plain-HTML house notes per locale
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, dirname, join, posix, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { staticImports } from './lib/bundle.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..');

/** Core catalog fragments (the public build registers these only). */
export const CORE_FRAGMENTS = ['entrance', 'a11y', 'pwa'];
/** Chunk names too generic to be evidence of anything. */
const GENERIC_CHUNKS = new Set([
  'index',
  'main',
  'vendor',
  'chunk',
  'client',
  'utils',
  'types',
  'core',
  'common',
  'shared',
  'store',
  'api',
  'react',
  'router',
  'entry',
  'polyfills',
]);

/**
 * The API a visitor's pages may call (docs/dev/entrance-isolation.md sections 4 and 7): the sign-in family, the
 * registration helpers, the entrance copy and the invite and template lookups. API paths under these prefixes are
 * not denylist tokens; every other operation of openapi.json is.
 */
export const PUBLIC_API_PREFIXES = [
  '/sr/auth/',
  '/sr/register',
  '/sr/usernames',
  '/sr/age-gate',
  '/sr/csrf',
  '/sr/compliance/public',
  '/sr/entrance/',
  '/sr/invites',
  '/guilds/templates/',
];
/** Exact paths the entrance calls that sit beside app paths; a match inside them is excused (see `excusedBy`). */
export const PUBLIC_API_EXACT = [
  {
    token: '/users/@me',
    reason: 'section 4: the hand-off asks whether a session exists (useRedirectIfLoggedIn)',
  },
];

const isWordChar = (c) => c !== undefined && /[A-Za-z0-9_]/.test(c);

// ---------------------------------------------------------------------------------------------------------------
// Reading the build

function walk(dir, filter = () => true) {
  const out = [];
  if (!existsSync(dir)) return out;
  const stack = [dir];
  while (stack.length) {
    const d = stack.pop();
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) stack.push(p);
      else if (filter(p)) out.push(p);
    }
  }
  return out.sort();
}

const toPosix = (p) => p.split('\\').join('/');
const rel = (dist, p) => toPosix(relative(dist, p));

/** `Foo-AbC123_x.js` -> `Foo`; leaves names without a hash alone. */
export function chunkBase(file) {
  const name = basename(file).replace(/\.(?:js|mjs|css|wasm|map)$/, '');
  return name.replace(/-[A-Za-z0-9_-]{6,10}$/, '').replace(/\.[A-Za-z0-9_-]{6,10}$/, '');
}

/** The part of a path template before the first parameter: `/channels/$id/x` -> `/channels`, `/a/{b}` -> `/a`. */
export function staticPrefix(path) {
  const cut = path.search(/[$:{*]/);
  let p = cut === -1 ? path : path.slice(0, cut);
  if (p.length > 1) p = p.replace(/\/$/, '');
  return p;
}

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

/** `src="/entrance/x.js"` and friends in a shell, as dist-relative paths. */
export function shellRefs(html) {
  const refs = [];
  for (const m of html.matchAll(/<(script|link)\b([^>]*)>/gi)) {
    const tag = m[1].toLowerCase();
    const attrs = m[2];
    const url = /(?:src|href)\s*=\s*"([^"]+)"/i.exec(attrs)?.[1];
    if (!url) continue;
    const relAttr = /\brel\s*=\s*"([^"]+)"/i.exec(attrs)?.[1]?.toLowerCase() ?? '';
    refs.push({ tag, rel: relAttr, url, attrs });
  }
  return refs;
}

const distPath = (url) => url.replace(/^\/+/, '').split(/[?#]/)[0];

function closure(dist, roots) {
  const seen = new Set();
  const stack = [...roots];
  while (stack.length) {
    const f = stack.pop();
    if (seen.has(f) || !existsSync(join(dist, f)) || !/\.m?js$/.test(f)) continue;
    seen.add(f);
    stack.push(...staticImports(f, readFileSync(join(dist, f), 'utf8')));
  }
  return seen;
}

/** Targets of `import("...")` in a built file, resolved to dist-relative paths ('' when computed). */
export function dynamicImports(file, code) {
  const out = [];
  for (const m of code.matchAll(/\bimport\(\s*(["'`])([^"'`]*)\1\s*\)/g)) {
    const spec = m[2];
    out.push(spec.startsWith('/') ? spec.slice(1) : posix.join(posix.dirname(file), spec));
  }
  const computed = [...code.matchAll(/\bimport\(\s*(?!["'`])/g)].length;
  return { targets: out, computed };
}

// ---------------------------------------------------------------------------------------------------------------
// The denylist

function loadHandDenylist(file) {
  const j = readJson(file);
  const words = (j.words ?? []).map((w) => (typeof w === 'string' ? { token: w } : w));
  for (const w of words) if (!w.token) throw new Error(`${file}: an entry has no token`);
  return words.map((w) => ({ token: w.token, kind: 'word', note: w.note ?? '' }));
}

/** Message-key prefixes ("lounge", "help", ...) of the catalog fragments that are not core. */
export function nonCorePrefixes(messagesDir) {
  const core = new Set();
  const other = new Set();
  if (!existsSync(messagesDir)) return [];
  for (const f of readdirSync(messagesDir).filter((x) => x.endsWith('.ts'))) {
    const stem = f.replace(/\.ts$/, '');
    const isCore = CORE_FRAGMENTS.includes(stem);
    const text = readFileSync(join(messagesDir, f), 'utf8');
    for (const m of text.matchAll(/^\s*['"]([a-z][A-Za-z0-9]*)\.[^'"]+['"]\s*:/gm)) {
      (isCore ? core : other).add(m[1]);
    }
  }
  return [...other].filter((p) => !core.has(p) && p.length >= 4).sort();
}

export function buildDenylist({ dist, repoRoot, surfaces, handFile }) {
  const tokens = [];
  const notes = [];
  const add = (token, kind, source) => tokens.push({ token, kind, source });

  // Public chunk names are shared vocabulary by definition.
  const publicNames = new Set(
    walk(join(dist, 'entrance'), (p) => /\.(js|css|wasm)$/.test(p)).map(chunkBase),
  );

  // 1. chunk base names of the app build
  const appDir = join(dist, 'app');
  const appFiles = walk(appDir, (p) => /\.(js|css|wasm)$/.test(p));
  if (appFiles.length === 0) {
    notes.push('dist/app has no files: the app build is missing (run node scripts/build-web.mjs)');
  }
  for (const name of new Set(appFiles.map(chunkBase))) {
    if (name.length < 4 || GENERIC_CHUNKS.has(name.toLowerCase()) || publicNames.has(name))
      continue;
    // A reference to the file (`Name-<hash>.js`) always counts; the bare name only when it is a compound
    // identifier (LoungeRoot, UserSettings), because a lone word (Account, Home, theme) is also plain English.
    add(name, 'chunkref', 'dist/app chunk name');
    if (
      /[a-z][A-Z]/.test(name) ||
      /[A-Z].*[A-Z]/.test(name) ||
      (name.includes('-') && name.length >= 6)
    ) {
      add(name, 'chunk', 'dist/app chunk name');
    }
  }

  // 2. the app's route paths (minus the neutral ones and any public surface's own paths)
  const publicPrefixes = new Set(
    ['/', '/app', '/login', ...surfaces.flatMap((s) => s.paths ?? [])].map(staticPrefix),
  );
  const routesFile = join(appDir, 'routes.json');
  if (existsSync(routesFile)) {
    const j = readJson(routesFile);
    const list = Array.isArray(j) ? j : (j.paths ?? j.routes ?? []);
    for (const p of new Set(list.map((x) => staticPrefix(String(x))))) {
      if (p.length < 3 || publicPrefixes.has(p)) continue;
      add(p, 'route', 'dist/app/routes.json');
    }
  } else {
    notes.push('dist/app/routes.json is missing: no route paths in the generated denylist');
  }

  // 3. the API's operations whose tag is not auth
  const openapi = [
    join(repoRoot, 'crates', 'sr-api', 'openapi.json'),
    join(repoRoot, 'packages', 'api-client', 'openapi.json'),
  ].find(existsSync);
  if (openapi) {
    const doc = readJson(openapi);
    const seen = new Set();
    for (const [path, ops] of Object.entries(doc.paths ?? {})) {
      const methods = Object.values(ops).filter(
        (o) => o && typeof o === 'object' && !Array.isArray(o),
      );
      const allAuth =
        methods.length > 0 && methods.every((o) => (o.tags ?? []).some((t) => /^auth$/i.test(t)));
      if (allAuth) continue;
      const p = staticPrefix(path.replace(/^\/api\/v\d+/, ''));
      if (
        p.length < 5 ||
        seen.has(p) ||
        PUBLIC_API_PREFIXES.some((x) => (p + '/').startsWith(x) || p.startsWith(x))
      )
        continue;
      seen.add(p);
      add(p, 'api', `${basename(openapi)} ${path}`);
    }
  } else {
    notes.push('no openapi.json found: no API paths in the generated denylist');
  }

  // 4. message-key prefixes of the non-core catalog fragments
  for (const p of nonCorePrefixes(join(repoRoot, 'apps', 'web', 'src', 'i18n', 'messages', 'en'))) {
    add(`${p}.`, 'i18n', 'non-core catalog fragment');
  }

  // 5. the hand-kept list
  if (existsSync(handFile)) {
    for (const w of loadHandDenylist(handFile))
      add(w.token, 'word', 'scripts/entrance-denylist.json');
  } else {
    notes.push(`${handFile} is missing`);
  }

  // 6. structural references to the app, never allowed in a public file
  for (const t of ['/app/', 'app.html', '.wasm'])
    add(t, 'structural', 'a public file must not reference the app');

  return { tokens, notes };
}

// ---------------------------------------------------------------------------------------------------------------
// Scanning

/** First match of `tok` in `text` (and `lower` = text.toLowerCase()), or null. */
export function findToken(text, lower, tok, start = 0) {
  const { token, kind } = tok;
  if (kind === 'chunkref') {
    const re = new RegExp(
      `(?<![A-Za-z0-9_])${token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}-[A-Za-z0-9_-]{6,10}\\.(?:m?js|css|wasm)`,
      'g',
    );
    re.lastIndex = start;
    const m = re.exec(text);
    return m ? m.index : null;
  }
  const ci = kind === 'word';
  const hay = ci ? lower : text;
  const needle = ci ? token.toLowerCase() : token;
  let from = start;
  for (;;) {
    const i = hay.indexOf(needle, from);
    if (i === -1) return null;
    const before = text[i - 1];
    const after = text[i + needle.length];
    let ok = true;
    switch (kind) {
      case 'chunk':
      case 'word':
        ok =
          !(isWordChar(token[0]) && isWordChar(before)) &&
          !(isWordChar(token.at(-1)) && isWordChar(after));
        break;
      case 'route':
        ok =
          (before === '"' || before === "'" || before === '`') &&
          !/[A-Za-z0-9_-]/.test(after ?? '');
        break;
      case 'api':
        ok =
          (before === '"' ||
            before === "'" ||
            before === '`' ||
            /\/api\/v\d+$/.test(text.slice(0, i))) &&
          !/[A-Za-z0-9_-]/.test(after ?? '');
        break;
      case 'i18n':
        ok = before === '"' || before === "'" || before === '`';
        break;
      default:
        ok = true;
    }
    if (ok) return i;
    from = i + 1;
  }
}

const snippet = (text, i, len) =>
  text
    .slice(Math.max(0, i - 24), i + len + 24)
    .replace(/\s+/g, ' ')
    .trim();

function loadAllow(file) {
  const problems = [];
  if (!existsSync(file)) return { entries: [], problems };
  const j = readJson(file);
  const entries = j.allow ?? [];
  entries.forEach((e, n) => {
    const where = `${basename(file)} entry ${n + 1}`;
    if (!e.token || typeof e.token !== 'string') problems.push(`${where} has no token`);
    if (!e.surface || typeof e.surface !== 'string') problems.push(`${where} has no surface`);
    if (!e.reason || !String(e.reason).trim()) {
      problems.push(
        `${where} (${e.surface}: ${e.token}) has no reason; every tolerated match needs one`,
      );
    }
  });
  return { entries, problems };
}

/** The allow entries that apply to this surface and file. */
function scoped(entries, surface, file) {
  return entries.filter(
    (e) =>
      e.token && (e.surface === surface || e.surface === '*') && (!e.file || file.includes(e.file)),
  );
}

/**
 * Is the match at [i, i+len) inside an occurrence of an allowed string? An entry excuses the denylist token it
 * contains where it occurs inside the entry itself (`/users/@me` excuses `/users` there and nowhere else).
 */
function excusedBy(entries, text, lower, i, tok) {
  const ci = tok.kind === 'word';
  const hay = ci ? lower : text;
  const needle = ci ? tok.token.toLowerCase() : tok.token;
  for (const e of entries) {
    const a = ci ? e.token.toLowerCase() : e.token;
    for (let k = a.indexOf(needle); k !== -1; k = a.indexOf(needle, k + 1)) {
      if (hay.startsWith(a, i - k) && i - k >= 0) return e;
    }
  }
  return null;
}

/** Reads ALLOWED.ts: every quoted string that looks like a repo path or package. */
export function readAllowedModules(allowedTs) {
  const text = readFileSync(allowedTs, 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '');
  return [...text.matchAll(/(["'`])([^"'`\n]+)\1/g)]
    .map((m) => m[2].trim())
    .filter((s) => s && !/\s/.test(s))
    .flatMap(expandBraces);
}

/** `a/{B,C}` -> `a/B`, `a/C` (the design writes its lists that way). */
export function expandBraces(s) {
  const m = /^(.*)\{([^{}]*)\}(.*)$/.exec(s);
  if (!m) return [s];
  return m[2].split(',').flatMap((x) => expandBraces(`${m[1]}${x.trim()}${m[3]}`));
}

/** Is a manifest `src` (e.g. "src/routes/entrance/Landing.tsx") covered by the surface's allowed list? */
export function moduleAllowed(src, surface, allowList, otherSurfaces = []) {
  if (/\.html$/.test(src)) return true;
  if (src.startsWith('node_modules/') || src.startsWith('../../node_modules/')) return true;
  if (src.startsWith('vite/') || src.startsWith('\0')) return true;
  if (src.startsWith(`src/public/${surface}/`)) return true;
  // Shared files of the public tree (stubs, helpers) are public by directory; another surface's folder is not.
  const publicDir = /^src\/public\/([^/]+)\//.exec(src)?.[1];
  if (publicDir && !otherSurfaces.includes(publicDir)) return true;
  const bare = src.replace(/\.(tsx?|jsx?|css|mjs)$/, '');
  return allowList.some((a) => {
    const entry = a.replace(/^@\//, 'src/').replace(/^\.\//, '').replace(/\/$/, '');
    const candidates = new Set([entry, `src/${entry}`]);
    const noExt = [...candidates].map((c) => c.replace(/\.(tsx?|jsx?|css|mjs)$/, ''));
    return noExt.some((c) => bare === c || bare.startsWith(`${c}/`)) || src.startsWith(`${entry}/`);
  });
}

function findManifest(dist) {
  return [
    join(dist, 'entrance', '.vite', 'manifest.json'),
    join(dist, '.vite', 'manifest.json'),
  ].find(existsSync);
}

// ---------------------------------------------------------------------------------------------------------------
// The check

const kb = (n) => (n / 1024).toFixed(1);

/**
 * Runs every check. Returns { ok, lines, failures } and writes nothing; the CLI prints.
 * `options.repoRoot` and the allow/denylist paths are overridable for tests.
 */
export function runCheck(distArg, options = {}) {
  const dist = resolve(distArg);
  const repoRoot = options.repoRoot ?? repo;
  const handFile = options.denylistFile ?? join(here, 'entrance-denylist.json');
  const allowFile = options.allowFile ?? join(here, 'entrance-allow.json');
  const lines = [];
  const failures = [];
  const fail = (surface, file, token, rule) => failures.push({ surface, file, token, rule });

  if (!existsSync(join(dist, 'surfaces.json'))) {
    return {
      ok: false,
      lines,
      failures: [
        {
          surface: '-',
          file: 'dist/surfaces.json',
          token: '',
          rule: 'no build (run node scripts/build-web.mjs)',
        },
      ],
    };
  }
  const table = readJson(join(dist, 'surfaces.json'));
  const surfaces = table.surfaces ?? [];
  const gz = new Map();
  const gzipSize = (f) => {
    if (!gz.has(f)) gz.set(f, gzipSync(readFileSync(join(dist, f))).length);
    return gz.get(f);
  };

  const allow = loadAllow(allowFile);
  const implicit = PUBLIC_API_EXACT.map((e) => ({ surface: '*', ...e }));
  allow.entries = [...allow.entries, ...implicit];
  for (const p of allow.problems) fail('-', basename(allowFile), '', p);
  const usedAllow = new Set();

  const { tokens, notes } = buildDenylist({ dist, repoRoot, surfaces, handFile });
  for (const n of notes) fail('-', 'denylist', '', n);
  lines.push(
    `     denylist  ${tokens.length} tokens (` +
      ['chunkref', 'chunk', 'route', 'api', 'i18n', 'word', 'structural']
        .map((k) => `${tokens.filter((t) => t.kind === k).length} ${k}`)
        .join(', ') +
      ')',
  );

  /** Scans one file's text for the denylist; records failures for non-allowed hits. */
  const scan = (surface, file, text) => {
    const lower = text.toLowerCase();
    const here = scoped(allow.entries, surface, file);
    for (const tok of tokens) {
      let i = findToken(text, lower, tok);
      while (i !== null) {
        const hit = here.length ? excusedBy(here, text, lower, i, tok) : null;
        if (!hit) {
          fail(
            surface,
            file,
            tok.token,
            `denylist (${tok.kind}: ${tok.source}) near "${snippet(text, i, tok.token.length)}"`,
          );
          break;
        }
        usedAllow.add(hit);
        i = findToken(text, lower, tok, i + 1);
      }
    }
  };

  // The app shell's own references, for the cross-reference rule.
  const appRefs = new Set();
  if (existsSync(join(dist, table.appShell ?? 'app.html'))) {
    const appHtml = readFileSync(join(dist, table.appShell ?? 'app.html'), 'utf8');
    for (const r of shellRefs(appHtml)) appRefs.add(distPath(r.url));
  }
  const appBaseNames = new Set([...appRefs].map((r) => basename(r)));

  // "A public shell in dist that is not in the table fails."
  const declaredShells = new Set([
    table.appShell ?? 'app.html',
    ...surfaces.flatMap((s) => [s.shell, ...(s.files ?? [])]).filter(Boolean),
  ]);
  for (const f of readdirSync(dist).filter((x) => x.endsWith('.html'))) {
    if (!declaredShells.has(f)) {
      fail(
        '-',
        f,
        '',
        'an HTML document at the top of dist that is not in public-surfaces.json: a new public page must join the table (and therefore this check)',
      );
    }
  }

  // The public build's manifest, for the allowed-module rule.
  const manifestFile = findManifest(dist);
  const manifest = manifestFile ? readJson(manifestFile) : null;
  const srcByFile = new Map();
  if (manifest) {
    for (const [key, v] of Object.entries(manifest)) {
      if (v?.file) srcByFile.set(v.file, v.src ?? (key.startsWith('_') ? null : key));
    }
  }
  const manifestBase =
    manifestFile && manifestFile.includes(join('entrance', '.vite')) ? 'entrance/' : '';
  const srcFor = (distFile) => {
    const f = distFile.startsWith(manifestBase) ? distFile.slice(manifestBase.length) : distFile;
    return srcByFile.get(f) ?? srcByFile.get(distFile) ?? null;
  };

  const sourceFilesSeen = new Set();
  const bundleNames = surfaces.filter((x) => x.kind === 'bundle').map((x) => x.surface);

  for (const s of surfaces) {
    const name = s.surface;
    const before = failures.length;
    const row = [];

    if (s.kind === 'bundle') {
      if (!s.shell || !existsSync(join(dist, s.shell))) {
        if (s.reserved) {
          lines.push(`skip ${name.padEnd(9)} reserved (wave ${s.wave ?? '?'}), no shell yet`);
          continue;
        }
        fail(
          name,
          s.shell ?? '(none)',
          '',
          'the surface is in public-surfaces.json but its shell is not in dist',
        );
        lines.push(`FAIL ${name.padEnd(9)} no shell`);
        continue;
      }
      const shell = readFileSync(join(dist, s.shell), 'utf8');
      const refs = shellRefs(shell);
      const roots = refs
        .filter((r) => r.tag === 'script' || r.rel === 'modulepreload')
        .map((r) => distPath(r.url))
        .filter((u) => /\.m?js$/.test(u));
      if (!refs.some((r) => r.tag === 'script'))
        fail(name, s.shell, '', 'the shell loads no script (a bundle surface needs its entry)');
      const files = closure(dist, roots);
      for (const r of roots) {
        if (!existsSync(join(dist, r)))
          fail(name, s.shell, r, 'the shell references a file that is not in dist');
      }

      // budget (gzip JavaScript of the static closure)
      const size = [...files].reduce((n, f) => n + gzipSize(f), 0);
      const budget = (s.budgetGzipKB ?? 0) * 1024;
      if (size > budget) {
        fail(
          name,
          s.shell,
          '',
          `first load is ${kb(size)} KB gzip, over the ${s.budgetGzipKB} KB budget`,
        );
      }
      row.push(`${kb(size)}/${s.budgetGzipKB} KB gzip, ${files.size} files`);

      // reach = static closure + dynamic imports that stay inside the public build
      const reach = new Set(files);
      const queue = [...files];
      while (queue.length) {
        const f = queue.pop();
        const code = readFileSync(join(dist, f), 'utf8');
        const dyn = dynamicImports(f, code);
        for (const t of dyn.targets) {
          if (!t.startsWith('entrance/') && !t.startsWith('fonts/')) {
            fail(
              name,
              f,
              t,
              'rule: import() outside /entrance/ (a public file must not fetch anything else)',
            );
            continue;
          }
          if (!reach.has(t) && existsSync(join(dist, t)) && /\.m?js$/.test(t)) {
            reach.add(t);
            for (const x of closure(dist, [t])) {
              if (!reach.has(x)) {
                reach.add(x);
                queue.push(x);
              }
            }
            queue.push(t);
          }
        }
      }

      // preload / stylesheet / script references
      for (const r of refs) {
        const u = distPath(r.url);
        const okPrefix = u.startsWith('entrance/') || u.startsWith('fonts/');
        const isPreload = r.rel === 'modulepreload' || r.rel === 'preload' || r.rel === 'prefetch';
        if (isPreload && !okPrefix)
          fail(name, s.shell, r.url, 'rule: preload outside /entrance/ and /fonts/');
        if ((r.tag === 'script' || r.rel === 'stylesheet') && /^(?:app|assets)\//.test(u)) {
          fail(name, s.shell, r.url, 'rule: the shell references a file of the app build');
        }
      }

      // allowed modules, from the public build's manifest
      const allowedTs = join(repoRoot, 'apps', 'web', 'src', 'public', name, 'ALLOWED.ts');
      if (!existsSync(allowedTs)) {
        fail(name, `src/public/${name}/ALLOWED.ts`, '', 'every bundle surface needs an ALLOWED.ts');
      } else if (!manifest) {
        fail(
          name,
          'dist/entrance/.vite/manifest.json',
          '',
          'no Vite manifest from the public build: cannot check the allowed modules',
        );
      } else {
        const list = readAllowedModules(allowedTs);
        for (const f of reach) {
          const src = srcFor(f);
          if (
            src &&
            !moduleAllowed(
              src,
              name,
              list,
              bundleNames.filter((n) => n !== name),
            )
          ) {
            fail(name, f, src, `rule: module outside src/public/${name}/ALLOWED.ts`);
          }
        }
      }

      // cross-reference with the app shell
      for (const f of reach) {
        if (appRefs.has(f) || appBaseNames.has(basename(f))) {
          fail(name, f, '', 'rule: the file is also referenced from app.html');
        }
      }

      // content: shell + every file in reach
      scan(name, s.shell, shell);
      for (const f of reach) {
        const code = readFileSync(join(dist, f), 'utf8');
        if (/sourceMappingURL\s*=/.test(code))
          fail(name, f, '', 'rule: sourceMappingURL in a public file');
        sourceFilesSeen.add(f);
        scan(name, f, code);
      }
      for (const r of refs.filter((x) => x.tag === 'link' && x.rel === 'stylesheet')) {
        const u = distPath(r.url);
        if (existsSync(join(dist, u)) && !reach.has(u)) {
          const css = readFileSync(join(dist, u), 'utf8');
          if (/sourceMappingURL\s*=/.test(css))
            fail(name, u, '', 'rule: sourceMappingURL in a public file');
          scan(name, u, css);
        }
      }
    } else if (s.kind === 'html') {
      // plain HTML: legal pages and the error pages
      const docs = [
        ...(s.files ?? []).map((f) => join(dist, f)),
        ...(name === 'legal' ? walk(join(dist, 'legal'), (p) => p.endsWith('.html')) : []),
      ].filter(existsSync);
      for (const f of (s.files ?? []).filter((x) => !existsSync(join(dist, x)))) {
        fail(name, f, '', 'the surface lists this file but it is not in dist');
      }
      if (name === 'legal' && docs.length === 0) {
        fail(
          name,
          'dist/legal/*.html',
          '',
          'no house-notes template was rendered (scripts/render-legal.mjs)',
        );
      }
      for (const f of docs) {
        const text = readFileSync(f, 'utf8');
        const r = rel(dist, f);
        if (/<script\b/i.test(text))
          fail(name, r, '<script', 'rule: a plain-HTML surface has no script');
        if (/<[a-z][^>]*\son[a-z]+\s*=/i.test(text))
          fail(name, r, 'on*=', 'rule: a plain-HTML surface has no event-handler attribute');
        if (/\bfetch\s*\(/.test(text))
          fail(name, r, 'fetch', 'rule: a plain-HTML surface fetches nothing');
        if (/rel\s*=\s*["'](?:modulepreload|preload)["']/i.test(text))
          fail(name, r, 'preload', 'rule: a plain-HTML surface preloads nothing');
        scan(name, r, text);
      }
      row.push(`${docs.length} document${docs.length === 1 ? '' : 's'}, no script`);
    }

    const bad = failures.length - before;
    lines.push(
      `${bad ? 'FAIL' : 'ok  '} ${name.padEnd(9)} ${bad ? `${bad} problem${bad === 1 ? '' : 's'}` : row.join('; ')}`,
    );
  }

  // The public static files: the worker, manifests, stylesheets, robots, notices, licences.
  {
    const before = failures.length;
    const files = [
      ...(table.publicFiles ?? []).filter((p) => !p.includes('*')).map((p) => p.replace(/^\//, '')),
      'fonts/README.md',
      'app-manifest.webmanifest',
    ];
    let count = 0;
    for (const f of new Set(files)) {
      const p = join(dist, f);
      if (!existsSync(p) || !statSync(p).isFile()) continue;
      if (f === 'app-manifest.webmanifest') {
        fail(
          'static',
          f,
          '',
          'rule: the app manifest is served from /app/ only, never as a public file',
        );
        continue;
      }
      if (/\.(?:html)$/.test(f)) continue; // covered as plain-HTML surfaces
      scan('static', f, readFileSync(p, 'utf8'));
      count += 1;
    }
    const maps = walk(dist, (p) => /\.map$/.test(p) && !toPosix(p).includes('/app/'));
    for (const m of maps)
      fail('static', rel(dist, m), '', 'rule: a source map in the public build');
    // The public build directory must not hold files for the app.
    for (const f of walk(join(dist, 'entrance'), (p) => /\.wasm$/.test(p))) {
      fail('static', rel(dist, f), '', 'rule: a .wasm file in the public build');
    }
    const bad = failures.length - before;
    lines.push(
      `${bad ? 'FAIL' : 'ok  '} ${'static'.padEnd(9)} ${bad ? `${bad} problem${bad === 1 ? '' : 's'}` : `${count} files scanned (sw.js, manifests, stylesheets, notices)`}`,
    );
  }

  // stale allow entries are reported, not failed
  for (const e of allow.entries) {
    if (!usedAllow.has(e) && e.reason && !implicit.includes(e))
      lines.push(`note stale allow entry: ${e.surface} ${e.token} (matched nothing)`);
  }

  return { ok: failures.length === 0, lines, failures };
}

function main() {
  const args = process.argv.slice(2);
  const quiet = args.includes('--quiet');
  const dist = resolve(args.find((a) => !a.startsWith('--')) ?? join(repo, 'apps', 'web', 'dist'));
  if (!existsSync(dist)) {
    console.error(`No build at ${dist}. Run: node scripts/build-web.mjs`);
    process.exit(2);
  }
  const repoArg = args.find((a) => a.startsWith('--repo='))?.slice(7);
  const { ok, lines, failures } = runCheck(dist, repoArg ? { repoRoot: resolve(repoArg) } : {});
  if (!quiet || !ok) for (const l of lines) console.log(l);
  const shown = failures.slice(0, 60);
  for (const f of shown) {
    console.error(
      `  surface ${f.surface} | ${f.file}${f.token ? ` | ${f.token}` : ''} | ${f.rule}`,
    );
  }
  if (failures.length > shown.length)
    console.error(`  ... and ${failures.length - shown.length} more`);
  if (!ok) {
    console.error(
      `\nEntrance isolation: ${failures.length} problem(s). A public page may carry only its own code (D-385).`,
    );
    console.error(
      'A tolerated match goes in scripts/entrance-allow.json with a reason; the three section names never do (O-7).',
    );
    process.exit(1);
  }
  console.log('Entrance isolation: every public surface is clean.');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
