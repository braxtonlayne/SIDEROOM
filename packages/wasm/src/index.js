// Typed facade (see index.d.ts) over the wasm-pack `--target web` output in ../dist/<name>/.
// Modules load lazily and once; the raw bindings return JSON strings, parsed here.

const once = (fn) => {
  let p;
  return (...args) => (p ??= fn(...args));
};

export const loadTheme = once(async (wasmUrl) => {
  const m = await import('../dist/theme/theme.js');
  await m.default({ module_or_path: wasmUrl });
  return {
    validate: (json) => JSON.parse(m.validateTheme(json)),
    compile: (json) => m.compileTheme(json),
    schema: () => m.themeSchema(),
    tokenNames: () => m.tokenNames(),
  };
});

export const loadMarkdown = once(async (wasmUrl) => {
  const m = await import('../dist/markdown/markdown.js');
  await m.default({ module_or_path: wasmUrl });
  const maxChars = m.defaultMaxChars();
  const maxMentions = m.defaultMaxMentions();
  return {
    parse: (input) => JSON.parse(m.parseMarkdown(input)),
    toPlainText: (input) => m.toPlainText(input),
    isJumbo: (input) => m.isJumbo(input),
    expandShortcodes: (input) => m.expandShortcodes(input),
    extractMentions: (content) => JSON.parse(m.extractMentions(content)),
    checkLimits: (content, chars = maxChars, mentions = maxMentions) =>
      JSON.parse(m.checkLimits(content, chars, mentions)),
    defaultMaxChars: maxChars,
    defaultMaxMentions: maxMentions,
  };
});

export const loadPermissions = once(async (wasmUrl) => {
  const m = await import('../dist/permissions/permissions.js');
  await m.default({ module_or_path: wasmUrl });
  return {
    computeBase: (isOwner, everyone, roles) => m.computeBase(isOwner, everyone, roles),
    computeOverwrites: (base, guildId, memberId, memberRoleIds, overwrites) =>
      m.computeOverwrites(base, guildId, memberId, memberRoleIds, JSON.stringify(overwrites)),
    has: (have, want) => m.hasPermission(have, want),
    compute: (input) => m.compute(JSON.stringify(input)),
  };
});

export const loadGraph = once(async (wasmUrl) => {
  const m = await import('../dist/graph/graph.js');
  await m.default({ module_or_path: wasmUrl });
  const [iterations, seed, , springLength] = m.layoutDefaults();
  return {
    layout: (nodeCount, edges, options = {}) =>
      m.layoutGraph(
        nodeCount,
        edges instanceof Uint32Array ? edges : Uint32Array.from(edges),
        options.iterations ?? 0,
        options.seed ?? 0,
        options.springLength ?? 0,
      ),
    defaults: { iterations, seed, springLength },
  };
});

export const loadLibrary = once(async (wasmUrl) => {
  const m = await import('../dist/library/library.js');
  await m.default({ module_or_path: wasmUrl });
  const [maxArchive, maxEntries, maxEntry, maxTotal] = m.limitDefaults();
  return {
    precheck: (head, tail, fileLen, maxArchiveBytes = 0) =>
      JSON.parse(m.precheckArchive(head, tail, fileLen, maxArchiveBytes)),
    firstTail: m.firstTail(),
    listPages: (tail, fileLen) => JSON.parse(m.listPages(tail, fileLen)),
    defaults: { maxArchive, maxEntries, maxEntry, maxTotal },
  };
});

// VOX-E2E-001/002/006/007: call E2EE (sr-e2ee): the MLS client (main thread) and the media
// transform (media worker).
// Secrets go in as bytes and stay in WASM memory; errors mean "drop this frame".
export const loadE2ee = once(async (wasmUrl) => {
  const m = await import('../dist/e2ee/e2ee.js');
  await m.default({ module_or_path: wasmUrl });
  return {
    isSupportedCodec: (mime) => m.isSupportedCodec(mime),
    createEncryptor: (secret, random) => new m.Encryptor(secret, random),
    createKeyRing: (graceMs = 10000) => new m.KeyRing(graceMs),
    // ACT-CH-001: activity data messages (AES-GCM under the sender key, replay window on receive).
    createDataSender: (secret, random) => new m.DataSender(secret, random),
    createDataReceiver: (graceMs = 10000) => new m.DataReceiver(graceMs),
    createPendingSession: (userId, identityKey) => new m.PendingSession(userId, identityKey),
    pairwiseCode: (userA, keyA, userB, keyB) => m.pairwiseCode(userA, keyA, userB, keyB),
    callCode: (epochAuthenticator) => m.callCode(epochAuthenticator),
  };
});

export async function initWasm(options = {}) {
  const [theme, markdown, permissions] = await Promise.all([
    loadTheme(options.theme),
    loadMarkdown(options.markdown),
    loadPermissions(options.permissions),
  ]);
  return { theme, markdown, permissions };
}

// MUS-WASM-001: audio tags, cover art and lyrics (sr-music-wasm). The raw bindings return JSON
// (without the picture) plus a separate cover read, so the picture bytes are never JSON-encoded.
// A Rust `NEED_MOOV:<offset>` error becomes an Error with `code` and `offset` (see MusicNeedMoovError).
const guard = (fn) => {
  try {
    return fn();
  } catch (e) {
    const hit = /NEED_MOOV:(\d+)/.exec(String(e?.message ?? e));
    if (hit)
      throw Object.assign(new Error(`NEED_MOOV:${hit[1]}`), {
        code: 'NEED_MOOV',
        offset: Number(hit[1]),
      });
    throw e;
  }
};

export const loadMusic = once(async (wasmUrl) => {
  const m = await import('../dist/music/music.js');
  await m.default({ module_or_path: wasmUrl });
  const build = (json, coverBytes) => {
    const t = JSON.parse(json);
    const { coverMime, ...rest } = t;
    return { ...rest, cover: coverMime ? { mime: coverMime, bytes: coverBytes() } : null };
  };
  return {
    readTags: (head) =>
      build(
        guard(() => m.readTags(head)),
        () => m.readCover(head),
      ),
    readTagsTail: (tail) =>
      build(
        guard(() => m.readTagsTail(tail)),
        () => new Uint8Array(0),
      ),
    parseLrc: (text) => JSON.parse(m.parseLrc(text)),
    lrcSidecarNames: (trackKey) => m.lrcSidecarNames(trackKey),
  };
});

// UTL-SRC-001: the utility drawer's logic (sr-tools-wasm). One module per tool group; each owner adds
// its own key to the object loadTools returns (search is utl-search's); raw is the wasm-bindgen module.
const toolsSearch = (m) => ({
  /** A search index over the embedded tool corpus; the client sets the channels and people. */
  create() {
    const s = new m.ToolSearch();
    return {
      setEntities: (entities) => s.setEntities(JSON.stringify(entities)),
      search: (query, limit = 20) => JSON.parse(s.search(String(query), limit)),
      free: () => s.free(),
    };
  },
});

// UTL-IMG-001 (utl-image): the GIF encoder and the palette extraction.
const toolsGif = (m) => {
  const [maxDurationMs, maxBytes, maxEdge, minDelayCs] = m.gifLimits();
  return {
    limits: { maxDurationMs, maxBytes, maxEdge, minDelayCs },
    /** A builder: addFrame(rgba) per frame, then finish() gives the GIF bytes. Errors carry a GIF_* code. */
    create: (width, height, delayCs) => {
      const b = new m.GifBuilder(width, height, delayCs);
      return {
        addFrame: (rgba) => b.addFrame(rgba),
        finish: () => b.finish(),
        get frameCount() {
          return b.frameCount;
        },
        get maxFrames() {
          return b.maxFrames;
        },
        free: () => b.free(),
      };
    },
  };
};
const toolsPalette = (m) => ({
  extract: (rgba, k = 6) => JSON.parse(m.extractPalette(rgba, k)),
});

// UTL-DEV-001 (utl-dev): dice, diff and hashes. Results are JSON text (dice, diff) or hex (hashes).
const toolsDev = (m) => ({
  diceRoll: (expression, seedHi, seedLo) => m.diceRoll(expression, seedHi, seedLo),
  diceFlip: (seedHi, seedLo) => m.diceFlip(seedHi, seedLo),
  dicePickWeighted: (weights, seedHi, seedLo) => m.dicePickWeighted(weights, seedHi, seedLo),
  diffText: (a, b, mode) => m.diffText(a, b, mode),
  hashMd5: (data) => m.hashMd5(data),
  hashCrc32: (data) => m.hashCrc32(data),
});

// UTL-CALC-001 (utl-calc) uses `raw` through its own typed wrapper (apps/web/src/tools/calc/engine.ts).
export const loadTools = once(async (wasmUrl) => {
  const m = await import('../dist/tools/tools.js');
  await m.default({ module_or_path: wasmUrl });
  return {
    raw: m,
    search: toolsSearch(m),
    gif: toolsGif(m),
    palette: toolsPalette(m),
    dev: toolsDev(m),
  };
});
