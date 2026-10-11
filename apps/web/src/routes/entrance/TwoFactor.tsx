import { Checkbox, GoldButton, TextInput } from '@sideroom/ui';
import { Link, useRouter, useSearch } from '@tanstack/react-router';
import { useRef, useState, type ClipboardEvent, type FormEvent, type KeyboardEvent } from 'react';
import { ApiError } from '../../api/client';
import { submitTwoFactor } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { FormError } from '../../components/FormParts';
import { PasskeyButton } from '../../components/PasskeyButton';
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
    <div className="ent-otp" role="group" aria-label="Six-digit code">
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
          aria-label={`Digit ${i + 1}`}
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
  const router = useRouter();
  const search = useSearch({ strict: false }) as { next?: string };
  const next = safeNext(search.next);
  const [mode, setMode] = useState<'totp' | 'recovery'>('totp');
  const [code, setCode] = useState('');
  const [recovery, setRecovery] = useState('');
  const [trust, setTrust] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const finish = () => router.history.push(next ?? useUiStore.getState().lastSection);

  const submit = async (method: 'totp' | 'recovery', value: string) => {
    setBusy(true);
    setError(null);
    try {
      await submitTwoFactor({ method, code: value, trust_device: trust });
      finish();
    } catch (e) {
      if (e instanceof ApiError && e.status === 429) {
        const m = minutesFromSeconds(e.retryAfter);
        setError(`Too many attempts. Try again in ${m} minute${m === 1 ? '' : 's'}.`);
      } else if (e instanceof ApiError && e.status === 401) {
        setError(
          method === 'totp'
            ? "That code didn't match. Check your authenticator and try again."
            : "That recovery code isn't valid.",
        );
        if (method === 'totp') setCode('');
      } else {
        setError("We couldn't reach the door. Check your connection and try again.");
      }
      setBusy(false);
    }
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (mode === 'totp') {
      if (code.length !== 6) return setError('Enter all six digits.');
      void submit('totp', code);
    } else {
      if (!recovery.trim()) return setError('Enter a recovery code.');
      void submit('recovery', recovery.trim());
    }
  };

  return (
    <AuthSplit
      eyebrow="One more lock."
      heading="Just to be"
      accent="sure."
      copy="Confirm it's really you, then you're in."
      footLabel="Member entrance / 02"
    >
      <FormHead
        eyebrow="Two-step check"
        title={mode === 'totp' ? 'Your code.' : 'Recovery code.'}
        sub={
          mode === 'totp'
            ? 'Enter the six-digit code from your authenticator app.'
            : 'Enter one of the recovery codes you saved. Each works once.'
        }
      />
      <form className="ent-form" onSubmit={onSubmit} noValidate>
        <FormError>{error}</FormError>
        {mode === 'totp' ? (
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
            label="Recovery code"
            value={recovery}
            onChange={(e) => setRecovery(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        )}
        <Checkbox
          label="Trust this device for 30 days"
          checked={trust}
          onChange={(e) => setTrust(e.target.checked)}
        />
        <GoldButton type="submit" arrow disabled={busy} aria-busy={busy}>
          {busy ? 'Checking…' : 'Step inside'}
        </GoldButton>
        {mode === 'totp' && <PasskeyButton onDone={finish} onError={setError} />}
        <button
          type="button"
          className="ent-linkbtn"
          onClick={() => {
            setMode(mode === 'totp' ? 'recovery' : 'totp');
            setError(null);
          }}
        >
          {mode === 'totp' ? 'Use a recovery code' : 'Use an authenticator code'}
        </button>
        <Link to="/login" className="ent-linkbtn">
          Back to sign in
        </Link>
      </form>
    </AuthSplit>
  );
}
