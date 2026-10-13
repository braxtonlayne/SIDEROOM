import {
  Button,
  ConfirmDialog,
  Modal,
  Notice,
  PasswordInput,
  QRCode,
  TextInput,
  useToast,
} from '@sideroom/ui';
import { useEffect, useState } from 'react';
import {
  errorText,
  getOwnProfile,
  listSessions,
  listTrustedDevices,
  logoutAll,
  regenerateRecoveryCodes,
  resendVerification,
  revokeSession,
  revokeTrustedDevice,
  sendPasswordReset,
  totpDisable,
  totpEnroll,
  totpVerify,
  useResource,
  type Me,
  type Resource,
  type TotpEnrollment,
} from '../api';
import { Card, Field, fmtDate, Loaded, SectionTitle, Unavailable } from '../parts';

/** Asks for the account password (re-authentication) and runs `onSubmit`; the server error shows inline. */
function PasswordPrompt({
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
function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
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
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const sendLink = async () => {
    if (!me.email) return;
    setBusy(true);
    setErr(null);
    try {
      await sendPasswordReset(me.email);
      toast.show({ title: 'Reset link sent', description: `Check ${me.email}.`, duration: 4000 });
    } catch (e) {
      setErr(errorText(e));
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
      <Unavailable>
        This server has no change-password endpoint yet, so the form below is disabled. You can
        still set a new password with an emailed reset link.
      </Unavailable>
      <form className="us-stack" aria-label="Change password" onSubmit={(e) => e.preventDefault()}>
        <PasswordInput
          label="Current password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          disabled
          autoComplete="current-password"
        />
        <PasswordInput
          label="New password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          disabled
          showStrength
          autoComplete="new-password"
        />
        <div className="us-actions">
          <Button type="submit" disabled>
            Change password
          </Button>
          <Button
            variant="secondary"
            type="button"
            loading={busy}
            disabled={!me.email}
            onClick={() => void sendLink()}
          >
            Email me a reset link
          </Button>
        </div>
        {err && (
          <Notice variant="danger" title="Couldn't send the link">
            {err}
          </Notice>
        )}
      </form>
    </Card>
  );
}

function IdentityCard({ me }: { me: Me }) {
  const [reveal, setReveal] = useState(false);
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
  const masked = email.replace(
    /^(.).*(@.*)$/,
    (_m, a: string, b: string) => `${a}${'*'.repeat(6)}${b}`,
  );
  return (
    <Card id="us-identity" title="Account">
      <dl>
        <Field label="Username">{me.username ?? ''}</Field>
        <Field label="Display name">{me.global_name || me.display_name || 'Not set'}</Field>
        <Field label="Email">
          <span>{reveal ? email : masked}</span>{' '}
          <Button
            variant="link"
            size="sm"
            aria-pressed={reveal}
            onClick={() => setReveal((r) => !r)}
          >
            {reveal ? 'Hide' : 'Reveal'}
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
      <Unavailable>
        Changing your username, display name or email isn&apos;t supported by the server yet.
      </Unavailable>
    </Card>
  );
}

function DiscordCard() {
  const profile = useResource(getOwnProfile);
  const link = profile.data?.connected_accounts.find((c) => c.type === 'discord');
  return (
    <Card id="us-discord" title="Linked Discord account">
      <Loaded res={profile} what="connections">
        {() =>
          link ? (
            <p>
              Linked as <strong>{link.name}</strong>
              <span className="us-badge" data-tone={link.verified ? 'good' : undefined}>
                {link.verified ? 'Verified' : 'Unverified'}
              </span>
            </p>
          ) : (
            <p className="us-muted">No Discord account is linked.</p>
          )
        }
      </Loaded>
      <p className="us-muted">
        Linking or unlinking from settings isn&apos;t available yet; Discord is linked while signing
        in.
      </p>
    </Card>
  );
}

function TrustedDevicesCard() {
  const toast = useToast();
  const res = useResource(listTrustedDevices);
  const [error, setError] = useState<string | null>(null);
  const revoke = async (id: string) => {
    setError(null);
    try {
      await revokeTrustedDevice(id);
      res.setData((d) => d?.filter((x) => x.id !== id));
      toast.show({ title: 'Device removed', duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-trusted"
      title="Trusted devices"
      description="Devices that skip the two-factor prompt."
    >
      <Loaded res={res} what="trusted devices">
        {(rows) =>
          rows.length === 0 ? (
            <p className="us-muted">No trusted devices.</p>
          ) : (
            <ul className="us-list">
              {rows.map((d) => (
                <li key={d.id}>
                  <div className="us-row-main">
                    <strong>{d.device_name || 'Unknown device'}</strong>
                    <p className="us-muted">
                      Added {fmtDate(d.created_at)}, expires {fmtDate(d.expires_at)}
                    </p>
                  </div>
                  <Button variant="secondary" size="sm" onClick={() => void revoke(d.id)}>
                    Revoke<span className="us-sr"> {d.device_name || 'device'}</span>
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      {error && (
        <Notice variant="danger" title="Couldn't remove the device">
          {error}
        </Notice>
      )}
    </Card>
  );
}

function SessionsCard({ onSignedOut }: { onSignedOut: () => void }) {
  const toast = useToast();
  const res = useResource(listSessions);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const revoke = async (id: string) => {
    setError(null);
    try {
      await revokeSession(id);
      res.setData((d) => d?.filter((x) => x.id !== id));
      toast.show({ title: 'Session ended', duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  const everywhere = async () => {
    setBusy(true);
    try {
      await logoutAll();
      onSignedOut();
    } catch (e) {
      setError(errorText(e));
      setConfirm(false);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-sessions"
      title="Devices and sessions"
      description="Where you're signed in."
      actions={
        <Button variant="danger" onClick={() => setConfirm(true)}>
          Log out everywhere
        </Button>
      }
    >
      <Loaded res={res} what="sessions">
        {(rows) => (
          <ul className="us-list">
            {rows.map((s) => (
              <li key={s.id}>
                <div className="us-row-main">
                  <strong>{s.device_name || 'Unknown device'}</strong>
                  {s.current && <span className="us-badge">This device</span>}
                  <p className="us-muted">Last active {fmtDate(s.last_seen_at)}</p>
                </div>
                {!s.current && (
                  <Button variant="secondary" size="sm" onClick={() => void revoke(s.id)}>
                    Log out<span className="us-sr"> {s.device_name || 'session'}</span>
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title="Something went wrong">
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={confirm}
        danger
        loading={busy}
        title="Log out everywhere?"
        description="This ends every session, including this one. You will need to sign in again."
        confirmLabel="Log out everywhere"
        onConfirm={() => void everywhere()}
        onCancel={() => setConfirm(false)}
      />
    </Card>
  );
}

export function AccountSection({ me, onSignedOut }: { me: Resource<Me>; onSignedOut: () => void }) {
  return (
    <>
      <SectionTitle>My Account</SectionTitle>
      <Loaded res={me} what="your account">
        {(m) => (
          <>
            <IdentityCard me={m} />
            <PasswordCard me={m} />
            <TwoFactorCard me={m} reload={me.reload} />
            <DiscordCard />
            <TrustedDevicesCard />
            <SessionsCard onSignedOut={onSignedOut} />
          </>
        )}
      </Loaded>
    </>
  );
}
