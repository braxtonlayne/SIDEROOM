import { Button, Checkbox, Modal, Notice, PasswordInput, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { withReauth } from '../../../admin/reauth';
import { ApiError } from '../../../api/client';
import { useT } from '../../../i18n';
import { apiFetch } from '../../../api/client';
import { errorText, getMe, useResource } from '../api';
import { Card, fmtDate } from '../parts';

interface SelfDeleteRun {
  id: string;
  state: 'queued' | 'running' | 'done' | 'failed';
  hidden_count?: number | null;
  requested_at: string;
}
interface Preview {
  count: number;
  last?: SelfDeleteRun | null;
}

const path = '/sr/users/@me/messages/delete-all';

const fieldMessage = (e: unknown, field: string): string | null =>
  e instanceof ApiError ? (e.body?.errors?.[field]?._errors?.[0]?.message ?? null) : null;

/**
 * Delete all my messages (N-05, D-324): hides every message and upload of the member's in servers and in
 * direct messages, with a count, a cut-off date, a typed username and the password. The server runs it in
 * the background, once a day at most; rows under a moderation hold are hidden but kept for staff.
 */
export function DeleteMessagesCard() {
  const t = useT();
  const me = useResource(getMe);
  const [servers, setServers] = useState(true);
  const [dms, setDms] = useState(true);
  const [before, setBefore] = useState('');
  const [preview, setPreview] = useState<Preview | null>(null);
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState('');
  const [password, setPassword] = useState('');
  const [code, setCode] = useState('');
  const [needCode, setNeedCode] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [queued, setQueued] = useState(false);
  const username = me.data?.username ?? '';
  const beforeMs = before ? new Date(`${before}T00:00:00`).getTime() : undefined;

  useEffect(() => {
    let live = true;
    const q = new URLSearchParams({ servers: String(servers), dms: String(dms) });
    if (beforeMs !== undefined && Number.isFinite(beforeMs)) q.set('before_ms', String(beforeMs));
    apiFetch<Preview>(`${path}?${q.toString()}`, { retries: 0 }).then(
      (p) => live && setPreview(p),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [servers, dms, beforeMs, queued]);

  const typedOk = username !== '' && typed.trim().toLowerCase() === username.toLowerCase();
  const running = preview?.last?.state === 'queued' || preview?.last?.state === 'running';
  const canOpen = (servers || dms) && !running && !queued;

  const submit = async () => {
    setError(null);
    if (!typedOk) return setError(t('settings.account.deleteUsernameMismatch'));
    if (!password) return setError(t('settings.account.enterPassword'));
    const clean = code.replace(/\s+/g, '');
    setBusy(true);
    try {
      await withReauth(() =>
        apiFetch<SelfDeleteRun>(path, {
          method: 'POST',
          retries: 0,
          json: {
            confirm_username: typed.trim(),
            password,
            servers,
            dms,
            ...(beforeMs !== undefined && Number.isFinite(beforeMs) ? { before_ms: beforeMs } : {}),
            ...(needCode && clean
              ? {
                  mfa: {
                    method: /^\d{6}$/.test(clean) ? 'totp' : 'recovery',
                    code: clean,
                  },
                }
              : {}),
          },
        }),
      );
      setOpen(false);
      setQueued(true);
      setPassword('');
      setTyped('');
      setCode('');
    } catch (e) {
      if (fieldMessage(e, 'mfa')) {
        setNeedCode(true);
        setError(fieldMessage(e, 'mfa'));
      } else {
        setError(errorText(e));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      id="us-messages"
      title={t('settings.privacy.deleteMessages.title')}
      description={t('settings.privacy.deleteMessages.description')}
    >
      <div className="us-stack">
        <Checkbox
          label={t('settings.privacy.deleteMessages.servers')}
          checked={servers}
          onChange={(e) => setServers(e.target.checked)}
        />
        <Checkbox
          label={t('settings.privacy.deleteMessages.dms')}
          checked={dms}
          onChange={(e) => setDms(e.target.checked)}
        />
        <TextInput
          label={t('settings.privacy.deleteMessages.before')}
          hint={t('settings.privacy.deleteMessages.beforeHint')}
          type="date"
          value={before}
          onChange={(e) => setBefore(e.target.value)}
        />
        {preview && (
          <p role="status">
            {t('settings.privacy.deleteMessages.count', { count: preview.count })}
          </p>
        )}
        {queued && (
          <Notice variant="success" role="status">
            {t('settings.privacy.deleteMessages.queued')}
          </Notice>
        )}
        {running && !queued && (
          <p className="us-muted">{t('settings.privacy.deleteMessages.running')}</p>
        )}
        {preview?.last?.state === 'done' && (
          <p className="us-muted">
            {t('settings.privacy.deleteMessages.lastRun', {
              when: fmtDate(preview.last.requested_at),
              count: preview.last.hidden_count ?? 0,
            })}
          </p>
        )}
        <p className="us-muted">{t('settings.privacy.deleteMessages.note')}</p>
        <div className="us-actions">
          <Button
            variant="danger"
            disabled={!canOpen || (preview?.count ?? 0) === 0}
            onClick={() => {
              setError(null);
              setOpen(true);
            }}
          >
            {t('settings.privacy.deleteMessages.button')}
          </Button>
        </div>
      </div>
      <Modal
        open={open}
        onClose={() => setOpen(false)}
        size="sm"
        title={t('settings.privacy.deleteMessages.confirmTitle')}
        description={t('settings.privacy.deleteMessages.confirmDescription', {
          count: preview?.count ?? 0,
        })}
      >
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
          noValidate
        >
          {error && (
            <Notice variant="danger" role="alert">
              {error}
            </Notice>
          )}
          <TextInput
            label={t('settings.account.deleteTypeUsername', { username })}
            autoComplete="off"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            data-autofocus=""
          />
          <PasswordInput
            label={t('settings.account.currentPassword')}
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {needCode && (
            <TextInput
              label={t('settings.account.deleteCodeLabel')}
              autoComplete="one-time-code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
            />
          )}
          <div className="us-actions">
            <Button variant="ghost" onClick={() => setOpen(false)} disabled={busy}>
              {t('settings.privacy.deleteMessages.cancel')}
            </Button>
            <Button type="submit" variant="danger" disabled={busy || !typedOk || !password}>
              {t('settings.privacy.deleteMessages.confirm')}
            </Button>
          </div>
        </form>
      </Modal>
    </Card>
  );
}
