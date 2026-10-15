import { Checkbox, GoldButton, TextInput } from '@sideroom/ui';
import { Link, useRouter, useSearch } from '@tanstack/react-router';
import { useRef, useState, type ClipboardEvent, type FormEvent, type KeyboardEvent } from 'react';
import { ApiError } from '../../api/client';
import { submitTwoFactor } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { FormError } from '../../components/FormParts';
import { PasskeyButton } from '../../components/PasskeyButton';
import { clearMfaMethods, readMfaMethods } from '../../lib/mfaMethods';
import { useT } from '../../i18n/react';
import { minutesFromSeconds, safeNext } from '../../lib/validation';
import { useUiStore } from '../../state/session';

/** Local primitive: segmented 6-digit input with auto-advance, backspace-back and paste. */
export function OtpInput({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const t = useT();
  const refs = useRef<(HTMLInputElement | null)[]>([]);
  const focus = (i: number) => refs.current[Math.max(0, Math.min(5, i))]?.focus();
  const digits = Array.from({ length: 6 }, (_, i) => value[i] ?? '');

  const set = (i: number, d: string) => {
    const next = digits.slice();
    next[i] = d;
    onChange(next.join('').slice(0, 6));
  };
  const onKey = (i: number, e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Backspace' && !digits[i]) focus(i - 1);
    if (e.key === 'ArrowLeft') focus(i - 1);
    if (e.key === 'ArrowRight') focus(i + 1);
  };
  const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
    const pasted = e.clipboardData.getData('text').replace(/\D/g, '').slice(0, 6);
    if (!pasted) return;
    e.preventDefault();
    onChange(pasted);
    focus(pasted.length);
  };

  return (
    <div className="ent-otp" role="group" aria-label={t('entrance.twoFactor.otpGroup')}>
      {digits.map((d, i) => (
        <input
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          value={d}
          inputMode="numeric"
          autoComplete={i === 0 ? 'one-time-code' : 'off'}
          maxLength={1}
          disabled={disabled}
          aria-label={t('entrance.twoFactor.digit', { n: i + 1 })}
          onChange={(e) => {
            const c = e.target.value.replace(/\D/g, '').slice(-1);
            set(i, c);
            if (c) focus(i + 1);
          }}
          onKeyDown={(e) => onKey(i, e)}
          onPaste={onPaste}
          onFocus={(e) => e.target.select()}
        />
      ))}
    </div>
  );
}

export function TwoFactor() {
  const t = useT();
  const router = useRouter();
  const search = useSearch({ strict: false }) as { next?: string };
  const next = safeNext(search.next);
  // Q83: only the methods the server offered at the password step are shown.
  const [methods] = useState(readMfaMethods);
  const hasTotp = methods.includes('totp');
  const hasPasskey = methods.includes('passkey');
  const [mode, setMode] = useState<'totp' | 'recovery' | 'passkey'>(
    hasTotp ? 'totp' : hasPasskey ? 'passkey' : 'recovery',
  );
  const [code, setCode] = useState('');
  const [recovery, setRecovery] = useState('');
  const [trust, setTrust] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const finish = () => {
    clearMfaMethods();
    router.history.push(next ?? useUiStore.getState().lastSection);
  };

  const submit = async (method: 'totp' | 'recovery', value: string) => {
    setBusy(true);
    setError(null);
    try {
      await submitTwoFactor({ method, code: value, trust_device: trust });
      finish();
    } catch (e) {
      if (e instanceof ApiError && e.status === 429) {
        setError(t('entrance.twoFactor.tooMany', { minutes: minutesFromSeconds(e.retryAfter) }));
      } else if (e instanceof ApiError && e.status === 401) {
        setError(
          method === 'totp' ? t('entrance.twoFactor.badCode') : t('entrance.twoFactor.badRecovery'),
        );
        if (method === 'totp') setCode('');
      } else {
        setError(t('entrance.common.unreachable'));
      }
      setBusy(false);
    }
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (mode === 'totp') {
      if (code.length !== 6) return setError(t('entrance.twoFactor.needDigits'));
      void submit('totp', code);
    } else {
      if (!recovery.trim()) return setError(t('entrance.twoFactor.needRecovery'));
      void submit('recovery', recovery.trim());
    }
  };

  return (
    <AuthSplit
      eyebrow={t('entrance.twoFactor.eyebrow')}
      heading={t('entrance.twoFactor.heading')}
      accent={t('entrance.twoFactor.headingAccent')}
      copy={t('entrance.twoFactor.copy')}
      footLabel={t('entrance.twoFactor.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.twoFactor.formEyebrow')}
        title={
          mode === 'totp'
            ? t('entrance.twoFactor.titleTotp')
            : mode === 'passkey'
              ? t('entrance.twoFactor.titlePasskey')
              : t('entrance.twoFactor.titleRecovery')
        }
        sub={
          mode === 'totp'
            ? t('entrance.twoFactor.subTotp')
            : mode === 'passkey'
              ? t('entrance.twoFactor.subPasskey')
              : t('entrance.twoFactor.subRecovery')
        }
      />
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{error}</FormError>
        {mode === 'passkey' ? null : mode === 'totp' ? (
          <OtpInput
            value={code}
            disabled={busy}
            onChange={(v) => {
              setCode(v);
              if (v.length === 6) void submit('totp', v);
            }}
          />
        ) : (
          <TextInput
            label={t('entrance.twoFactor.recoveryLabel')}
            value={recovery}
            onChange={(e) => setRecovery(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        )}
        <Checkbox
          label={t('entrance.twoFactor.trust')}
          checked={trust}
          onChange={(e) => setTrust(e.target.checked)}
        />
        {mode !== 'passkey' && (
          <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
            {busy ? t('entrance.twoFactor.checking') : t('entrance.twoFactor.submit')}
          </GoldButton>
        )}
        {hasPasskey && (
          <PasskeyButton
            secondFactor={{ trustDevice: trust }}
            label={
              mode === 'passkey'
                ? t('entrance.twoFactor.usePasskey')
                : t('entrance.twoFactor.usePasskeyInstead')
            }
            onDone={finish}
            onError={setError}
          />
        )}
        {(['totp', 'recovery'] as const)
          .filter((m) => m !== mode && methods.includes(m))
          .map((m) => (
            <button
              key={m}
              type="button"
              className="ent-linkbtn"
              onClick={() => {
                setMode(m);
                setError(null);
              }}
            >
              {m === 'totp' ? t('entrance.twoFactor.useTotp') : t('entrance.twoFactor.useRecovery')}
            </button>
          ))}
        <Link to="/login" className="ent-linkbtn">
          {t('entrance.common.backToSignIn')}
        </Link>
      </form>
    </AuthSplit>
  );
}
