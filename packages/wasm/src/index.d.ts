/** Typed facade over the wasm-pack output in `dist/`. Call `initWasm()` (or any `load*`) once before use. */

export interface ThemeIssue {
  severity: "error" | "warning";
  /** Stable machine code, e.g. `unknown-token`, `low-contrast`. */
  code: string;
  /** `manifest.id`, `tokens.--ui-accent`, `assets[0].path`, ... */
  path: string;
  message: string;
}

export interface ThemeReport {
  valid: boolean;
  issues: ThemeIssue[];
}

/** Generated JSON Schema lives in `theme.schema.json` (from `crates/sr-theme`). */
export interface ThemeApi {
  /** Validate a theme package (JSON text). Never throws; bad JSON yields an `invalid-json` issue. */
  validate(packageJson: string): ThemeReport;
  /** Compile to a `[data-theme="id"] { ... }` rule. Throws on an invalid package. */
  compile(packageJson: string): string;
  /** JSON Schema text for theme packages. */
  schema(): string;
  /** Every token a theme may set. */
  tokenNames(): string[];
}

/** Snowflake-like ids are decimal strings. */
export interface Mentions {
  users: string[];
  roles: string[];
  channels: string[];
  emoji: string[];
  everyone: boolean;
  here: boolean;
}

/** AST node as produced by sr-markdown (`type` is the snake_case variant name). */
export interface MarkdownNode {
  type: string;
  [field: string]: unknown;
}

export interface MarkdownApi {
  parse(input: string): MarkdownNode[];
  toPlainText(input: string): string;
  /** Same rule as the server: 1 to 27 emoji and nothing else. */
  isJumbo(input: string): boolean;
  /** Replace `:shortcode:` names with unicode emoji (composer send-time step). */
  expandShortcodes(input: string): string;
  extractMentions(content: string): Mentions;
  /** Returns the mentions, or throws if over `maxChars` / `maxMentions`. */
  checkLimits(content: string, maxChars?: number, maxMentions?: number): Mentions;
  readonly defaultMaxChars: number;
  readonly defaultMaxMentions: number;
}

export interface PermissionOverwrite {
  id: string;
  kind: "role" | "member";
  /** Permission bitfield as a decimal string. */
  allow: string;
  deny: string;
}

/** Permission bitfields and ids are decimal strings (bits 56+ exceed `Number.MAX_SAFE_INTEGER`). */
export interface PermissionsApi {
  computeBase(isOwner: boolean, everyone: string, rolePermissions: string[]): string;
  computeOverwrites(
    base: string,
    guildId: string,
    memberId: string,
    memberRoleIds: string[],
    overwrites: PermissionOverwrite[],
  ): string;
  /** Administrators bypass. */
  has(have: string, want: string): boolean;
}

/** Optional overrides of where the `.wasm` binaries are loaded from (defaults to `dist/<name>/`). */
export interface WasmInitOptions {
  theme?: string | URL;
  markdown?: string | URL;
  permissions?: string | URL;
}

export function loadTheme(wasmUrl?: string | URL): Promise<ThemeApi>;
export function loadMarkdown(wasmUrl?: string | URL): Promise<MarkdownApi>;
export function loadPermissions(wasmUrl?: string | URL): Promise<PermissionsApi>;

/** Load all modules in parallel. */
export function initWasm(options?: WasmInitOptions): Promise<{
  theme: ThemeApi;
  markdown: MarkdownApi;
  permissions: PermissionsApi;
}>;
