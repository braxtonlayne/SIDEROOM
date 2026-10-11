// Typed facade (see index.d.ts) over the wasm-pack `--target web` output in ../dist/<name>/.
// Modules load lazily and once; the raw bindings return JSON strings, parsed here.

const once = (fn) => {
  let p;
  return (...args) => (p ??= fn(...args));
};

export const loadTheme = once(async (wasmUrl) => {
  const m = await import("../dist/theme/theme.js");
  await m.default({ module_or_path: wasmUrl });
  return {
    validate: (json) => JSON.parse(m.validateTheme(json)),
    compile: (json) => m.compileTheme(json),
    schema: () => m.themeSchema(),
    tokenNames: () => m.tokenNames(),
  };
});

export const loadMarkdown = once(async (wasmUrl) => {
  const m = await import("../dist/markdown/markdown.js");
  await m.default({ module_or_path: wasmUrl });
  const maxChars = m.defaultMaxChars();
  const maxMentions = m.defaultMaxMentions();
  return {
    parse: (input) => JSON.parse(m.parseMarkdown(input)),
    toPlainText: (input) => m.toPlainText(input),
    extractMentions: (content) => JSON.parse(m.extractMentions(content)),
    checkLimits: (content, chars = maxChars, mentions = maxMentions) =>
      JSON.parse(m.checkLimits(content, chars, mentions)),
    defaultMaxChars: maxChars,
    defaultMaxMentions: maxMentions,
  };
});

export const loadPermissions = once(async (wasmUrl) => {
  const m = await import("../dist/permissions/permissions.js");
  await m.default({ module_or_path: wasmUrl });
  return {
    computeBase: (isOwner, everyone, roles) => m.computeBase(isOwner, everyone, roles),
    computeOverwrites: (base, guildId, memberId, memberRoleIds, overwrites) =>
      m.computeOverwrites(base, guildId, memberId, memberRoleIds, JSON.stringify(overwrites)),
    has: (have, want) => m.hasPermission(have, want),
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
