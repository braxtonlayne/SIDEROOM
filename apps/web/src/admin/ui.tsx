// Shared pieces for the Admin pages: viewer context, reason-required modal, cursor paging, small widgets.
import { Badge, Button, Modal, Notice, TextArea, type BadgeVariant } from '@sideroom/ui';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { errorText } from './api';

export { errorText };
import { hasPerm, type AdminPerm } from './permissions';
import '../settings/server/server-settings.css';
import './admin.css';

export {
  SectionHeader,
  Loading,
  LoadError,
  Async,
  useAction,
  ActionStatus,
} from '../settings/server/ui';
export { useResource, type Resource } from '../settings/server/hooks';

// ---- viewer ----

export interface AdminViewer {
  userId: string;
  isOwner: boolean;
  perms: bigint;
  can: (p: AdminPerm) => boolean;
}
export const AdminViewerContext = createContext<AdminViewer | null>(null);
export function useAdminViewer(): AdminViewer {
  const v = useContext(AdminViewerContext);
  if (!v) throw new Error('useAdminViewer outside the admin shell');
  return v;
}
export const makeViewer = (userId: string, isOwner: boolean, perms: bigint): AdminViewer => ({
  userId,
  isOwner,
  perms,
  can: (p) => hasPerm(perms, p),
});

// ---- formatting ----

export const fmtMs = (ms: number | null | undefined) => (ms ? new Date(ms).toLocaleString() : '');
export const fmtUs = (us: number | null | undefined) =>
  us ? new Date(us / 1000).toLocaleString() : '';
export const shortHash = (h: string | null | undefined) => (h ? `${h.slice(0, 12)}…` : '');

const STATE_VARIANT: Record<string, BadgeVariant> = {
  active: 'success',
  deactivated: 'warning',
  deleted: 'neutral',
  platform_banned: 'danger',
  open: 'accent',
  redeemed: 'success',
  revoked: 'neutral',
  expired: 'neutral',
  dead: 'danger',
  failed: 'danger',
  queued: 'neutral',
  running: 'accent',
  done: 'success',
};
export function StateBadge({ state }: { state: string }) {
  return <Badge variant={STATE_VARIANT[state] ?? 'neutral'}>{state.replace(/_/g, ' ')}</Badge>;
}

export function Stat({
  label,
  value,
  hint,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
}) {
  return (
    <div className="adm-stat">
      <div className="adm-stat-value">{value}</div>
      <div className="adm-stat-label">{label}</div>
      {hint && <div className="adm-stat-hint">{hint}</div>}
    </div>
  );
}

/** An API failure as a banner; 403s get dedicated copy so staff know it is a permission, not an outage. */
export function ApiProblem({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <Notice variant="danger" className="ss-status" role="alert">
      {error}
    </Notice>
  );
}

// ---- reason-required action modal (ADM-UI-003) ----

export interface ReasonDialogProps {
  open: boolean;
  title: string;
  description?: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  /** Reason is mandatory by default (the API refuses an empty one). */
  reasonOptional?: boolean;
  reasonLabel?: string;
  /** Extra form fields rendered above the reason. */
  children?: ReactNode;
  /** Disable confirm until the extra fields are valid. */
  valid?: boolean;
  /** Resolve to close; reject (or throw) to show the error and stay open. */
  onConfirm: (reason: string) => Promise<unknown>;
  onCancel: () => void;
}

export function ReasonDialog({
  open,
  title,
  description,
  confirmLabel,
  danger,
  reasonOptional,
  reasonLabel = 'Reason (recorded in the audit log)',
  children,
  valid = true,
  onConfirm,
  onCancel,
}: ReasonDialogProps) {
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) {
      setReason('');
      setError(null);
      setBusy(false);
    }
  }, [open]);
  const trimmed = reason.trim();
  const ready = valid && (reasonOptional || trimmed.length > 0);
  const submit = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm(trimmed);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={busy ? () => undefined : onCancel}
      title={title}
      description={description}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant={danger ? 'danger' : 'primary'}
            disabled={!ready}
            loading={busy}
            onClick={() => void submit()}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <form
        className="adm-form"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        {children}
        <TextArea
          label={reasonLabel}
          value={reason}
          maxLength={500}
          rows={3}
          required={!reasonOptional}
          onChange={(e) => setReason(e.target.value)}
        />
        <ApiProblem error={error} />
      </form>
    </Modal>
  );
}

// ---- cursor paging ----

export interface Paged<T> {
  items: T[];
  loading: boolean;
  error: string | null;
  more: boolean;
  loadMore: () => void;
  reload: () => void;
}

/**
 * Accumulating cursor pages. `fetchPage(cursor)` returns `{ items, next }`; `key` resets the list (new
 * filter). Stale responses from an older key are dropped.
 */
export function usePaged<T, C = string>(
  key: string,
  fetchPage: (cursor: C | null) => Promise<{ items: T[]; next: C | null | undefined }>,
): Paged<T> {
  const [items, setItems] = useState<T[]>([]);
  const [next, setNext] = useState<C | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const fetchRef = useRef(fetchPage);
  fetchRef.current = fetchPage;
  const keyRef = useRef(key);
  keyRef.current = key;
  const [nonce, setNonce] = useState(0);

  const run = useCallback(async (cursor: C | null, append: boolean) => {
    const forKey = keyRef.current;
    setLoading(true);
    try {
      const page = await fetchRef.current(cursor);
      if (keyRef.current !== forKey) return;
      setItems((cur) => (append ? [...cur, ...page.items] : page.items));
      setNext(page.next ?? null);
      setError(null);
    } catch (e) {
      if (keyRef.current === forKey) setError(errorText(e, 'Could not load this.'));
    } finally {
      if (keyRef.current === forKey) setLoading(false);
    }
  }, []);

  useEffect(() => {
    setItems([]);
    setNext(null);
    void run(null, false);
  }, [key, nonce, run]);

  return {
    items,
    loading,
    error,
    more: next !== null,
    loadMore: () => void run(next, true),
    reload: () => setNonce((n) => n + 1),
  };
}

export function LoadMore({ paged }: { paged: Paged<unknown> }) {
  if (!paged.more) return null;
  return (
    <div className="adm-more">
      <Button variant="secondary" loading={paged.loading} onClick={paged.loadMore}>
        Load more
      </Button>
    </div>
  );
}
