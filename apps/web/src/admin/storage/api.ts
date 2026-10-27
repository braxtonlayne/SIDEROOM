// Admin > Storage (BYOS-LEG-001): /admin/storage/mounts. Ids are snowflake strings.
import { apiFetch } from '../../api/client';
import { withReauth } from '../reauth';

export type MountStatus = 'active' | 'failing' | 'disabled' | 'delisted';

export interface AdminStorageMount {
  id: string;
  owner_kind: 'user' | 'guild' | 'platform';
  owner_id: string;
  owner_name: string;
  label: string;
  provider: string;
  bucket: string;
  status: MountStatus;
  key_hint: string;
  created_at_ms: number;
  delisted_at_ms: number | null;
  delist_reason: string;
  delisted_objects: number;
}

export const PAGE = 50;

export const listMounts = (p: { status?: MountStatus | ''; before?: string | null }) => {
  const q = new URLSearchParams({ limit: String(PAGE) });
  if (p.status) q.set('status', p.status);
  if (p.before) q.set('before', p.before);
  return withReauth(() =>
    apiFetch<AdminStorageMount[]>(`/admin/storage/mounts?${q.toString()}`, { retries: 0 }),
  );
};

export const delistMount = (id: string, reason: string) =>
  withReauth(() =>
    apiFetch<{ id: string; status: string }>(`/admin/storage/mounts/${id}/delist`, {
      method: 'POST',
      retries: 0,
      json: { reason },
    }),
  );

export const relistMount = (id: string, reason: string) =>
  withReauth(() =>
    apiFetch<{ id: string; status: string }>(`/admin/storage/mounts/${id}/relist`, {
      method: 'POST',
      retries: 0,
      json: { reason },
    }),
  );
