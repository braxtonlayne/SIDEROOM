// Wire shapes ASSUMED for BYOS-MNT-001..003 (roadmap-p27.md section 5). byos-core must match these;
// they are listed in docs/dev/handoff/feat-p27-byos-ui.md. The secret key is never returned: the view
// carries the access key id and `key_hint` (last four characters of the secret).

export type MountProvider = 's3' | 'r2' | 'b2' | 'minio' | 'wasabi' | 'garage';
export const MOUNT_PROVIDERS: MountProvider[] = ['s3', 'r2', 'b2', 'wasabi', 'minio', 'garage'];

export type MountStatus = 'active' | 'failing' | 'disabled';
export type MountOwnerKind = 'user' | 'guild' | 'platform';

export type ProbeStatus = 'pass' | 'warn' | 'fail';
/** `id` is the probe: head_bucket, list, put, multipart, cors, list_buckets. */
export interface ProbeRow {
  id: string;
  status: ProbeStatus;
  message: string;
}
/** `POST .../mounts/{id}/test` answer; the same object is stored as `last_test`. */
export interface TestResult {
  at: string;
  rows: ProbeRow[];
}

export interface Mount {
  id: string;
  owner_kind: MountOwnerKind;
  owner_id: string | null;
  label: string;
  provider: MountProvider;
  endpoint: string | null;
  region: string | null;
  bucket: string;
  prefix: string;
  path_style: boolean;
  access_key_id: string;
  key_hint: string;
  status: MountStatus;
  cors_ok_at: string | null;
  last_test_at: string | null;
  last_test: TestResult | null;
  created_at: string;
}

/** `POST /users/@me/storage/mounts` and the guild twin. `secret_access_key` is write-only. */
export interface MountCreate {
  label: string;
  provider: MountProvider;
  endpoint?: string;
  region?: string;
  bucket: string;
  prefix?: string;
  path_style: boolean;
  access_key_id: string;
  secret_access_key: string;
}
/** `PATCH`: any subset; a new `secret_access_key` re-seals the credential. */
export type MountPatch = Partial<MountCreate> & { status?: 'active' | 'disabled' };

export type CorsState = 'ok' | 'missing' | 'different' | 'manual';
/** `POST .../cors/check` and `.../cors/apply` answer. `manual_steps` is provider prose when the API refuses. */
export interface CorsResult {
  state: CorsState;
  required: string[];
  found: string[];
  manual_steps?: string | null;
  checked_at: string;
}
