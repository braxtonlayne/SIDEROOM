import { Button, Modal, Notice, PasswordInput, TextInput, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import {
  deletePasskey,
  listPasskeys,
  passkeyRegisterBegin,
  passkeyRegisterFinish,
  renamePasskey,
  type Passkey,
} from '../../../api/passkeys';
import { createPasskey, isCancelled, passkeysSupported } from '../../../lib/webauthn';
import { errorText, useResource } from '../api';
import { Card, fmtDate, Loaded } from '../parts';
import { PasswordPrompt, RecoveryCodes } from './Account';

/** Password plus a name, then the browser's own passkey prompt (ENT-AUTH-010, LNG-USET-005). */
function AddPasskey({
  open,
  onClose,
  onAdded,
}: {
  open: boolean;
  onClose: () => void;
  onAdded: (p: Passkey, recoveryCodes: string[] | null) => void;
}) {
  const [name, setName] = useState('Passkey');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setName('Passkey');
      setPassword('');
      setError(null);
    }
  }, [open]);
  const submit = async () => {
    if (!name.trim()) return setError('Give the passkey a name.');
    if (!password) return setError('Enter your password.');
    setBusy(true);
    setError(null);
    try {
      const { ceremony_id, options } = await passkeyRegisterBegin(password);
      const credential = await createPasskey(options);
      const res = await passkeyRegisterFinish({ ceremony_id, name: name.trim(), credential });
      onAdded(res.passkey, res.recovery_codes);
    } catch (e) {
      setError(isCancelled(e) ? 'The passkey prompt was cancelled. Try again.' : errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Add a passkey"
      description="Confirm your password, then follow your browser's prompt."
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            Add passkey
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
          label="Name"
          value={name}
          maxLength={64}
          onChange={(e) => setName(e.target.value)}
          data-autofocus=""
        />
        <PasswordInput
          label="Current password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          error={error}
          autoComplete="current-password"
        />
      </form>
    </Modal>
  );
}

function RenamePasskey({
  passkey,
  onClose,
  onRenamed,
}: {
  passkey: Passkey | null;
  onClose: () => void;
  onRenamed: (p: Passkey) => void;
}) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setName(passkey?.name ?? '');
    setError(null);
  }, [passkey]);
  const submit = async () => {
    if (!passkey) return;
    if (!name.trim()) return setError('Give the passkey a name.');
    setBusy(true);
    setError(null);
    try {
      onRenamed(await renamePasskey(passkey.id, name.trim()));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={passkey !== null}
      onClose={onClose}
      title="Rename passkey"
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            Save
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
          label="Name"
          value={name}
          maxLength={64}
          error={error}
          onChange={(e) => setName(e.target.value)}
          data-autofocus=""
        />
      </form>
    </Modal>
  );
}

/** Passkeys list: add, rename, remove. Adding and removing re-check the password. */
export function PasskeysCard() {
  const toast = useToast();
  const res = useResource(listPasskeys);
  const supported = passkeysSupported();
  const [adding, setAdding] = useState(false);
  const [renaming, setRenaming] = useState<Passkey | null>(null);
  const [removing, setRemoving] = useState<Passkey | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);
  return (
    <Card
      id="us-passkeys"
      title="Passkeys"
      description="Sign in with your fingerprint, face or a security key instead of a password. A passkey also counts as your second step."
      actions={
        <Button disabled={!supported} onClick={() => setAdding(true)}>
          Add a passkey
        </Button>
      }
    >
      {!supported && (
        <Notice variant="info" title="Not supported here">
          This browser can't create passkeys. You can still manage the ones you have.
        </Notice>
      )}
      <Loaded res={res} what="passkeys">
        {(rows) =>
          rows.length === 0 ? (
            <p className="us-muted">No passkeys yet.</p>
          ) : (
            <ul className="us-list" aria-label="Passkeys">
              {rows.map((p) => (
                <li key={p.id}>
                  <div className="us-row-main">
                    <strong>{p.name}</strong>
                    <p className="us-muted">
                      Added {fmtDate(p.created_at)},{' '}
                      {p.last_used_at ? `last used ${fmtDate(p.last_used_at)}` : 'never used'}
                      {p.backup_eligible ? ', synced' : ''}
                    </p>
                  </div>
                  <div className="us-actions">
                    <Button variant="secondary" size="sm" onClick={() => setRenaming(p)}>
                      Rename<span className="us-sr"> {p.name}</span>
                    </Button>
                    <Button variant="danger" size="sm" onClick={() => setRemoving(p)}>
                      Remove<span className="us-sr"> {p.name}</span>
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      <AddPasskey
        open={adding}
        onClose={() => setAdding(false)}
        onAdded={(p, recovery) => {
          setAdding(false);
          res.setData((d) => [...(d ?? []), p]);
          toast.show({ title: 'Passkey added', duration: 3000 });
          if (recovery) setCodes(recovery);
        }}
      />
      <RenamePasskey
        passkey={renaming}
        onClose={() => setRenaming(null)}
        onRenamed={(p) => {
          setRenaming(null);
          res.setData((d) => d?.map((x) => (x.id === p.id ? p : x)));
        }}
      />
      <PasswordPrompt
        open={removing !== null}
        danger
        title="Remove passkey"
        description={`Remove "${removing?.name ?? ''}"? Enter your password to confirm.`}
        confirmLabel="Remove"
        onClose={() => setRemoving(null)}
        onSubmit={async (pw) => {
          if (!removing) return;
          await deletePasskey(removing.id, pw);
          res.setData((d) => d?.filter((x) => x.id !== removing.id));
          setRemoving(null);
          toast.show({ title: 'Passkey removed', duration: 3000 });
        }}
      />
      <Modal
        open={codes !== null}
        onClose={() => setCodes(null)}
        title="Your recovery codes"
        size="md"
        closeOnBackdrop={false}
      >
        {codes && <RecoveryCodes codes={codes} onDone={() => setCodes(null)} />}
      </Modal>
    </Card>
  );
}
