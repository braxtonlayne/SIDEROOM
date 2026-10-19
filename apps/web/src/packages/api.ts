// Data layer of declarative packages (EXT-PKG) and customised profiles (EXT-PRF). Components never build URLs.
import { ApiError, apiFetch, apiPost, getCsrf } from '../api/client';

const id = encodeURIComponent;
const ro = { retries: 0 } as const;

export type PackageKind =
  | 'theme'
  | 'profile_layout'
  | 'emoji_pack'
  | 'sticker_pack'
  | 'sound_pack'
  | 'recipe'
  | 'server_template';

export const PACKAGE_KINDS: readonly PackageKind[] = [
  'theme',
  'profile_layout',
  'emoji_pack',
  'sticker_pack',
  'sound_pack',
  'recipe',
  'server_template',
];

export interface ThemePayload {
  scheme: 'light' | 'dark';
  targets: string[];
  extends: string | null;
  tokens: Record<string, string>;
  css: string;
}

export interface PackageView {
  id: string;
  kind: PackageKind;
  slug: string;
  name: string;
  description: string;
  author_id: string;
  author_name: string;
  version_id: string;
  version: string;
  size_bytes: number;
  updated_at_ms: number;
  payload: Record<string, unknown> | null;
}

export interface VersionView {
  id: string;
  package_id: string;
  kind: PackageKind;
  slug: string;
  name: string;
  version: string;
  review_state: 'pending' | 'approved' | 'rejected' | 'withdrawn' | 'revoked';
  review_note: string | null;
  size_bytes: number;
  created_at_ms: number;
  files: { path: string; size: number }[];
  warnings: string[];
}

export interface MyPackage {
  id: string;
  kind: PackageKind;
  slug: string;
  name: string;
  versions: VersionView[];
}

export interface PendingVersion extends VersionView {
  author_id: string;
  author_name: string;
  payload: Record<string, unknown>;
}

export interface Enablement {
  package_id: string;
  version_id: string;
  kind: PackageKind;
  slug: string;
  name: string;
  version: string;
  section: string;
  config: Record<string, unknown>;
  enabled_at_ms: number;
  update_available: boolean;
  payload: Record<string, unknown> | null;
  /** For a pack turned on in a server: how many items it added and that are still there (D-282). */
  item_count?: number | null;
}

export interface ApplyResult {
  applied: { feature_id: string; enabled: boolean | null; settings: string[] }[];
  enablement: Enablement;
}

export const keys = {
  directory: (kind: PackageKind | '', q: string) => ['packages', 'directory', kind, q] as const,
  mine: ['packages', 'mine'] as const,
  myEnabled: ['packages', 'enabled', 'me'] as const,
  guildEnabled: (g: string) => ['packages', 'enabled', 'guild', g] as const,
  pending: ['packages', 'pending'] as const,
};

type ErrorTree = { _errors?: { message: string }[] } & Record<string, unknown>;

/** Every message in a Discord-style error tree, at any depth, in key order. */
function messagesOf(tree: ErrorTree): string[] {
  const own = (tree._errors ?? []).map((x) => x.message);
  const nested = Object.entries(tree)
    .filter(([k, v]) => k !== '_errors' && v && typeof v === 'object')
    .flatMap(([, v]) => messagesOf(v as ErrorTree));
  return [...own, ...nested];
}

/** Every problem the server listed for a refused package or document (one per line), or the single error text. */
export function problemLines(e: unknown): string[] {
  if (e instanceof ApiError) {
    const lines = messagesOf((e.body?.errors ?? {}) as ErrorTree);
    if (lines.length) return lines;
    if (e.message) return [e.message];
  }
  return [];
}

// ---- browsing ----
export const listPackages = (kind: PackageKind | '', q: string) => {
  const p = new URLSearchParams();
  if (kind) p.set('kind', kind);
  if (q.trim()) p.set('q', q.trim());
  const qs = p.toString();
  return apiFetch<PackageView[]>(`/packages${qs ? `?${qs}` : ''}`, ro);
};
export const getPackage = (packageId: string) =>
  apiFetch<PackageView>(`/packages/${id(packageId)}`, ro);
export const versionFileUrl = (versionId: string, path: string) =>
  `/api/v1/packages/versions/${id(versionId)}/files/${path.split('/').map(id).join('/')}`;

// ---- turning on ----
export const myEnabled = () => apiFetch<Enablement[]>('/users/@me/packages', ro);
export const enableForMe = (packageId: string, section: string) =>
  apiFetch<Enablement>(`/users/@me/packages/${id(packageId)}`, {
    method: 'PUT',
    json: { section },
    ...ro,
  });
export const disableForMe = (packageId: string) =>
  apiFetch<void>(`/users/@me/packages/${id(packageId)}`, { method: 'DELETE', ...ro });
export const guildEnabled = (guildId: string) =>
  apiFetch<Enablement[]>(`/guilds/${id(guildId)}/packages`, ro);
export const enableForGuild = (guildId: string, packageId: string, section = 'all') =>
  apiFetch<Enablement>(`/guilds/${id(guildId)}/packages/${id(packageId)}`, {
    method: 'PUT',
    json: { section },
    ...ro,
  });
export const disableForGuild = (guildId: string, packageId: string) =>
  apiFetch<void>(`/guilds/${id(guildId)}/packages/${id(packageId)}`, { method: 'DELETE', ...ro });
export const applyRecipe = (guildId: string, packageId: string) =>
  apiPost<ApplyResult>(`/guilds/${id(guildId)}/packages/${id(packageId)}/apply`, {});
export const createGuildFromPackage = (packageId: string, name: string) =>
  apiPost<{ id: string }>(`/packages/${id(packageId)}/create-guild`, { name });

// ---- creators ----
export const myPackages = () => apiFetch<MyPackage[]>('/packages/mine', ro);
export const withdrawVersion = (versionId: string) =>
  apiPost<VersionView>(`/packages/versions/${id(versionId)}/withdraw`, {});
export async function submitPackage(
  file: File,
  onProgress: (fraction: number) => void,
): Promise<VersionView> {
  const token = await getCsrf();
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', '/api/v1/packages');
    xhr.withCredentials = true;
    xhr.setRequestHeader('content-type', 'application/zip');
    if (token) xhr.setRequestHeader('X-SR-CSRF', token);
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable && e.total > 0) onProgress(e.loaded / e.total);
    };
    xhr.onload = () => {
      let body: unknown = null;
      try {
        body = JSON.parse(xhr.responseText);
      } catch {
        /* no JSON body */
      }
      if (xhr.status >= 200 && xhr.status < 300) resolve(body as VersionView);
      else
        reject(
          new ApiError(
            xhr.status,
            (body as { message?: string } | null)?.message ?? `Upload failed (${xhr.status}).`,
            body as ConstructorParameters<typeof ApiError>[2],
          ),
        );
    };
    xhr.onerror = () => reject(new Error('The upload could not be completed.'));
    xhr.send(file);
  });
}

// ---- staff ----
export const pendingPackages = () => apiFetch<PendingVersion[]>('/admin/developers/packages', ro);
export const decidePackage = (
  versionId: string,
  decision: 'approve' | 'reject' | 'revoke',
  note?: string,
) =>
  apiPost<VersionView>(`/admin/developers/packages/versions/${id(versionId)}/decision`, {
    decision,
    ...(note ? { note } : {}),
  });
export const packageDownloadUrl = (versionId: string) =>
  `/api/v1/admin/developers/packages/versions/${id(versionId)}/download`;

// ---- server look: theme, accent colour, packs added to a server (LNG-SSET-050) ----
export interface GuildAppearance {
  /** 0xRRGGBB, null when the server has none. */
  accent_color: number | null;
  theme: {
    package_id: string;
    version_id: string;
    slug: string;
    name: string;
    scheme: 'light' | 'dark';
    tokens: Record<string, string>;
  } | null;
}
export interface PackImportResult {
  enablement: Enablement;
  added: { name: string; id: string }[];
  skipped: { name: string; reason: string }[];
}
export const guildAppearance = (guildId: string) =>
  apiFetch<GuildAppearance>(`/guilds/${id(guildId)}/appearance`, ro);
export const setGuildAccent = (guildId: string, accent: number | null) =>
  apiFetch<GuildAppearance>(`/guilds/${id(guildId)}/appearance`, {
    method: 'PATCH',
    json: { accent_color: accent },
    ...ro,
  });
export const importPack = (guildId: string, packageId: string) =>
  apiPost<PackImportResult>(`/guilds/${id(guildId)}/packages/${id(packageId)}/import`, {});
