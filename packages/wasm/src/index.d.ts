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
  /** Full channel permissions: overwrites plus the timeout and implicit rules (browser clock). */
  compute(input: ComputeChannelInput): string;
}

/** Input of `PermissionsApi.compute` (mirrors `ComputeIn` in crates/sr-permissions-wasm). */
export interface ComputeChannelInput {
  is_owner: boolean;
  /** The @everyone role's permissions. */
  everyone: string;
  /** Permissions of the member's roles (without @everyone). */
  role_permissions: string[];
  guild_id: string;
  member_id: string;
  member_role_ids: string[];
  overwrites: PermissionOverwrite[];
  /** Unix milliseconds the member's timeout ends, or null. */
  communication_disabled_until_ms: number | null;
}

/** Options of `GraphApi.layout`; leave a field out for its default. */
export interface GraphLayoutOptions {
  /** Steps to run (default 300). */
  iterations?: number;
  /** Same graph and seed give the same picture (default 0, which is the same as 1). */
  seed?: number;
  /** Ideal edge length (default 40). */
  springLength?: number;
}

/** Force-directed layout with Barnes-Hut repulsion (ADM-GRAPH-002). */
export interface GraphApi {
  /**
   * Positions for `nodeCount` nodes joined by `edges`, a flat list of node index pairs
   * `[a0, b0, a1, b1, ...]`. Returns `[x0, y0, x1, y1, ...]` centred on the origin. Throws for an
   * edge that points at a node that does not exist.
   */
  layout(
    nodeCount: number,
    edges: ArrayLike<number> | Uint32Array,
    options?: GraphLayoutOptions,
  ): Float32Array;
  defaults: { iterations: number; seed: number; springLength: number };
}

/** Optional overrides of where the `.wasm` binaries are loaded from (defaults to `dist/<name>/`). */
export interface WasmInitOptions {
  theme?: string | URL;
  markdown?: string | URL;
  permissions?: string | URL;
}

export function loadTheme(wasmUrl?: string | URL): Promise<ThemeApi>;
export function loadMarkdown(wasmUrl?: string | URL): Promise<MarkdownApi>;
/** `wasmUrl` may also be the module bytes (tests read the `.wasm` from disk). */
export function loadPermissions(wasmUrl?: string | URL | BufferSource): Promise<PermissionsApi>;
/** Loaded on its own (not part of `initWasm`): only the Invite Graph needs it. */
export function loadGraph(wasmUrl?: string | URL | BufferSource): Promise<GraphApi>;

/** Load all modules in parallel. */
export function initWasm(options?: WasmInitOptions): Promise<{
  theme: ThemeApi;
  markdown: MarkdownApi;
  permissions: PermissionsApi;
}>;
