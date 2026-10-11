import { Checkbox, GoldButton, PasswordInput, TextInput } from '@sideroom/ui';
import { Link, useRouter } from '@tanstack/react-router';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import {
  checkUsername,
  discordStartUrl,
  getPendingDiscord,
  register,
  unlinkPendingDiscord,
  type DiscordLink,
} from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { Divider, DiscordButton, FormError } from '../../components/FormParts';
import { clearInviteCode, heldInviteCode } from '../../lib/inviteCode';
import {
  USERNAME_HINT,
  validateEmail,
  validatePassword,
  validateUsername,
} from '../../lib/validation';

type Field = 'email' | 'username' | 'display_name' | 'password' | 'confirm' | 'rules';
type UsernameState = 'idle' | 'checking' | 'free' | 'taken';

function fieldErrorsFrom(e: ApiError): Partial<Record<Field, string>> {
  const out: Partial<Record<Field, string>> = {};
  for (const [k, v] of Object.entries(e.body?.errors ?? {})) {
    const m = v._errors?.[0]?.message;
    if (m) out[k as Field] = m;
  }
  return out;
}

export function Register() {
  const router = useRouter();
  const [code] = useState(() => heldInviteCode());
  const [email, setEmail] = useState('');
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [rules, setRules] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [unameState, setUnameState] = useState<UsernameState>('idle');
  const [discord, setDiscord] = useState<DiscordLink | null>(null);
  const userEdited = useRef(false);

  // No code in hand: back to the entrance.
  useEffect(() => {
    if (!code) router.history.replace('/');
  }, [code, router]);

  // Returning from the Discord OAuth round trip: prefill from the pending link.
  useEffect(() => {
    let live = true;
    getPendingDiscord()
      .then((d) => {
        if (!live || !d) return;
        setDiscord(d);
        if (d.email) setEmail((v) => v || d.email!);
        if (!userEdited.current)
          setUsername(
            d.username
              .toLowerCase()
              .replace(/[^a-z0-9_.]/g, '')
              .slice(0, 32),
          );
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  // Debounced live availability check.
  useEffect(() => {
    if (validateUsername(username)) {
      setUnameState('idle');
      return;
    }
    setUnameState('checking');
    let live = true;
    const t = setTimeout(() => {
      checkUsername(username)
        .then((r) => live && setUnameState(r.available ? 'free' : 'taken'))
        .catch(() => live && setUnameState('idle'));
    }, 400);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [username]);

  const validate = (): Partial<Record<Field, string>> => {
    const e: Partial<Record<Field, string>> = {};
    const em = validateEmail(email);
    if (em) e.email = em;
    const un = validateUsername(username);
    if (un) e.username = un;
    const pw = validatePassword(password);
    if (pw) e.password = pw;
    if (confirm !== password) e.confirm = "Those passwords don't match.";
    if (!rules) e.rules = 'Please confirm you have read the house notes.';
    return e;
  };

  const onSubmit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (!code) return;
    let taken = unameState === 'taken';
    if (unameState === 'checking' && !validateUsername(username)) {
      // The debounced check hasn't landed yet; settle it before submitting.
      taken = await checkUsername(username).then(
        (r) => !r.available,
        () => false,
      );
      setUnameState(taken ? 'taken' : 'free');
    }
    const e = validate();
    if (taken) e.username = 'That username is taken.';
    setErrors(e);
    setFormError(null);
    if (Object.keys(e).length) return;
    setBusy(true);
    try {
      await register({
        code,
        email: email.trim(),
        username,
        display_name: displayName.trim() || undefined,
        password,
        accepted_house_notes: true,
      });
      clearInviteCode();
      router.history.push('/channels');
    } catch (err) {
      if (err instanceof ApiError) {
        const fe = fieldErrorsFrom(err);
        setErrors(fe);
        if (err.status === 429) setFormError('Too many tries. Take a breath and try again later.');
        else if (!Object.keys(fe).length)
          setFormError(
            err.status === 403 || err.status === 404
              ? "That invitation doesn't open this door."
              : "We couldn't open the door. Check the details and try again.",
          );
      } else setFormError("We couldn't reach the door. Check your connection and try again.");
      setBusy(false);
    }
  };

  if (!code) return null;

  const unameHint =
    unameState === 'checking' ? 'Checking…' : unameState === 'free' ? 'Available.' : USERNAME_HINT;

  return (
    <AuthSplit
      eyebrow="Your invitation worked."
      heading="Make yourself"
      accent="at home."
      copy="A few details and the door is yours."
      footLabel="Member entrance / 00"
    >
      <FormHead eyebrow="Almost inside" title="Open the door." sub="Create your account." />
      {discord ? (
        <p className="ent-chip">
          Linked to Discord as @{discord.username}
          <button
            type="button"
            aria-label="Unlink Discord"
            onClick={() => {
              void unlinkPendingDiscord().catch(() => {});
              setDiscord(null);
            }}
          >
            ×
          </button>
        </p>
      ) : (
        <>
          <DiscordButton href={discordStartUrl('register')}>Continue with Discord</DiscordButton>
          <Divider>or start fresh</Divider>
        </>
      )}
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{formError}</FormError>
        <TextInput
          label="Email"
          type="email"
          autoComplete="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          error={errors.email}
        />
        <TextInput
          label="Username"
          autoComplete="username"
          autoCapitalize="none"
          spellCheck={false}
          value={username}
          onChange={(e) => {
            userEdited.current = true;
            setUsername(e.target.value.toLowerCase());
          }}
          hint={unameHint}
          error={errors.username}
        />
        <TextInput
          label="Display name (optional)"
          autoComplete="nickname"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          error={errors.display_name}
        />
        <PasswordInput
          label="Password"
          showStrength
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          hint="At least 10 characters."
          error={errors.password}
        />
        <PasswordInput
          label="Confirm password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          error={errors.confirm}
        />
        <div>
          <Checkbox
            label={
              <>
                I’ve read the <Link to="/house-notes">house notes</Link>.
              </>
            }
            checked={rules}
            onChange={(e) => setRules(e.target.checked)}
          />
          {errors.rules && (
            <p role="alert" className="ent-footnote">
              {errors.rules}
            </p>
          )}
        </div>
        <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
          {busy ? 'Opening…' : 'Open the door'}
        </GoldButton>
        <p className="ent-footnote">Your invitation is single-use. Once you’re in, it’s spent.</p>
      </form>
    </AuthSplit>
  );
}
