import { Button, Modal, PasswordInput, TextInput, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../../api/client';
import { changeUsername, errorText, patchSelf } from '../api';

export const USERNAME_RULE = /^[a-z0-9_.]{2,32}$/;
export const DISPLAY_NAME_MAX = 32;

/** First message for one field of a Discord-style error tree, else the generic text. */
function fieldText(e: unknown, field: string): string {
  if (e instanceof ApiError) {
    const m = e.body?.errors?.[field]?._errors?.[0]?.message;
    if (m) return m;
  }
  return errorText(e);
}

/**
 * Change-username dialog (LNG-USET-004). `PUT /sr/auth/username` re-checks the password, applies the signup
 * rules (2 to 32 of a-z 0-9 _ .) and allows two changes a day, so a refusal is shown on the field.
 */
export function ChangeUsernameDialog({
  open,
  current,
  onClose,
  onChanged,
}: {
  open: boolean;
  current: string;
  onClose: () => void;
  onChanged: (username: string) => void;
}) {
  const toast = useToast();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [errors, setErrors] = useState<{ username?: string; password?: string }>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!open) return;
    setUsername(current);
    setPassword('');
    setErrors({});
  }, [open, current]);

  const submit = async () => {
    const name = username.trim().toLowerCase();
    if (!USERNAME_RULE.test(name))
      return setErrors({ username: 'Use 2 to 32 letters, digits, underscores or dots.' });
    if (name === current) return setErrors({ username: 'That is already your username.' });
    if (!password) return setErrors({ password: 'Enter your password.' });
    setBusy(true);
    setErrors({});
    try {
      const res = await changeUsername(name, password);
      toast.show({ title: 'Username changed', duration: 3000 });
      onChanged(res.username ?? name);
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.body?.errors?.username)
        setErrors({ username: fieldText(e, 'username') });
      else setErrors({ password: errorText(e) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Change your username"
      description="Your username is what you sign in with. You can change it twice a day."
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            Done
          </Button>
        </>
      }
    >
      <form
        className="us-stack"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <TextInput
          label="Username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          error={errors.username}
          autoCapitalize="none"
          autoComplete="username"
          data-autofocus=""
        />
        <PasswordInput
          label="Current password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          error={errors.password}
          autoComplete="current-password"
        />
      </form>
    </Modal>
  );
}

/** Change-display-name dialog: `PATCH /users/@me {global_name}`; an empty name falls back to the username. */
export function DisplayNameDialog({
  open,
  current,
  onClose,
  onChanged,
}: {
  open: boolean;
  current: string;
  onClose: () => void;
  onChanged: (name: string | null) => void;
}) {
  const toast = useToast();
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!open) return;
    setName(current);
    setError(null);
  }, [open, current]);

  const submit = async () => {
    const clean = name.trim();
    if ([...clean].length > DISPLAY_NAME_MAX)
      return setError(`Display names can be at most ${DISPLAY_NAME_MAX} characters.`);
    setBusy(true);
    setError(null);
    try {
      await patchSelf({ global_name: clean === '' ? null : clean });
      toast.show({ title: 'Display name changed', duration: 3000 });
      onChanged(clean === '' ? null : clean);
      onClose();
    } catch (e) {
      setError(fieldText(e, 'global_name'));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Change your display name"
      description="This is the name people see. Leave it empty to show your username."
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            Done
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <TextInput
          label="Display name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          error={error}
          data-autofocus=""
        />
      </form>
    </Modal>
  );
}
