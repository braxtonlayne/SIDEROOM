import { Avatar, Button, Checkbox, EmptyState, Modal, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeGoto } from '../nav';
import { useCurrentUser, useDms, usePresenceMap } from '../data';
import { addRecipient } from './api';
import { rememberDm, useOpenDm } from './dm';
import { GroupInviteLink } from './GroupInviteLink';
import { friendsOf, useSocial, wireName } from './social';

/** A group DM holds you plus up to nine others. */
export const MAX_GROUP_RECIPIENTS = 9;

/** Create DM / Group DM picker (LNG-HOME-005): search friends, tick up to nine, create. */
export function CreateDmDialog({
  onClose,
  addTo,
}: {
  onClose: () => void;
  /** Add people to this DM instead of creating one (LNG-DM-004): a 1:1 DM turns into a new group. */
  addTo?: string | undefined;
}) {
  const t = useT();
  const rows = useSocial((s) => s.relationships);
  const status = useSocial((s) => s.status);
  const load = useSocial((s) => s.loadRelationships);
  const presence = usePresenceMap();
  const openDm = useOpenDm();
  const goto = useLoungeGoto();
  const me = useCurrentUser();
  const target = useDms().find((d) => d.id === addTo);
  const existing = target?.recipientIds ?? [];
  // The owner of an existing group can copy its invite link from here too (RDY-PAR-001).
  const canInvite = !!target && existing.length > 1 && target.ownerId === me.id;
  /** The group that was just created: the dialog then offers its invite link before closing. */
  const [created, setCreated] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const friends = friendsOf(rows).filter((r) => !existing.includes(r.id));
  const q = query.trim().toLowerCase();
  const shown = friends.filter(
    (r) =>
      !q || wireName(r.user).toLowerCase().includes(q) || r.user.username.toLowerCase().includes(q),
  );
  const full = picked.length + existing.length >= MAX_GROUP_RECIPIENTS;

  const toggle = (id: string, on: boolean) =>
    setPicked((p) => (on ? (p.includes(id) ? p : [...p, id]) : p.filter((x) => x !== id)));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!picked.length) return;
    setBusy(true);
    setError(null);
    try {
      if (addTo) {
        let target = addTo;
        for (const id of picked) {
          // Adding to a 1:1 DM answers with the new group; later people join that group.
          const made = await addRecipient(target, id);
          if (made) {
            rememberDm(made);
            target = made.id;
          }
        }
        if (target !== addTo) goto(HOME, target);
      } else {
        const dm = await openDm(picked);
        if (dm.type === 3 || dm.recipients.length > 1) {
          setCreated(dm.id);
          setBusy(false);
          return;
        }
      }
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError && err.message ? err.message : t('lounge.createDm.startError'),
      );
      setBusy(false);
    }
  };

  if (created)
    return (
      <Modal open onClose={onClose} size="sm" title={t('lounge.createDm.createdTitle')}>
        <div className="lng-form">
          <GroupInviteLink channelId={created} />
          <div className="lng-form-actions">
            <Button onClick={onClose}>{t('lounge.createDm.done')}</Button>
          </div>
        </div>
      </Modal>
    );

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={addTo ? t('lounge.createDm.addTitle') : t('lounge.createDm.title')}
      description={t('lounge.createDm.canAdd', {
        n: MAX_GROUP_RECIPIENTS - existing.length - picked.length,
      })}
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        <TextInput
          label={t('lounge.createDm.searchLabel')}
          value={query}
          placeholder={t('lounge.createDm.searchPlaceholder')}
          autoFocus
          onChange={(e) => setQuery(e.target.value)}
        />
        {status === 'loading' && friends.length === 0 && (
          <p role="status">
            <Spinner label={t('lounge.createDm.loadingFriends')} />
          </p>
        )}
        {status === 'error' && (
          <p role="alert" className="lng-form-error">
            {t('lounge.createDm.loadError')}
          </p>
        )}
        {status === 'ready' && friends.length === 0 && (
          <EmptyState
            title={t('lounge.createDm.noFriendsTitle')}
            description={t('lounge.createDm.noFriendsBody')}
          />
        )}
        {friends.length > 0 && shown.length === 0 && (
          <p className="lng-muted">{t('lounge.createDm.noMatch')}</p>
        )}
        <ul className="lng-pick-list" aria-label={t('lounge.createDm.friendsList')}>
          {shown.map((r) => {
            const on = picked.includes(r.id);
            return (
              <li key={r.id} className="lng-pick-row">
                <Avatar
                  name={wireName(r.user)}
                  size={32}
                  status={presence[r.id]?.status ?? 'offline'}
                />
                <Checkbox
                  label={wireName(r.user)}
                  description={r.user.username}
                  checked={on}
                  disabled={!on && full}
                  onChange={(e) => toggle(r.id, e.target.checked)}
                />
              </li>
            );
          })}
        </ul>
        {canInvite && addTo && <GroupInviteLink channelId={addTo} />}
        {error && (
          <p role="alert" className="lng-form-error">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.createDm.cancel')}
          </Button>
          <Button type="submit" loading={busy} disabled={!picked.length}>
            {addTo
              ? t('lounge.createDm.add')
              : picked.length > 1
                ? t('lounge.createDm.createGroup')
                : t('lounge.createDm.create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
