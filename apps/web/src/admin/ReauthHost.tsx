// The re-auth modal (ADM-STF-009): mounted once by the admin shell, it answers `withReauth` by collecting a
// TOTP code, a recovery code or a passkey assertion and opening the 15-minute second-factor window.
import { Button, Modal, Notice, SegmentedControl, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { passkeyVerifyBegin } from '../api/passkeys';
import { getPasskey, isCancelled, passkeysSupported } from '../lib/webauthn';
import * as api from './api';
import { onEnrollmentRequired, registerReauthPrompt, type ReauthReason } from './reauth';
import { useOpenUserSettings } from '../settings/user/nav';
import { ThemedOverlayRoot } from '../settings/user/ThemedOverlayRoot';

type Method = 'totp' | 'recovery';

/**
 * The app-root host (router.tsx). It sits outside every section's shell, which is where the `--ui-*` tokens live, so
 * without a theme root of its own the dialog had no surface, backdrop or field boxes (seen in the D-573 visual check
 * over Settings). It takes the member's Lounge theme like the player and the drawer; the admin shell keeps mounting
 * the bare host inside its own theme.
 */
export function RootReauthHost() {
  return (
    <ThemedOverlayRoot className="sr-reauth-theme">
      <ReauthHost />
    </ThemedOverlayRoot>
  );
}

export function ReauthHost() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<api.ReauthStatus | null>(null);
  const [method, setMethod] = useState<Method>('totp');
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resolver = useRef<((ok: boolean) => void) | null>(null);
  const [enrol, setEnrol] = useState(false);
  // D-573: the same dialog collects the second factor for a change to how the account signs in.
  const [reason, setReason] = useState<ReauthReason>('action');
  const openSettings = useOpenUserSettings();
  useEffect(() => onEnrollmentRequired(() => setEnrol(true)), []);

  const finish = useCallback((ok: boolean) => {
    resolver.current?.(ok);
    resolver.current = null;
    setOpen(false);
    setCode('');
    setError(null);
    setBusy(false);
  }, []);

  // Leaving the admin area with the dialog open counts as giving up.
  useEffect(() => () => resolver.current?.(false), []);

  useEffect(
    () =>
      registerReauthPrompt(
        (why) =>
          new Promise<boolean>((resolve) => {
            resolver.current = resolve;
            setReason(why);
            setStatus(null);
            setOpen(true);
            api
              .getReauthStatus()
              .then((s) => {
                // Another tab may have opened the window meanwhile.
                if (s.fresh) finish(true);
                else setStatus(s);
              })
              .catch(() =>
                setStatus({ fresh: false, enrolled: true, window_secs: 900, required: true }),
              );
          }),
      ),
    [finish],
  );

  const prove = async (m: 'totp' | 'recovery' | 'passkey', value: string) => {
    setBusy(true);
    setError(null);
    try {
      await api.submitReauth(m, value);
      finish(true);
    } catch (e) {
      setBusy(false);
      if (e instanceof ApiError && e.status === 429) setError(t('admin.reauth.tooMany'));
      else setError(t('admin.reauth.wrong'));
    }
  };

  const passkey = async () => {
    setBusy(true);
    setError(null);
    try {
      const { ceremony_id, options } = await passkeyVerifyBegin();
      const credential = await getPasskey(options);
      await prove('passkey', JSON.stringify({ ceremony_id, credential }));
    } catch (e) {
      setBusy(false);
      setError(isCancelled(e) ? t('admin.reauth.passkeyCancelled') : t('admin.reauth.wrong'));
    }
  };

  const minutes = Math.round((status?.window_secs ?? 900) / 60);
  const blocked = status !== null && !status.enrolled;
  return (
    <>
      <Modal
        open={enrol}
        onClose={() => setEnrol(false)}
        title={t('admin.reauth.enrolTitle')}
        description={t('admin.reauth.enrolDescription')}
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setEnrol(false)}>
              {t('admin.ui.cancel')}
            </Button>
            <Button
              onClick={() => {
                setEnrol(false);
                openSettings('account');
              }}
            >
              {t('admin.reauth.enrolOpen')}
            </Button>
          </>
        }
      >
        <Notice variant="warning" role="alert">
          {t('admin.reauth.enrolNotice')}
        </Notice>
      </Modal>
      <Modal
        open={open}
        onClose={() => (busy ? undefined : finish(false))}
        title={t('admin.reauth.title')}
        description={
          reason === 'signIn'
            ? t('settings.reauth.description', { minutes })
            : t('admin.reauth.description', { minutes })
        }
        size="sm"
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={() => finish(false)}>
              {t('admin.ui.cancel')}
            </Button>
            {!blocked && (
              <Button
                disabled={code.trim().length === 0}
                loading={busy}
                onClick={() => void prove(method, code.trim())}
              >
                {t('admin.reauth.confirm')}
              </Button>
            )}
          </>
        }
      >
        {blocked ? (
          <Notice variant="warning" role="alert">
            {t('admin.reauth.notEnrolled')}
          </Notice>
        ) : (
          <form
            className="adm-form"
            onSubmit={(e) => {
              e.preventDefault();
              if (code.trim()) void prove(method, code.trim());
            }}
          >
            <SegmentedControl
              label={t('admin.reauth.method')}
              value={method}
              onValueChange={(v) => {
                setMethod(v as Method);
                setCode('');
              }}
              options={[
                { value: 'totp', label: t('admin.reauth.totp') },
                { value: 'recovery', label: t('admin.reauth.recovery') },
              ]}
            />
            <TextInput
              label={
                method === 'totp' ? t('admin.reauth.totpLabel') : t('admin.reauth.recoveryLabel')
              }
              value={code}
              autoComplete="one-time-code"
              inputMode={method === 'totp' ? 'numeric' : 'text'}
              autoFocus
              onChange={(e) => setCode(e.target.value)}
            />
            {passkeysSupported() && (
              <Button variant="secondary" disabled={busy} onClick={() => void passkey()}>
                {t('admin.reauth.passkey')}
              </Button>
            )}
            {error && (
              <Notice variant="danger" role="alert">
                {error}
              </Notice>
            )}
          </form>
        )}
      </Modal>
    </>
  );
}
