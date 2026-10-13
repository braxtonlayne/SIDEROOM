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
import { useT } from '../../i18n/react';
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
  const t = useT();
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
    const timer = setTimeout(() => {
      checkUsername(username)
        .then((r) => live && setUnameState(r.available ? 'free' : 'taken'))
        .catch(() => live && setUnameState('idle'));
    }, 400);
    return () => {
      live = false;
      clearTimeout(timer);
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
    if (confirm !== password) e.confirm = t('entrance.register.mismatch');
    if (!rules) e.rules = t('entrance.register.rulesRequired');
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
    if (taken) e.username = t('entrance.register.nameTaken');
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
        if (err.status === 429) setFormError(t('entrance.register.tooMany'));
        else if (!Object.keys(fe).length)
          setFormError(
            err.status === 403 || err.status === 404
              ? t('entrance.register.badInvite')
              : t('entrance.register.failed'),
          );
      } else setFormError(t('entrance.register.unreachable'));
      setBusy(false);
    }
  };

  if (!code) return null;

  const unameHint =
    unameState === 'checking'
      ? t('entrance.register.checkingName')
      : unameState === 'free'
        ? t('entrance.register.nameFree')
        : USERNAME_HINT;

  return (
    <AuthSplit
      eyebrow={t('entrance.register.eyebrow')}
      heading={t('entrance.register.heading')}
      accent={t('entrance.register.headingAccent')}
      copy={t('entrance.register.copy')}
      footLabel={t('entrance.register.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.register.formEyebrow')}
        title={t('entrance.register.title')}
        sub={t('entrance.register.sub')}
      />
      {discord ? (
        <p className="ent-chip">
          {t('entrance.register.discordLinked', { username: discord.username })}
          <button
            type="button"
            aria-label={t('entrance.register.discordUnlink')}
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
          <DiscordButton href={discordStartUrl('register')}>
            {t('entrance.login.discord')}
          </DiscordButton>
          <Divider>{t('entrance.register.divider')}</Divider>
        </>
      )}
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{formError}</FormError>
        <TextInput
          label={t('entrance.register.email')}
          type="email"
          autoComplete="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          error={errors.email}
        />
        <TextInput
          label={t('entrance.register.username')}
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
          label={t('entrance.register.displayName')}
          autoComplete="nickname"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          error={errors.display_name}
        />
        <PasswordInput
          label={t('entrance.register.password')}
          showStrength
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          hint={t('entrance.register.passwordHint')}
          error={errors.password}
        />
        <PasswordInput
          label={t('entrance.register.confirm')}
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          error={errors.confirm}
        />
        <div>
          <Checkbox
            label={
              <>
                {t('entrance.register.rulesBefore')}{' '}
                <Link to="/house-notes">{t('entrance.register.rulesLink')}</Link>.
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
          {busy ? t('entrance.register.submitting') : t('entrance.register.submit')}
        </GoldButton>
        <p className="ent-footnote">{t('entrance.register.footnote')}</p>
      </form>
    </AuthSplit>
  );
}
