import { GoldButton, PasswordInput, TextInput } from '@sideroom/ui';
import { useParams } from '@tanstack/react-router';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { forgotPassword, resetPassword } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { FormError } from '../../components/FormParts';
import { useT } from '../../i18n/react';
import { minutesFromSeconds, validateEmail, validatePassword } from '../../lib/validation';

export function Forgot() {
  const t = useT();
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
        setError(t('entrance.common.tooManyTries', { minutes: minutesFromSeconds(ex.retryAfter) }));
      } else setError(t('entrance.common.unreachable'));
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow={t('entrance.forgot.eyebrow')}
      heading={t('entrance.forgot.heading')}
      accent={t('entrance.forgot.headingAccent')}
      copy={t('entrance.forgot.copy')}
      footLabel={t('entrance.forgot.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.forgot.formEyebrow')}
        title={t('entrance.forgot.title')}
        sub={t('entrance.forgot.sub')}
      />
      {sent ? (
        <p role="status" className="ent-sub">
          {t('entrance.forgot.sent')}
        </p>
      ) : (
        <form className="ent-form" onSubmit={onSubmit} noValidate>
          <FormError>{error}</FormError>
          <TextInput
            label={t('entrance.forgot.email')}
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            error={err ?? undefined}
          />
          <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
            {busy ? t('entrance.forgot.sending') : t('entrance.forgot.submit')}
          </GoldButton>
        </form>
      )}
      <p className="ent-footnote">
        <a href="/login">{t('entrance.common.backToSignIn')}</a>
      </p>
    </AuthSplit>
  );
}

export function Reset() {
  const t = useT();
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
      confirm: confirm !== pw ? t('entrance.reset.mismatch') : undefined,
    };
    setErrors(next);
    setError(null);
    if (next.pw || next.confirm) return;
    setBusy(true);
    try {
      await resetPassword(token, pw);
      setDone(true);
    } catch (ex) {
      // The server's zxcvbn verdict is authoritative (400 with errors.password); the link stays valid.
      const weak =
        ex instanceof ApiError ? ex.body?.errors?.password?._errors?.[0]?.message : undefined;
      if (weak) {
        setErrors({ pw: weak });
        setBusy(false);
        return;
      }
      setError(
        ex instanceof ApiError && (ex.status === 400 || ex.status === 404 || ex.status === 410)
          ? t('entrance.reset.expired')
          : t('entrance.common.unreachable'),
      );
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow={t('entrance.reset.eyebrow')}
      heading={t('entrance.reset.heading')}
      accent={t('entrance.reset.headingAccent')}
      copy={t('entrance.reset.copy')}
      footLabel={t('entrance.reset.footLabel')}
    >
      <FormHead eyebrow={t('entrance.reset.formEyebrow')} title={t('entrance.reset.title')} />
      {done ? (
        <>
          <p role="status" className="ent-sub">
            {t('entrance.reset.done')}
          </p>
          <a href="/login">{t('entrance.common.backToSignIn')}</a>
        </>
      ) : (
        <form className="ent-form" onSubmit={onSubmit} noValidate>
          <FormError>{error}</FormError>
          <PasswordInput
            label={t('entrance.reset.password')}
            showStrength
            autoComplete="new-password"
            value={pw}
            onChange={(e) => setPw(e.target.value)}
            hint={t('entrance.reset.passwordHint')}
            error={errors.pw}
          />
          <PasswordInput
            label={t('entrance.reset.confirm')}
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            error={errors.confirm}
          />
          <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
            {busy ? t('entrance.reset.saving') : t('entrance.reset.submit')}
          </GoldButton>
        </form>
      )}
    </AuthSplit>
  );
}
