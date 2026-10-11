import { GoldButton, PasswordInput, TextInput } from '@sideroom/ui';
import { Link, useParams } from '@tanstack/react-router';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { forgotPassword, resetPassword } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { FormError } from '../../components/FormParts';
import { minutesFromSeconds, validateEmail, validatePassword } from '../../lib/validation';

const NETWORK = "We couldn't reach the door. Check your connection and try again.";

export function Forgot() {
  const [email, setEmail] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const v = validateEmail(email);
    setErr(v);
    setError(null);
    if (v) return;
    setBusy(true);
    try {
      await forgotPassword(email.trim());
      setSent(true);
    } catch (ex) {
      if (ex instanceof ApiError && ex.status === 429) {
        const m = minutesFromSeconds(ex.retryAfter);
        setError(`Too many tries. Try again in ${m} minute${m === 1 ? '' : 's'}.`);
      } else setError(NETWORK);
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow="Locked out?"
      heading="No harm,"
      accent="no fuss."
      copy="We'll send a link to get you back in."
      footLabel="Member entrance / 01"
    >
      <FormHead
        eyebrow="Forgot password"
        title="Let’s fix that."
        sub="Enter the email on your account."
      />
      {sent ? (
        <p role="status" className="ent-sub">
          If that address is on the list, a link is on its way. It works once, for 30 minutes.
        </p>
      ) : (
        <form className="ent-form" onSubmit={onSubmit} noValidate>
          <FormError>{error}</FormError>
          <TextInput
            label="Email"
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            error={err ?? undefined}
          />
          <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
            {busy ? 'Sending…' : 'Send the link'}
          </GoldButton>
        </form>
      )}
      <p className="ent-footnote">
        <Link to="/login">Back to sign in</Link>
      </p>
    </AuthSplit>
  );
}

export function Reset() {
  const { token } = useParams({ strict: false }) as { token: string };
  const [pw, setPw] = useState('');
  const [confirm, setConfirm] = useState('');
  const [errors, setErrors] = useState<{ pw?: string; confirm?: string }>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const next = {
      pw: validatePassword(pw) ?? undefined,
      confirm: confirm !== pw ? "Those passwords don't match." : undefined,
    };
    setErrors(next);
    setError(null);
    if (next.pw || next.confirm) return;
    setBusy(true);
    try {
      await resetPassword(token, pw);
      setDone(true);
    } catch (ex) {
      setError(
        ex instanceof ApiError && (ex.status === 400 || ex.status === 404 || ex.status === 410)
          ? 'That link has expired or was already used. Ask for a new one.'
          : NETWORK,
      );
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow="Fresh start."
      heading="A new key,"
      accent="same door."
      copy="Choose a password you haven't used anywhere else."
      footLabel="Member entrance / 01"
    >
      <FormHead eyebrow="Reset password" title="Choose a new one." />
      {done ? (
        <>
          <p role="status" className="ent-sub">
            Password changed. If you use two-step sign-in, you’ll still be asked for your code.
          </p>
          <Link to="/login">Back to sign in</Link>
        </>
      ) : (
        <form className="ent-form" onSubmit={onSubmit} noValidate>
          <FormError>{error}</FormError>
          <PasswordInput
            label="New password"
            showStrength
            autoComplete="new-password"
            value={pw}
            onChange={(e) => setPw(e.target.value)}
            hint="At least 10 characters."
            error={errors.pw}
          />
          <PasswordInput
            label="Confirm new password"
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            error={errors.confirm}
          />
          <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
            {busy ? 'Saving…' : 'Set password'}
          </GoldButton>
        </form>
      )}
    </AuthSplit>
  );
}
