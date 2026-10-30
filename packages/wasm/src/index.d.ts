/** Typed facade over the wasm-pack output in `dist/`. Call `initWasm()` (or any `load*`) once before use. */

export interface ThemeIssue {
  severity: 'error' | 'warning';
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
  kind: 'role' | 'member';
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

/** Why an archive pre-check refused a file; `files` names up to ten offending entries. */
export interface ArchiveRefusal {
  kind:
    | 'too_large'
    | 'too_many_entries'
    | 'entry_too_large'
    | 'suspicious_ratio'
    | 'unsafe_path'
    | 'encrypted'
    | 'avif'
    | 'no_pages'
    | 'not_an_archive'
    | 'unreadable';
  message: string;
  files: string[];
}

/** Where a page sits in a ZIP, so the browser can inflate and decode it. */
export interface ZipProbe {
  name: string;
  local_offset: number;
  compressed: number;
  size: number;
  /** 0 stored, 8 deflate. */
  method: number;
}

export type ArchivePrecheck =
  /** The directory starts before the bytes given: read this many bytes from the end and call again. */
  | { need_tail: number }
  | { ok: false; container?: string; error: ArchiveRefusal }
  /** 7z and RAR cannot be listed from a tail; only the container and size were checked. */
  | { ok: true; container: '7z' | 'rar'; listed: false }
  | {
      ok: true;
      container: 'zip';
      listed: true;
      pages: number;
      total_bytes: number;
      largest_bytes: number;
      first_names: string[];
      comic_info: boolean;
      series_json: boolean;
      ignored: number;
      first: ZipProbe;
      last: ZipProbe;
    };

/** Browser pre-checks for Library uploads (LIB-ING-009), judged by the same rules as the server. */
export interface LibraryApi {
  /**
   * Check an archive from its first bytes, its last `firstTail` bytes (or the `need_tail` the previous
   * call asked for) and its size. `maxArchiveBytes` is the operator's upload limit (0 = the default).
   */
  precheck(
    head: Uint8Array,
    tail: Uint8Array,
    fileLen: number,
    maxArchiveBytes?: number,
  ): ArchivePrecheck;
  /** Bytes from the end of the file to read first. */
  firstTail: number;
  /**
   * Every page of a CBZ in reading order, from the last bytes of the file (a CBZ in a bucket is read by range
   * requests, BYOS-CAT-002). `need_tail`: read that many bytes from the end and call again.
   */
  listPages(
    tail: Uint8Array,
    fileLen: number,
  ): { need_tail: number } | { ok: true; pages: ZipProbe[] } | { ok: false; message: string };
  defaults: { maxArchive: number; maxEntries: number; maxEntry: number; maxTotal: number };
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

/** Loaded on its own: only the Library upload forms need it. */
export function loadLibrary(wasmUrl?: string | URL | BufferSource): Promise<LibraryApi>;

/** One sender's frame encryptor for one epoch (sr-e2ee). Call `free()` when the epoch ends. */
export interface E2eeEncryptor {
  /** Encrypt an encoded frame of `mime`; throws when the frame must be dropped. */
  encrypt(mime: string, frame: Uint8Array): Uint8Array;
  readonly counter: number;
  free(): void;
}

/** The receiver's epoch keys, with a grace window for the previous epoch. */
export interface E2eeKeyRing {
  /** `secrets` holds one 16-byte secret per user id, in order. */
  setEpoch(epoch: number, userIds: string[], secrets: Uint8Array, nowMs: number): void;
  /** Decrypt a frame from `sender`; throws when the frame must be dropped. */
  decrypt(sender: string, mime: string, frame: Uint8Array, nowMs: number): Uint8Array;
  /** -1 before the first epoch. */
  readonly epoch: number;
  clear(): void;
  free(): void;
}

export interface E2eePendingSession {
  /** What the identity key signs (ECDSA P-256, SHA-256). */
  certMessage(): Uint8Array;
  /** The signature as WebCrypto returns it (64 bytes r||s). */
  certify(signature: Uint8Array): E2eeCallClient;
  free(): void;
}

/** One member's MLS state for one call (VOX-E2E-001). MLS messages are base64 strings. */
export interface E2eeCallClient {
  keyPackage(): string;
  createGroup(groupId: string, externalKey?: string | null, externalIdentity?: string | null): void;
  readonly hasGroup: boolean;
  /** -1 without a group. */
  readonly epoch: number;
  ownLeaf(): number;
  epochAuthenticator(): Uint8Array;
  /** JSON: `[{leaf, user_id, identity_key (hex)}]`. */
  members(): string;
  senderUserIds(): string[];
  /** 16 bytes per user of `senderUserIds()`, concatenated. */
  senderSecrets(): Uint8Array;
  /** JSON `{based_on, commit, welcome, welcomed}` or `"null"`. */
  commit(keyPackages: string[], roster: string[]): string;
  commitAccepted(): number;
  commitRejected(): void;
  /** JSON `{type: "commit", epoch} | {type: "proposal"} | {type: "ignored"}`. */
  handle(message: string, roster: string[]): string;
  join(welcome: string, roster: string[]): number;
  leave(): void;
  free(): void;
}

export interface E2eeApi {
  isSupportedCodec(mime: string): boolean;
  /** `secret`: 16 bytes; `random`: 4 bytes from `crypto.getRandomValues`. */
  createEncryptor(secret: Uint8Array, random: Uint8Array): E2eeEncryptor;
  createKeyRing(graceMs?: number): E2eeKeyRing;
  /** Step 1 of an MLS session: `identityKey` is the 65-byte raw P-256 identity public key. */
  createPendingSession(userId: string, identityKey: Uint8Array): E2eePendingSession;
  /** D-250: the stable verification code between two users (six groups of five digits). */
  pairwiseCode(userA: string, keyA: Uint8Array, userB: string, keyB: Uint8Array): string;
  /** D-250: the code of the current call epoch. */
  callCode(epochAuthenticator: Uint8Array): string;
}

export function loadE2ee(wasmUrl?: string | URL | BufferSource): Promise<E2eeApi>;

/** Load all modules in parallel. */
export function initWasm(options?: WasmInitOptions): Promise<{
  theme: ThemeApi;
  markdown: MarkdownApi;
  permissions: PermissionsApi;
}>;

// MUS-WASM-001: audio tags, cover art and lyrics (sr-music-wasm). The internals are a hand-written
// ID3v2/ID3v1 + FLAC (+ OGG, MP4 ilst) reader that `lofty` can replace later behind this same shape.
// Feed `readTags` the first 1 MiB of the file (and the last 128 bytes for ID3v1 via `readTagsTail`);
// never the whole file.

/** `cover` holds the picture; absent when the file has none. */
export interface MusicCover {
  mime: string;
  bytes: Uint8Array;
}

export interface MusicTags {
  title: string | null;
  artist: string | null;
  album: string | null;
  albumArtist: string | null;
  /** 1-based track number (the "/total" part of "3/12" is dropped). */
  track: number | null;
  disc: number | null;
  year: number | null;
  genre: string | null;
  /** Duration in milliseconds when the tags or stream header give one (TLEN, FLAC STREAMINFO), else null. */
  durationHint: number | null;
  cover: MusicCover | null;
}

export interface LrcLine {
  /** Start of the line in milliseconds. */
  ms: number;
  line: string;
}

export interface MusicApi {
  /**
   * Read tags from the head of an audio file (MP3/ID3v2, FLAC, OGG, M4A). `head` is the first bytes
   * (1 MiB is enough). Throws on unsupported, truncated or garbage input; never returns partial garbage.
   */
  readTags(head: Uint8Array): MusicTags;
  /** Read an ID3v1 tag from the last 128 bytes of an MP3 (`tail`). Same errors as `readTags`. */
  readTagsTail(tail: Uint8Array): MusicTags;
  /** Parse LRC text (`[mm:ss.xx]line`, multiple stamps per line, offset tag) into lines sorted by time. */
  parseLrc(text: string): LrcLine[];
  /** Candidate sidecar keys for a track key, best first: `a/b.mp3` -> `["a/b.lrc", "a/b.mp3.lrc"]`. */
  lrcSidecarNames(trackKey: string): string[];
}

export function loadMusic(wasmUrl?: string | URL | BufferSource): Promise<MusicApi>;
