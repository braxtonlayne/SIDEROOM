import { GoldButton, PasswordInput, TextInput } from '@sideroom/ui';
import { Link, useRouter, useSearch } from '@tanstack/react-router';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { discordStartUrl, login } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { Divider, DiscordButton, FormError } from '../../components/FormParts';
import { LockIcon } from '../../components/icons';
import { PasskeyButton } from '../../components/PasskeyButton';
import { rememberMfaMethods } from '../../lib/mfaMethods';
import { minutesFromSeconds, safeNext } from '../../lib/validation';
import { useRedirectIfLoggedIn } from '../../lib/useRedirectIfLoggedIn';
import { useUiStore } from '../../state/session';

export function loginErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 429) {
      const m = minutesFromSeconds(e.retryAfter);
      return `Too many attempts. Try again in ${m} minute${m === 1 ? '' : 's'}.`;
    }
    if (e.status === 401 || e.status === 400) return "That doesn't match our list.";
  }
  return "We couldn't reach the door. Check your connection and try again.";
}

export function Login() {
  useRedirectIfLoggedIn();
  const router = useRouter();
  const search = useSearch({ strict: false }) as { next?: string; error?: string };
  const next = safeNext(search.next);
  const [id, setId] = useState('');
  const [pw, setPw] = useState('');
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(
    search.error === 'discord_unlinked'
      ? "No SIDE ROOM account is linked to that Discord account. You'll need an invitation."
      : null,
  );

  const finish = () => router.history.push(next ?? useUiStore.getState().lastSection);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (!id.trim() || !pw) return;
    setBusy(true);
    setError(null);
    try {
      const res = await login({ login: id.trim(), password: pw });
      if (res.status === 'mfa_required') {
        rememberMfaMethods(res.methods);
        void router.navigate({ to: '/login/2fa', search: next ? { next } : {} });
      } else finish();
    } catch (err) {
      setError(loginErrorMessage(err));
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow="Your usual place."
      heading="Good to have you"
      accent="back."
      copy={
        <>
          Same people. New stories.
          <br />
          Pick up where you left off.
        </>
      }
      footLabel="Member entrance / 01"
    >
      <FormHead
        eyebrow="On the other side"
        title="Come on in."
        sub="Sign in to your corner of the internet."
      />
      <DiscordButton href={discordStartUrl('login', next ?? undefined)}>
        Continue with Discord
      </DiscordButton>
      <Divider>or make yourself at home</Divider>
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{error}</FormError>
        <TextInput
          label="Username or email"
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder="Your username or email address"
          autoComplete="username webauthn"
          error={touched && !id.trim() ? 'Enter your username or email.' : undefined}
        />
        <div>
          <div className="ent-row">
            <span />
            <Link to="/forgot">Forgot password?</Link>
          </div>
          <PasswordInput
            label="Password"
            value={pw}
            onChange={(e) => setPw(e.target.value)}
            placeholder="Your password"
            autoComplete="current-password"
            error={touched && !pw ? 'Enter your password.' : undefined}
          />
        </div>
        <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
          {busy ? 'Stepping inside…' : 'Step inside'}
        </GoldButton>
        <PasskeyButton onDone={finish} onError={setError} />
      </form>
      <Link to="/" className="ent-newhere">
        <LockIcon />
        <span>
          For people already on the list.
          <small>New here? You’ll need an invitation.</small>
        </span>
        <span aria-hidden="true">↗</span>
      </Link>
    </AuthSplit>
  );
}
