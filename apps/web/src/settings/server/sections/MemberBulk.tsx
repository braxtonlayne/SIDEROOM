// LNG-SSET-028: bulk actions over the selected members (add or remove a role, kick, ban). The server has no
// bulk endpoint, so each member is sent on its own, one after the other; the dialog reports which ones failed.
import { Button, Modal, TextArea } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import * as api from '../api';
import { ActionStatus, useAction } from '../ui';
import { DELETE_WINDOWS } from './Members';

export type BulkKind = 'addRole' | 'removeRole' | 'kick' | 'ban';

export interface BulkResult {
  kind: BulkKind;
  roleId?: string;
  ok: string[];
  failed: string[];
}

export function BulkDialog({
  guildId,
  kind,
  members,
  assignable,
  onClose,
  onDone,
}: {
  guildId: string;
  kind: BulkKind;
  /** The selected members this action may be applied to. */
  members: api.MemberObject[];
  /** Roles the viewer may hand out, highest first. */
  assignable: { id: string; name: string }[];
  onClose: () => void;
  onDone: (r: BulkResult) => void;
}) {
  const t = useT();
  const action = useAction();
  const [roleId, setRoleId] = useState(assignable[0]?.id ?? '');
  const [reason, setReason] = useState('');
  const [seconds, setSeconds] = useState(0);
  const isRole = kind === 'addRole' || kind === 'removeRole';

  const submit = async () => {
    const out = await action.run(async () => {
      const ok: string[] = [];
      const failed: string[] = [];
      for (const m of members) {
        try {
          if (kind === 'addRole') await api.addMemberRole(guildId, m.user.id, roleId);
          else if (kind === 'removeRole') await api.removeMemberRole(guildId, m.user.id, roleId);
          else if (kind === 'kick')
            await api.kickMember(guildId, m.user.id, reason.trim() || undefined);
          else
            await api.banMember(guildId, m.user.id, {
              ...(reason.trim() ? { reason: reason.trim() } : {}),
              delete_message_seconds: seconds,
            });
          ok.push(m.user.id);
        } catch {
          failed.push(m.user.id);
        }
      }
      return { ok, failed };
    });
    if (out) onDone({ kind, ...(isRole ? { roleId } : {}), ...out });
  };

  const verb =
    kind === 'addRole'
      ? t('settings.members.bulkAddRole')
      : kind === 'removeRole'
        ? t('settings.members.bulkRemoveRole')
        : kind === 'kick'
          ? t('settings.members.kick')
          : t('settings.members.ban');
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.members.bulkTitle', { action: verb, count: members.length })}
      description={t(
        kind === 'kick'
          ? 'settings.members.bulkKickDescription'
          : kind === 'ban'
            ? 'settings.members.bulkBanDescription'
            : 'settings.members.bulkRoleDescription',
        { count: members.length },
      )}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.members.cancel')}
          </Button>
          <Button
            variant={isRole ? 'primary' : 'danger'}
            loading={action.busy}
            disabled={(isRole && !roleId) || members.length === 0}
            onClick={() => void submit()}
          >
            {verb}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        {isRole ? (
          <div>
            <label className="ss-field-label" htmlFor="ss-bulk-role">
              {t('settings.members.bulkRole')}
            </label>
            <select
              id="ss-bulk-role"
              className="ss-select"
              value={roleId}
              onChange={(e) => setRoleId(e.target.value)}
            >
              {assignable.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
            </select>
          </div>
        ) : (
          <>
            {kind === 'ban' && (
              <div>
                <label className="ss-field-label" htmlFor="ss-bulk-delete">
                  {t('settings.members.deleteHistory')}
                </label>
                <select
                  id="ss-bulk-delete"
                  className="ss-select"
                  value={seconds}
                  onChange={(e) => setSeconds(Number(e.target.value))}
                >
                  {DELETE_WINDOWS.map((w) => (
                    <option key={w.seconds} value={w.seconds}>
                      {t(w.label)}
                    </option>
                  ))}
                </select>
              </div>
            )}
            <TextArea
              label={t('settings.members.reason')}
              value={reason}
              maxLength={512}
              onChange={(e) => setReason(e.target.value)}
            />
          </>
        )}
      </div>
    </Modal>
  );
}
