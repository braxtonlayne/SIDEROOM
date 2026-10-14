import { Button, Icon, Modal, Notice, PasswordInput, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { errorText, isWrongPassword } from './api';

export function Loading({ what }: { what: string }) {
  return (
    <p className="dev-muted" role="status">
      <Spinner size="sm" label={`Loading ${what}`} /> Loading {what}...
    </p>
  );
}

export function LoadError({
  what,
  error,
  onRetry,
}: {
  what: string;
  error: unknown;
  onRetry?: () => void;
}) {
  return (
    <Notice variant="danger" title={`Couldn't load ${what}`}>
      {errorText(error)}{' '}
      {onRetry && (
        <Button variant="link" size="sm" onClick={onRetry}>
          Try again
        </Button>
      )}
    </Notice>
  );
}

/** Honest placeholder where the server has no endpoint yet. */
export function NotAvailable({ children }: { children: ReactNode }) {
  return (
    <Notice variant="info" title="Not available yet">
      {children}
    </Notice>
  );
}

/** Copy `text` to the clipboard and say so (polite live region + toast-free inline label). */
export function CopyButton({
  text,
  label = 'Copy',
  size = 'sm',
}: {
  text: string;
  label?: string;
  size?: 'sm' | 'md';
}) {
  const [done, setDone] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <Button
      variant="secondary"
      size={size}
      leadingIcon={<Icon name="copy" size={16} />}
      onClick={() => {
        void navigator.clipboard?.writeText(text).then(
          () => {
            setDone(true);
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => setDone(false), 2000);
          },
          () => undefined,
        );
      }}
    >
      {done ? 'Copied' : label}
    </Button>
  );
}

/** A code sample with a copy button. `lang` is a caption only (no highlighter is loaded). */
export function CodeBlock({ code, lang, title }: { code: string; lang: string; title?: string }) {
  return (
    <figure className="dev-code">
      <figcaption>
        <span>{title ?? lang}</span>
        <CopyButton text={code} label={`Copy ${title ?? lang}`} />
      </figcaption>
      <pre tabIndex={0}>
        <code>{code}</code>
      </pre>
    </figure>
  );
}

export interface Secret {
  label: string;
  value: string;
}

/**
 * Shows secrets exactly once. They live only in the caller's state while the dialog is open; closing the
 * dialog calls `onClose`, which must drop them. They are never written to storage or logged.
 */
export function SecretDialog({
  title,
  description,
  secrets,
  onClose,
}: {
  title: string;
  description: string;
  secrets: Secret[] | null;
  onClose: () => void;
}) {
  return (
    <Modal
      open={secrets !== null}
      onClose={onClose}
      title={title}
      description={description}
      size="md"
      closeOnBackdrop={false}
      footer={<Button onClick={onClose}>Done</Button>}
    >
      {(secrets ?? []).map((s, i) => (
        <div className="dev-secret" key={s.label}>
          <label className="dev-label" htmlFor={`dev-secret-${i}`}>
            {s.label}
          </label>
          <input
            id={`dev-secret-${i}`}
            className="dev-secret-input"
            readOnly
            value={s.value}
            onFocus={(e) => e.currentTarget.select()}
            autoComplete="off"
            spellCheck={false}
          />
          <CopyButton text={s.value} label={`Copy ${s.label.toLowerCase()}`} size="md" />
        </div>
      ))}
      <Notice variant="warning" title="You will not see this again">
        Copy it somewhere safe now. Once you close this dialog only a reset can give you a new one.
      </Notice>
    </Modal>
  );
}

/**
 * Destructive action that re-confirms the account password (the server asks for it on secret and token
 * resets and on delete). `onSubmit` resolves on success; a rejection is shown inside the dialog.
 */
export function PasswordConfirm({
  open,
  title,
  description,
  confirmLabel,
  danger = true,
  requireText,
  onSubmit,
  onCancel,
}: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  danger?: boolean;
  /** Also require this exact text to be typed (deleting an application). */
  requireText?: string;
  onSubmit: (password: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [password, setPassword] = useState('');
  const [typed, setTyped] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) {
      setPassword('');
      setTyped('');
      setError(null);
    }
  }, [open]);
  const ready = password !== '' && (requireText === undefined || typed === requireText);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await onSubmit(password);
    } catch (e) {
      setError(
        isWrongPassword(e) ? 'That password is not right.' : errorText(e, 'That did not work.'),
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      description={description}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant={danger ? 'danger' : 'primary'}
            disabled={!ready}
            loading={busy}
            onClick={() => void submit()}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <form
        className="dev-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !busy) void submit();
        }}
      >
        {requireText !== undefined && (
          <TextInput
            label={`Type "${requireText}" to confirm`}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            autoComplete="off"
          />
        )}
        <PasswordInput
          label="Your password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
        />
        {error && (
          <p className="dev-error" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
