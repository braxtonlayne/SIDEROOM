import { Button, Modal, Notice, PasswordInput, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { withReauth } from '../../../admin/reauth';
import { ApiError } from '../../../api/client';
import { Link } from '@tanstack/react-router';
import { useT } from '../../../i18n';
import { useCompliancePublic } from '../../../lib/compliancePublic';
import { deleteAccount, errorText } from '../api';

/** First error message of one field in a Discord-style error tree. */
function fieldMessage(e: unknown, field: string): string | null {
  if (!(e instanceof ApiError)) return null;
  return e.body?.errors?.[field]?._errors?.[0]?.message ?? null;
}

/**
 * Delete-account dialog (LNG-USET-006, D-124, G-82) over `POST /sr/auth/account/delete`: the person types
 * their username and password, plus an authenticator or recovery code when two-factor is on. The server refuses
 * while the account is on the staff (`staff`), is the Owner (`owner`) or still owns a server (`guilds`); each
 * refusal gets its own sentence here. On success the session is already gone, so the app signs out.
 */
export function DeleteAccountDialog({
  open,
  mfaEnabled,
  username,
  onClose,
  onDeleted,
}: {
  open: boolean;
  mfaEnabled: boolean;
  username: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const t = useT();
  const compliance = useCompliancePublic();
  const [typed, setTyped] = useState('');
  const [password, setPassword] = useState('');
  const [code, setCode] = useState('');
  const [needCode, setNeedCode] = useState(mfaEnabled);
  const [errors, setErrors] = useState<{
    username?: string;
    password?: string;
    code?: string;
    blocked?: string;
  }>({});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setTyped('');
    setPassword('');
    setCode('');
    setNeedCode(mfaEnabled);
    setErrors({});
  }, [open, mfaEnabled]);

  const typedOk = typed.trim().toLowerCase() === username.toLowerCase() && username !== '';

  const submit = async () => {
    const next: typeof errors = {};
    if (!typedOk) next.username = t('settings.account.deleteUsernameMismatch');
    if (!password) next.password = t('settings.account.enterPassword');
    if (needCode && !code.trim()) next.code = t('settings.account.deleteEnterCode');
    setErrors(next);
    if (Object.keys(next).length) return;
    const clean = code.replace(/\s+/g, '');
    setBusy(true);
    try {
      await withReauth(() =>
        deleteAccount({
          password,
          username: typed.trim(),
          ...(needCode && clean
            ? {
                mfa: {
                  method: /^\d{6}$/.test(clean) ? ('totp' as const) : ('recovery' as const),
                  code: clean,
                },
              }
            : {}),
        }),
      );
      onClose();
      onDeleted();
    } catch (e) {
      if (fieldMessage(e, 'staff')) {
        setErrors({ blocked: t('settings.account.deleteStaff') });
      } else if (fieldMessage(e, 'owner')) {
        setErrors({ blocked: t('settings.account.deleteOwner') });
      } else if (fieldMessage(e, 'guilds')) {
        setErrors({ blocked: fieldMessage(e, 'guilds') ?? '' });
      } else if (fieldMessage(e, 'mfa')) {
        setNeedCode(true);
        setErrors({ code: fieldMessage(e, 'mfa') ?? undefined });
      } else if (fieldMessage(e, 'username')) {
        setErrors({ username: t('settings.account.deleteUsernameMismatch') });
      } else {
        setErrors({ password: fieldMessage(e, 'password') ?? errorText(e) });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      size="sm"
      title={t('settings.account.deleteTitle')}
      description={t('settings.account.deleteDescription')}
    >
      <form
        className="us-stack"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        noValidate
      >
        <p>
          {t('settings.account.removalRetained', {
            years: compliance.invoice_retention_years ?? 10,
          })}
        </p>
        <p>
          {t('settings.account.removalMessages')}{' '}
          <Link
            className="us-link"
            to="/settings/user/$section"
            params={{ section: 'privacy' }}
            hash="us-messages"
            onClick={onClose}
          >
            {t('settings.account.removalMessagesLink')}
          </Link>
        </p>
        {errors.blocked && (
          <Notice variant="danger" role="alert">
            {errors.blocked}
          </Notice>
        )}
        <TextInput
          label={t('settings.account.deleteTypeUsername', { username })}
          autoComplete="off"
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          error={errors.username}
          data-autofocus=""
        />
        <PasswordInput
          label={t('settings.account.currentPassword')}
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          error={errors.password}
        />
        {needCode && (
          <TextInput
            label={t('settings.account.deleteCodeLabel')}
            inputMode="text"
            autoComplete="one-time-code"
            maxLength={64}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            error={errors.code}
          />
        )}
        <div className="us-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('settings.account.cancel')}
          </Button>
          <Button type="submit" variant="danger" loading={busy} disabled={!typedOk}>
            {t('settings.account.deleteConfirm')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
