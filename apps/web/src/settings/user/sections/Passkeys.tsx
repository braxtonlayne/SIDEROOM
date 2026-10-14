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
import { useT } from '../../../i18n';
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
  const t = useT();
  const [name, setName] = useState(() => t('settings.passkeys.defaultName'));
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setName(t('settings.passkeys.defaultName'));
      setPassword('');
      setError(null);
    }
  }, [open, t]);
  const submit = async () => {
    if (!name.trim()) return setError(t('settings.passkeys.needName'));
    if (!password) return setError(t('settings.passkeys.needPassword'));
    setBusy(true);
    setError(null);
    try {
      const { ceremony_id, options } = await passkeyRegisterBegin(password);
      const credential = await createPasskey(options);
      const res = await passkeyRegisterFinish({ ceremony_id, name: name.trim(), credential });
      onAdded(res.passkey, res.recovery_codes);
    } catch (e) {
      setError(isCancelled(e) ? t('settings.passkeys.cancelled') : errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('settings.passkeys.addTitle')}
      description={t('settings.passkeys.addDesc')}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.passkeys.cancel')}
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            {t('settings.passkeys.addConfirm')}
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
          label={t('settings.passkeys.name')}
          value={name}
          maxLength={64}
          onChange={(e) => setName(e.target.value)}
          data-autofocus=""
        />
        <PasswordInput
          label={t('settings.passkeys.currentPassword')}
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
  const t = useT();
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setName(passkey?.name ?? '');
    setError(null);
  }, [passkey]);
  const submit = async () => {
    if (!passkey) return;
    if (!name.trim()) return setError(t('settings.passkeys.needName'));
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
      title={t('settings.passkeys.renameTitle')}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.passkeys.cancel')}
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            {t('settings.passkeys.save')}
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
          label={t('settings.passkeys.name')}
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
  const t = useT();
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
      title={t('settings.passkeys.title')}
      description={t('settings.passkeys.desc')}
      actions={
        <Button disabled={!supported} onClick={() => setAdding(true)}>
          {t('settings.passkeys.add')}
        </Button>
      }
    >
      {!supported && (
        <Notice variant="info" title={t('settings.passkeys.unsupportedTitle')}>
          {t('settings.passkeys.unsupportedBody')}
        </Notice>
      )}
      <Loaded res={res} what={t('settings.passkeys.what')}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="us-muted">{t('settings.passkeys.empty')}</p>
          ) : (
            <ul className="us-list" aria-label={t('settings.passkeys.title')}>
              {rows.map((p) => (
                <li key={p.id}>
                  <div className="us-row-main">
                    <strong>{p.name}</strong>
                    <p className="us-muted">
                      {t(
                        p.backup_eligible
                          ? 'settings.passkeys.metaSynced'
                          : 'settings.passkeys.meta',
                        {
                          created: fmtDate(p.created_at),
                          usage: p.last_used_at
                            ? t('settings.passkeys.lastUsed', { date: fmtDate(p.last_used_at) })
                            : t('settings.passkeys.neverUsed'),
                        },
                      )}
                    </p>
                  </div>
                  <div className="us-actions">
                    <Button variant="secondary" size="sm" onClick={() => setRenaming(p)}>
                      {t('settings.passkeys.rename')}
                      <span className="us-sr"> {p.name}</span>
                    </Button>
                    <Button variant="danger" size="sm" onClick={() => setRemoving(p)}>
                      {t('settings.passkeys.remove')}
                      <span className="us-sr"> {p.name}</span>
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
          toast.show({ title: t('settings.passkeys.added'), duration: 3000 });
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
        title={t('settings.passkeys.removeTitle')}
        description={t('settings.passkeys.removeDesc', { name: removing?.name ?? '' })}
        confirmLabel={t('settings.passkeys.remove')}
        onClose={() => setRemoving(null)}
        onSubmit={async (pw) => {
          if (!removing) return;
          await deletePasskey(removing.id, pw);
          res.setData((d) => d?.filter((x) => x.id !== removing.id));
          setRemoving(null);
          toast.show({ title: t('settings.passkeys.removed'), duration: 3000 });
        }}
      />
      <Modal
        open={codes !== null}
        onClose={() => setCodes(null)}
        title={t('settings.passkeys.codesTitle')}
        size="md"
        closeOnBackdrop={false}
      >
        {codes && <RecoveryCodes codes={codes} onDone={() => setCodes(null)} />}
      </Modal>
    </Card>
  );
}
