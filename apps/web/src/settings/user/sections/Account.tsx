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
import { cdnUrl } from '../../../store';
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
import { Card, Field, Loaded, SectionTitle, Unavailable } from '../parts';
import { ChangeUsernameDialog, DisplayNameDialog } from './AccountEdit';
import { ChangeEmailDialog } from './ChangeEmail';
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
      setError('Enter your password.');
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
            Cancel
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
          label="Current password"
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
  const toast = useToast();
  const text = codes.join('\n');
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.show({ title: 'Copied recovery codes', duration: 2000 });
    } catch {
      toast.show({
        title: "Couldn't copy. Select the codes and copy them by hand.",
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
      <Notice variant="warning" title="Save these now">
        Each code works once if you lose your authenticator. They are shown only this time.
      </Notice>
      <ul className="us-codes" aria-label="Recovery codes">
        {codes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <div className="us-actions">
        <Button variant="secondary" onClick={() => void copy()}>
          Copy codes
        </Button>
        <Button variant="secondary" onClick={download}>
          Download codes
        </Button>
        <Button onClick={onDone}>I have saved them</Button>
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
  }, [open]);

  const verify = async () => {
    const clean = code.replace(/\s+/g, '');
    if (!/^\d{6}$/.test(clean)) {
      setCodeError('Enter the 6-digit code from your app.');
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
      title={codes ? 'Your recovery codes' : 'Enable authenticator app'}
      size="md"
      closeOnBackdrop={false}
    >
      {codes ? (
        <RecoveryCodes codes={codes} onDone={onClose} />
      ) : loadError ? (
        <Notice variant="danger" title="Couldn't start setup">
          {loadError}
        </Notice>
      ) : !enroll ? (
        <p className="us-muted">Preparing your key...</p>
      ) : (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            void verify();
          }}
        >
          <p>Scan this code with an authenticator app, then enter the 6-digit code it shows.</p>
          <div className="us-qr">
            <QRCode value={enroll.otpauth_uri} label="QR code for your authenticator app" />
            <p className="us-muted">Can&apos;t scan? Enter this key by hand:</p>
            <code className="us-key" aria-label="Manual entry key">
              {enroll.manual_entry_key}
            </code>
          </div>
          <TextInput
            label="Authentication code"
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
              Cancel
            </Button>
            <Button type="submit" loading={busy}>
              Verify and enable
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}

function TwoFactorCard({ me, reload }: { me: Me; reload: () => void }) {
  const toast = useToast();
  const [wizard, setWizard] = useState(false);
  const [regen, setRegen] = useState(false);
  const [disable, setDisable] = useState(false);
  const [newCodes, setNewCodes] = useState<string[] | null>(null);
  const on = !!me.mfa_enabled;
  return (
    <Card
      id="us-2fa"
      title="Two-factor authentication"
      description="Protect your account with a code from an authenticator app."
      actions={
        on ? (
          <>
            <Button variant="secondary" onClick={() => setRegen(true)}>
              Regenerate recovery codes
            </Button>
            <Button variant="danger" onClick={() => setDisable(true)}>
              Remove authenticator
            </Button>
          </>
        ) : (
          <Button onClick={() => setWizard(true)}>Enable authenticator app</Button>
        )
      }
    >
      <p>
        Status:{' '}
        <span className="us-badge" data-tone={on ? 'good' : undefined}>
          {on ? 'Enabled' : 'Off'}
        </span>
      </p>
      <TotpWizard open={wizard} onClose={() => setWizard(false)} onEnabled={() => reload()} />
      <PasswordPrompt
        open={regen}
        title="Regenerate recovery codes"
        description="This replaces your current codes. Enter your password to continue."
        confirmLabel="Regenerate"
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
        title="Your new recovery codes"
        size="md"
        closeOnBackdrop={false}
      >
        {newCodes && <RecoveryCodes codes={newCodes} onDone={() => setNewCodes(null)} />}
      </Modal>
      <PasswordPrompt
        open={disable}
        danger
        title="Remove authenticator"
        description="You will sign in with only your password. Enter it to confirm."
        confirmLabel="Remove"
        onClose={() => setDisable(false)}
        onSubmit={async (pw) => {
          await totpDisable(pw);
          setDisable(false);
          toast.show({ title: 'Two-factor authentication removed', duration: 3000 });
          reload();
        }}
      />
    </Card>
  );
}

function PasswordCard({ me }: { me: Me }) {
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
      toast.show({ title: 'Reset link sent', description: `Check ${me.email}.`, duration: 4000 });
    } catch (e) {
      setErrors({ link: errorText(e) });
    } finally {
      setSending(false);
    }
  };
  const submit = async () => {
    if (!current) return setErrors({ current: 'Enter your current password.' });
    if (!next) return setErrors({ next: 'Enter a new password.' });
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
        title: 'Password changed',
        description: others ? 'Your other devices were signed out.' : undefined,
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
      title="Password"
      description="Choose a strong password you don't use anywhere else."
    >
      <form
        className="us-stack"
        aria-label="Change password"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <PasswordInput
          label="Current password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          error={errors.current}
          autoComplete="current-password"
        />
        <PasswordInput
          label="New password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          error={errors.next}
          showStrength
          autoComplete="new-password"
        />
        <Switch
          label="Sign out my other devices"
          description="Ends every other session and trusted device. This device stays signed in."
          checked={others}
          onChange={(e) => setOthers(e.target.checked)}
        />
        <div className="us-actions">
          <Button type="submit" loading={busy}>
            Change password
          </Button>
          <Button
            variant="secondary"
            type="button"
            loading={sending}
            disabled={!me.email}
            onClick={() => void sendLink()}
          >
            Email me a reset link
          </Button>
        </div>
        {errors.link && (
          <Notice variant="danger" title="Couldn't send the link">
            {errors.link}
          </Notice>
        )}
      </form>
    </Card>
  );
}

function IdentityCard({ me, reload }: { me: Me; reload: () => void }) {
  const [reveal, setReveal] = useState(false);
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
    <Card id="us-identity" title="Account">
      <div className="us-account-head">
        <div
          className="us-account-banner"
          data-testid="account-banner"
          style={
            banner ? { backgroundImage: `url(${banner})`, backgroundSize: 'cover' } : undefined
          }
        />
        <Avatar
          name={me.global_name || me.username || 'You'}
          src={avatar}
          size={80}
          className="us-account-avatar"
        />
      </div>
      <dl>
        <Field label="Username">
          <span>{me.username ?? ''}</span>{' '}
          <Button variant="link" size="sm" onClick={() => setEditUser(true)}>
            Edit<span className="us-sr"> username</span>
          </Button>
        </Field>
        <Field label="Display name">
          <span>{me.global_name || me.display_name || 'Not set'}</span>{' '}
          <Button variant="link" size="sm" onClick={() => setEditName(true)}>
            Edit<span className="us-sr"> display name</span>
          </Button>
        </Field>
        <Field label="Email">
          <span>{reveal ? email : masked}</span>{' '}
          <Button
            variant="link"
            size="sm"
            aria-pressed={reveal}
            onClick={() => setReveal((r) => !r)}
          >
            {reveal ? 'Hide' : 'Reveal'}
          </Button>{' '}
          <Button variant="link" size="sm" onClick={() => setChanging(true)}>
            Change email
          </Button>
          <span className="us-badge" data-tone={me.verified ? 'good' : 'bad'}>
            {me.verified ? 'Verified' : 'Not verified'}
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
            Resend verification email
          </Button>
          <span role="status" className="us-muted">
            {sent === 'sent' && 'Check your inbox for the link.'}
            {sent === 'wait' && 'A link was sent a moment ago. Try again in a minute.'}
            {sent === 'error' && "We couldn't send the link. Try again in a moment."}
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

export function AccountSection({ me }: { me: Resource<Me>; onSignedOut: () => void }) {
  return (
    <>
      <SectionTitle>My Account</SectionTitle>
      <Loaded res={me} what="your account">
        {(m) => (
          <>
            <IdentityCard me={m} reload={me.reload} />
            <PasswordCard me={m} />
            <TwoFactorCard me={m} reload={me.reload} />
            <PasskeysCard />
            <Card
              id="us-removal"
              title="Account removal"
              description="Deleting your account is permanent: your messages stay in the servers they were posted in, and you need a new invite to return."
            >
              <Unavailable>
                The server cannot delete accounts yet (it needs the deletion process and a data
                scrub). Ask a server owner or the staff to remove your account in the meantime.
              </Unavailable>
            </Card>
          </>
        )}
      </Loaded>
    </>
  );
}
