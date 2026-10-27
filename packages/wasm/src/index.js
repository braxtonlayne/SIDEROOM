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
