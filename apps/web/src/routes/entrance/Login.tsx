import { GoldButton, PasswordInput, TextInput } from '@sideroom/ui';
import { Link, useRouter, useSearch } from '@tanstack/react-router';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { discordStartUrl, login } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { Divider, DiscordButton, FormError } from '../../components/FormParts';
import { BanAppeal, isBannedError } from './BanAppeal';
import { LockIcon } from '../../components/icons';
import { PasskeyButton } from '../../components/PasskeyButton';
import { t as tr } from '../../i18n/runtime';
import { useT } from '../../i18n/react';
import { rememberMfaMethods } from '../../lib/mfaMethods';
import { minutesFromSeconds, safeNext } from '../../lib/validation';
import { useRedirectIfLoggedIn } from '../../lib/useRedirectIfLoggedIn';
import { useUiStore } from '../../state/session';

export function loginErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 429) {
      const m = minutesFromSeconds(e.retryAfter);
      return tr('entrance.login.tooMany', { minutes: m });
    }
    if (e.status === 401 || e.status === 400) return tr('entrance.login.mismatch');
  }
  return tr('entrance.login.unreachable');
}

export function Login() {
  useRedirectIfLoggedIn();
  const t = useT();
  const router = useRouter();
  const search = useSearch({ strict: false }) as { next?: string; error?: string };
  const next = safeNext(search.next);
  const [id, setId] = useState('');
  const [pw, setPw] = useState('');
  const [touched, setTouched] = useState(false);
  const [banned, setBanned] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(
    search.error === 'discord_unlinked' ? t('entrance.login.discordUnlinked') : null,
  );

  const finish = () => router.history.push(next ?? useUiStore.getState().lastSection);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (!id.trim() || !pw) return;
    setBusy(true);
    setError(null);
    setBanned(false);
    try {
      const res = await login({ login: id.trim(), password: pw });
      if (res.status === 'mfa_required') {
        rememberMfaMethods(res.methods);
        void router.navigate({ to: '/login/2fa', search: next ? { next } : {} });
      } else finish();
    } catch (err) {
      if (isBannedError(err)) {
        setBanned(true);
        setError(t('entrance.appeal.banned'));
      } else setError(loginErrorMessage(err));
      setBusy(false);
    }
  };

  return (
    <AuthSplit
      eyebrow={t('entrance.login.eyebrow')}
      heading={t('entrance.login.heading')}
      accent={t('entrance.login.headingAccent')}
      copy={
        <>
          {t('entrance.login.copy1')}
          <br />
          {t('entrance.login.copy2')}
        </>
      }
      footLabel={t('entrance.login.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.login.formEyebrow')}
        title={t('entrance.login.title')}
        sub={t('entrance.login.sub')}
      />
      <DiscordButton href={discordStartUrl('login', next ?? undefined)}>
        {t('entrance.login.discord')}
      </DiscordButton>
      <Divider>{t('entrance.login.divider')}</Divider>
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{error}</FormError>
        {banned && <BanAppeal login={id.trim()} password={pw} />}
        <TextInput
          label={t('entrance.login.id')}
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder={t('entrance.login.idPlaceholder')}
          autoComplete="username webauthn"
          error={touched && !id.trim() ? t('entrance.login.idRequired') : undefined}
        />
        <div>
          <div className="ent-row">
            <span />
            <Link to="/forgot">{t('entrance.login.forgot')}</Link>
          </div>
          <PasswordInput
            label={t('entrance.login.password')}
            value={pw}
            onChange={(e) => setPw(e.target.value)}
            placeholder={t('entrance.login.passwordPlaceholder')}
            autoComplete="current-password"
            error={touched && !pw ? t('entrance.login.passwordRequired') : undefined}
          />
        </div>
        <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
          {busy ? t('entrance.login.submitting') : t('entrance.login.submit')}
        </GoldButton>
        <PasskeyButton onDone={finish} onError={setError} />
      </form>
      <Link to="/" className="ent-newhere">
        <LockIcon />
        <span>
          {t('entrance.login.newHere')}
          <small>{t('entrance.login.newHereSub')}</small>
        </span>
        <span aria-hidden="true">↗</span>
      </Link>
    </AuthSplit>
  );
}
