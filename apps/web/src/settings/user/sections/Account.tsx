import {
  Avatar,
  Button,
  Modal,
  Notice,
  PasswordInput,
  QRCode,
  Switch,
  TextInput,
  useToast,
} from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../../api/client';
import { useT } from '../../../i18n';
import { cdnUrl } from '../../../store';
import { useStreamer } from '../prefs';
import {
  changePassword,
  errorText,
  regenerateRecoveryCodes,
  resendVerification,
  sendPasswordReset,
  totpDisable,
  totpEnroll,
  totpVerify,
  type Me,
  type Resource,
  type TotpEnrollment,
} from '../api';
import { Card, Field, Loaded, SectionTitle } from '../parts';
import { ChangeUsernameDialog, DisplayNameDialog } from './AccountEdit';
import { ChangeEmailDialog } from './ChangeEmail';
import { DeleteAccountDialog } from './DeleteAccount';
import { PasskeysCard } from './Passkeys';

/** Asks for the account password (re-authentication) and runs `onSubmit`; the server error shows inline. */
export function PasswordPrompt({
  open,
  title,
  description,
  confirmLabel,
  danger,
  onClose,
  onSubmit,
}: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  danger?: boolean;
  onClose: () => void;
  onSubmit: (password: string) => Promise<void>;
}) {
  const t = useT();
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setPassword('');
      setError(null);
    }
  }, [open]);
  const submit = async () => {
    if (!password) {
      setError(t('settings.account.enterPassword'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onSubmit(password);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.account.cancel')}
          </Button>
          <Button
            variant={danger ? 'danger' : 'primary'}
            loading={busy}
            onClick={() => void submit()}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <PasswordInput
          label={t('settings.account.currentPassword')}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          error={error}
          autoComplete="current-password"
          data-autofocus=""
        />
      </form>
    </Modal>
  );
}

/** The ten recovery codes, shown once (copy / download). */
export function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const t = useT();
  const toast = useToast();
  const text = codes.join('\n');
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.show({ title: t('settings.account.codesCopied'), duration: 2000 });
    } catch {
      toast.show({
        title: t('settings.account.copyFailed'),
        variant: 'warning',
      });
    }
  };
  const download = () => {
    const url = URL.createObjectURL(
      new Blob([`SIDE ROOM recovery codes\n\n${text}\n`], { type: 'text/plain' }),
    );
    const a = document.createElement('a');
    a.href = url;
    a.download = 'sideroom-recovery-codes.txt';
    a.click();
    URL.revokeObjectURL(url);
  };
  return (
    <div>
      <Notice variant="warning" title={t('settings.account.saveNowTitle')}>
        {t('settings.account.saveNowBody')}
      </Notice>
      <ul className="us-codes" aria-label={t('settings.account.recoveryCodesAria')}>
        {codes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <div className="us-actions">
        <Button variant="secondary" onClick={() => void copy()}>
          {t('settings.account.copyCodes')}
        </Button>
        <Button variant="secondary" onClick={download}>
          {t('settings.account.downloadCodes')}
        </Button>
        <Button onClick={onDone}>{t('settings.account.savedThem')}</Button>
      </div>
    </div>
  );
}

function TotpWizard({
  open,
  onClose,
  onEnabled,
}: {
  open: boolean;
  onClose: () => void;
  onEnabled: () => void;
}) {
  const t = useT();
  const [enroll, setEnroll] = useState<TotpEnrollment | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [code, setCode] = useState('');
  const [codeError, setCodeError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [codes, setCodes] = useState<string[] | null>(null);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setEnroll(null);
    setLoadError(null);
    setCode('');
    setCodeError(null);
    setCodes(null);
    totpEnroll()
      .then((e) => live && setEnroll(e))
      .catch((e: unknown) => live && setLoadError(errorText(e)));
    return () => {
      live = false;
    };
  }, [open, t]);

  const verify = async () => {
    const clean = code.replace(/\s+/g, '');
    if (!/^\d{6}$/.test(clean)) {
      setCodeError(t('settings.account.enterCode'));
      return;
    }
    setBusy(true);
    setCodeError(null);
    try {
      const res = await totpVerify(clean);
      setCodes(res.recovery_codes);
      onEnabled();
    } catch (e) {
      setCodeError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={
        codes ? t('settings.account.recoveryCodesTitle') : t('settings.account.enableAuthTitle')
      }
      size="md"
      closeOnBackdrop={false}
    >
      {codes ? (
        <RecoveryCodes codes={codes} onDone={onClose} />
      ) : loadError ? (
        <Notice variant="danger" title={t('settings.account.setupError')}>
          {loadError}
        </Notice>
      ) : !enroll ? (
        <p className="us-muted">{t('settings.account.preparingKey')}</p>
      ) : (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            void verify();
          }}
        >
          <p>{t('settings.account.scanCode')}</p>
          <div className="us-qr">
            <QRCode value={enroll.otpauth_uri} label={t('settings.account.qrLabel')} />
            <p className="us-muted">{t('settings.account.cantScan')}</p>
            <code className="us-key" aria-label={t('settings.account.manualKey')}>
              {enroll.manual_entry_key}
            </code>
          </div>
          <TextInput
            label={t('settings.account.authCode')}
            inputMode="numeric"
            autoComplete="one-time-code"
            maxLength={7}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            error={codeError}
            data-autofocus=""
          />
          <div className="us-actions">
            <Button variant="ghost" onClick={onClose}>
              {t('settings.account.cancel')}
            </Button>
            <Button type="submit" loading={busy}>
              {t('settings.account.verifyEnable')}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}

function TwoFactorCard({ me, reload }: { me: Me; reload: () => void }) {
  const t = useT();
  const toast = useToast();
  const [wizard, setWizard] = useState(false);
  const [regen, setRegen] = useState(false);
  const [disable, setDisable] = useState(false);
  const [newCodes, setNewCodes] = useState<string[] | null>(null);
  const on = !!me.mfa_enabled;
  return (
    <Card
      id="us-2fa"
      title={t('settings.account.twoFactorTitle')}
      description={t('settings.account.twoFactorDesc')}
      actions={
        on ? (
          <>
            <Button variant="secondary" onClick={() => setRegen(true)}>
              {t('settings.account.regenCodes')}
            </Button>
            <Button variant="danger" onClick={() => setDisable(true)}>
              {t('settings.account.removeAuth')}
            </Button>
          </>
        ) : (
          <Button onClick={() => setWizard(true)}>{t('settings.account.enableAuth')}</Button>
        )
      }
    >
      <p>
        {t('settings.account.status')}{' '}
        <span className="us-badge" data-tone={on ? 'good' : undefined}>
          {on ? t('settings.account.enabled') : t('settings.account.off')}
        </span>
      </p>
      <TotpWizard open={wizard} onClose={() => setWizard(false)} onEnabled={() => reload()} />
      <PasswordPrompt
        open={regen}
        title={t('settings.account.regenCodes')}
        description={t('settings.account.regenDesc')}
        confirmLabel={t('settings.account.regenConfirm')}
        onClose={() => setRegen(false)}
        onSubmit={async (pw) => {
          const res = await regenerateRecoveryCodes(pw);
          setRegen(false);
          setNewCodes(res.recovery_codes);
        }}
      />
      <Modal
        open={newCodes !== null}
        onClose={() => setNewCodes(null)}
        title={t('settings.account.newCodesTitle')}
        size="md"
        closeOnBackdrop={false}
      >
        {newCodes && <RecoveryCodes codes={newCodes} onDone={() => setNewCodes(null)} />}
      </Modal>
      <PasswordPrompt
        open={disable}
        danger
        title={t('settings.account.removeAuth')}
        description={t('settings.account.removeAuthDesc')}
        confirmLabel={t('settings.account.removeConfirm')}
        onClose={() => setDisable(false)}
        onSubmit={async (pw) => {
          await totpDisable(pw);
          setDisable(false);
          toast.show({ title: t('settings.account.twoFactorRemoved'), duration: 3000 });
          reload();
        }}
      />
    </Card>
  );
}

function PasswordCard({ me }: { me: Me }) {
  const t = useT();
  const toast = useToast();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [others, setOthers] = useState(true);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [errors, setErrors] = useState<{ current?: string; next?: string; link?: string }>({});
  const sendLink = async () => {
    if (!me.email) return;
    setSending(true);
    setErrors({});
    try {
      await sendPasswordReset(me.email);
      toast.show({
        title: t('settings.account.resetSent'),
        description: t('settings.account.checkEmail', { email: me.email }),
        duration: 4000,
      });
    } catch (e) {
      setErrors({ link: errorText(e) });
    } finally {
      setSending(false);
    }
  };
  const submit = async () => {
    if (!current) return setErrors({ current: t('settings.account.enterCurrent') });
    if (!next) return setErrors({ next: t('settings.account.enterNew') });
    setBusy(true);
    setErrors({});
    try {
      await changePassword({
        current_password: current,
        new_password: next,
        revoke_other_sessions: others,
      });
      setCurrent('');
      setNext('');
      toast.show({
        title: t('settings.account.passwordChanged'),
        description: others ? t('settings.account.othersSignedOut') : undefined,
        duration: 4000,
      });
    } catch (e) {
      const field = e instanceof ApiError ? Object.keys(e.body?.errors ?? {})[0] : undefined;
      const text = errorText(e);
      if (field === 'new_password') setErrors({ next: text });
      else setErrors({ current: text });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-password"
      title={t('settings.account.passwordTitle')}
      description={t('settings.account.passwordDesc')}
    >
      <form
        className="us-stack"
        aria-label={t('settings.account.changePasswordAria')}
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <PasswordInput
          label={t('settings.account.currentPassword')}
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          error={errors.current}
          autoComplete="current-password"
        />
        <PasswordInput
          label={t('settings.account.newPassword')}
          value={next}
          onChange={(e) => setNext(e.target.value)}
          error={errors.next}
          showStrength
          autoComplete="new-password"
        />
        <Switch
          label={t('settings.account.signOutOthers')}
          description={t('settings.account.signOutOthersDesc')}
          checked={others}
          onChange={(e) => setOthers(e.target.checked)}
        />
        <div className="us-actions">
          <Button type="submit" loading={busy}>
            {t('settings.account.changePassword')}
          </Button>
          <Button
            variant="secondary"
            type="button"
            loading={sending}
            disabled={!me.email}
            onClick={() => void sendLink()}
          >
            {t('settings.account.emailReset')}
          </Button>
        </div>
        {errors.link && (
          <Notice variant="danger" title={t('settings.account.sendLinkError')}>
            {errors.link}
          </Notice>
        )}
      </form>
    </Card>
  );
}

function IdentityCard({ me, reload }: { me: Me; reload: () => void }) {
  const t = useT();
  const [revealed, setReveal] = useState(false);
  // Streamer Mode (hide personal info) keeps the email masked and takes the Reveal button away.
  const streamerHides = useStreamer().personal;
  const reveal = revealed && !streamerHides;
  const [editName, setEditName] = useState(false);
  const [editUser, setEditUser] = useState(false);
  const [changing, setChanging] = useState(false);
  const [sent, setSent] = useState<'idle' | 'sending' | 'sent' | 'wait' | 'error'>('idle');
  const resend = async () => {
    setSent('sending');
    try {
      await resendVerification();
      setSent('sent');
    } catch (e) {
      setSent((e as { status?: number }).status === 429 ? 'wait' : 'error');
    }
  };
  const email = me.email ?? '';
  const avatar = cdnUrl('avatars', me.id, me.avatar, { size: 128 });
  const banner = cdnUrl('avatars', me.id, me.banner, { size: 512 });
  const masked = email.replace(
    /^(.).*(@.*)$/,
    (_m, a: string, b: string) => `${a}${'*'.repeat(6)}${b}`,
  );
  return (
    <Card id="us-identity" title={t('settings.account.identityTitle')}>
      <div className="us-account-head">
        <div
          className="us-account-banner"
          data-testid="account-banner"
          style={
            banner ? { backgroundImage: `url(${banner})`, backgroundSize: 'cover' } : undefined
          }
        />
        <Avatar
          name={me.global_name || me.username || t('settings.account.you')}
          src={avatar}
          size={80}
          className="us-account-avatar"
        />
      </div>
      <dl>
        <Field label={t('settings.account.username')}>
          <span>{me.username ?? ''}</span>{' '}
          <Button variant="link" size="sm" onClick={() => setEditUser(true)}>
            {t('settings.account.edit')}
            <span className="us-sr"> {t('settings.account.usernameSr')}</span>
          </Button>
        </Field>
        <Field label={t('settings.account.displayName')}>
          <span>{me.global_name || me.display_name || t('settings.account.notSet')}</span>{' '}
          <Button variant="link" size="sm" onClick={() => setEditName(true)}>
            {t('settings.account.edit')}
            <span className="us-sr"> {t('settings.account.displayNameSr')}</span>
          </Button>
        </Field>
        <Field label={t('settings.account.email')}>
          <span>{reveal ? email : masked}</span>{' '}
          {!streamerHides && (
            <Button
              variant="link"
              size="sm"
              aria-pressed={reveal}
              onClick={() => setReveal((r) => !r)}
            >
              {reveal ? t('settings.account.hide') : t('settings.account.reveal')}
            </Button>
          )}{' '}
          <Button variant="link" size="sm" onClick={() => setChanging(true)}>
            {t('settings.account.changeEmail')}
          </Button>
          <span className="us-badge" data-tone={me.verified ? 'good' : 'bad'}>
            {me.verified ? t('settings.account.verified') : t('settings.account.notVerified')}
          </span>
        </Field>
      </dl>
      {!me.verified && (
        <div className="us-actions">
          <Button
            variant="secondary"
            loading={sent === 'sending'}
            disabled={sent === 'sent'}
            onClick={() => void resend()}
          >
            {t('settings.account.resend')}
          </Button>
          <span role="status" className="us-muted">
            {sent === 'sent' && t('settings.account.resendSent')}
            {sent === 'wait' && t('settings.account.resendWait')}
            {sent === 'error' && t('settings.account.resendError')}
          </span>
        </div>
      )}
      <ChangeUsernameDialog
        open={editUser}
        current={me.username ?? ''}
        onClose={() => setEditUser(false)}
        onChanged={reload}
      />
      <DisplayNameDialog
        open={editName}
        current={me.global_name ?? ''}
        onClose={() => setEditName(false)}
        onChanged={reload}
      />
      <ChangeEmailDialog
        open={changing}
        mfaEnabled={!!me.mfa_enabled}
        currentEmail={email}
        onClose={() => setChanging(false)}
      />
    </Card>
  );
}

/** Settings, Account removal (LNG-USET-006, G-82): the retention text and the delete dialog. */
function RemovalCard({ me, onSignedOut }: { me: Me; onSignedOut: () => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <Card
      id="us-removal"
      title={t('settings.account.removalTitle')}
      description={t('settings.account.removalDesc')}
    >
      <p>{t('settings.account.removalRetained')}</p>
      <div>
        <Button variant="danger" onClick={() => setOpen(true)}>
          {t('settings.account.deleteButton')}
        </Button>
      </div>
      <DeleteAccountDialog
        open={open}
        mfaEnabled={!!me.mfa_enabled}
        username={me.username ?? ''}
        onClose={() => setOpen(false)}
        onDeleted={onSignedOut}
      />
    </Card>
  );
}

export function AccountSection({ me, onSignedOut }: { me: Resource<Me>; onSignedOut: () => void }) {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.account.title')}</SectionTitle>
      <Loaded res={me} what={t('settings.account.whatAccount')}>
        {(m) => (
          <>
            <IdentityCard me={m} reload={me.reload} />
            <PasswordCard me={m} />
            <TwoFactorCard me={m} reload={me.reload} />
            <PasskeysCard />
            <RemovalCard me={m} onSignedOut={onSignedOut} />
          </>
        )}
      </Loaded>
    </>
  );
}
