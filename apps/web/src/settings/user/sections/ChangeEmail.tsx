import { Button, Modal, PasswordInput, TextInput, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../../api/client';
import { useT } from '../../../i18n';
import { changeEmail, errorText } from '../api';

type Step = 'details' | 'code';

/** Error text for one field of a Discord-style error tree, with its machine code. */
function fieldIssue(e: unknown, field: string): { code?: string; message: string } | null {
  if (!(e instanceof ApiError)) return null;
  const first = e.body?.errors?.[field]?._errors?.[0];
  return first ? { code: first.code, message: first.message } : null;
}

/**
 * Change-email dialog (D-167). Step one takes the new address and the current password; when the account
 * has two-factor authentication on, step two asks for an authenticator or recovery code (a code the server
 * accepts from this session in the last 15 minutes is not asked for again, so the server can also ask for it
 * after step one). The address does not change until the emailed link is used.
 */
export function ChangeEmailDialog({
  open,
  mfaEnabled,
  currentEmail,
  onClose,
}: {
  open: boolean;
  mfaEnabled: boolean;
  currentEmail: string;
  onClose: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [step, setStep] = useState<Step>('details');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [code, setCode] = useState('');
  const [errors, setErrors] = useState<{ email?: string; password?: string; code?: string }>({});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setStep('details');
    setEmail('');
    setPassword('');
    setCode('');
    setErrors({});
  }, [open]);

  const send = async (withCode: boolean) => {
    const clean = code.replace(/\s+/g, '');
    setBusy(true);
    setErrors({});
    try {
      await changeEmail({
        new_email: email.trim(),
        password,
        ...(withCode && clean
          ? {
              mfa: {
                method: /^\d{6}$/.test(clean) ? ('totp' as const) : ('recovery' as const),
                code: clean,
              },
            }
          : {}),
      });
      onClose();
      toast.show({
        title: t('settings.changeEmail.sentTitle'),
        description: t('settings.changeEmail.sentDesc', {
          sent: email.trim(),
          current: currentEmail,
        }),
        duration: 7000,
      });
    } catch (e) {
      const mfa = fieldIssue(e, 'mfa');
      const addr = fieldIssue(e, 'new_email');
      if (mfa) {
        setStep('code');
        setErrors({ code: mfa.code === 'required' && !withCode ? undefined : mfa.message });
      } else if (addr) {
        setStep('details');
        setErrors({ email: addr.message });
      } else {
        setStep('details');
        setErrors({ password: fieldIssue(e, 'password')?.message ?? errorText(e) });
      }
    } finally {
      setBusy(false);
    }
  };

  const next = () => {
    const nextErrors: typeof errors = {};
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()))
      nextErrors.email = t('settings.changeEmail.validEmail');
    if (!password) nextErrors.password = t('settings.changeEmail.enterPassword');
    setErrors(nextErrors);
    if (nextErrors.email || nextErrors.password) return;
    if (mfaEnabled) {
      setStep('code');
      return;
    }
    void send(false);
  };

  const confirmCode = () => {
    if (!code.trim()) {
      setErrors({ code: t('settings.changeEmail.enterCode') });
      return;
    }
    void send(true);
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      size="sm"
      title={
        step === 'code' ? t('settings.changeEmail.confirmTitle') : t('settings.changeEmail.title')
      }
      description={
        step === 'code' ? t('settings.changeEmail.codeDesc') : t('settings.changeEmail.detailsDesc')
      }
    >
      {step === 'details' ? (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            next();
          }}
          noValidate
        >
          <TextInput
            label={t('settings.changeEmail.newEmail')}
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            error={errors.email}
            data-autofocus=""
          />
          <PasswordInput
            label={t('settings.changeEmail.currentPassword')}
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            error={errors.password}
          />
          <div className="us-actions">
            <Button variant="ghost" onClick={onClose}>
              {t('settings.changeEmail.cancel')}
            </Button>
            <Button type="submit" loading={busy && !mfaEnabled}>
              {mfaEnabled ? t('settings.changeEmail.continue') : t('settings.changeEmail.sendLink')}
            </Button>
          </div>
        </form>
      ) : (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            confirmCode();
          }}
          noValidate
        >
          <TextInput
            label={t('settings.changeEmail.codeLabel')}
            inputMode="text"
            autoComplete="one-time-code"
            maxLength={64}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            error={errors.code}
            data-autofocus=""
          />
          <div className="us-actions">
            <Button variant="ghost" onClick={() => setStep('details')}>
              {t('settings.changeEmail.back')}
            </Button>
            <Button type="submit" loading={busy}>
              {t('settings.changeEmail.sendLink')}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}
