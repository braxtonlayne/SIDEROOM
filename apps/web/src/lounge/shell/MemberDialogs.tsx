import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { t, useT, type MessageKey } from '../../i18n';
import { banMember, kickMember } from '../../settings/server/api';
import { useStore } from '../../store';
import { useGuild, useMembers } from '../data';

/** Timeout lengths offered in the timeout dialog (seconds). */
export const TIMEOUT_OPTIONS: { labelKey: MessageKey; seconds: number }[] = [
  { labelKey: 'lounge.memberDialogs.timeout60s', seconds: 60 },
  { labelKey: 'lounge.memberDialogs.timeout5m', seconds: 300 },
  { labelKey: 'lounge.memberDialogs.timeout10m', seconds: 600 },
  { labelKey: 'lounge.memberDialogs.timeout1h', seconds: 3600 },
  { labelKey: 'lounge.memberDialogs.timeout1d', seconds: 86400 },
  { labelKey: 'lounge.memberDialogs.timeout1w', seconds: 604800 },
];

/** Message history removed along with a ban (seconds). */
export const BAN_HISTORY_OPTIONS: { labelKey: MessageKey; seconds: number }[] = [
  { labelKey: 'lounge.memberDialogs.historyNone', seconds: 0 },
  { labelKey: 'lounge.memberDialogs.historyHour', seconds: 3600 },
  { labelKey: 'lounge.memberDialogs.history24h', seconds: 86400 },
  { labelKey: 'lounge.memberDialogs.history7d', seconds: 604800 },
];

export type MemberAction = 'timeout' | 'kick' | 'ban';

const errorText = (e: unknown, fallback: string) =>
  e instanceof ApiError && e.status === 403
    ? t('lounge.memberDialogs.forbidden')
    : e instanceof ApiError && e.message
      ? e.message
      : fallback;

function timeoutRequest(guildId: string, userId: string, until: string | null, reason: string) {
  return apiFetch<unknown>(`/guilds/${guildId}/members/${userId}`, {
    method: 'PATCH',
    json: { communication_disabled_until: until },
    retries: 0,
    ...(reason ? { headers: { 'X-Audit-Log-Reason': encodeURIComponent(reason) } } : {}),
  });
}

/** Timeout, Kick and Ban modals for a member (LNG-GLD-024). The server enforces hierarchy and permissions. */
export function MemberActionDialog({
  action,
  guildId,
  userId,
  onClose,
}: {
  action: MemberAction;
  guildId: string;
  userId: string;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const member = useMembers(guildId).find((m) => m.user.id === userId);
  const name = member?.nick || member?.user.displayName || t('lounge.memberDialogs.thisMember');
  const [reason, setReason] = useState('');
  const [seconds, setSeconds] = useState(action === 'timeout' ? 3600 : 0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!guild) return null;

  const title = {
    timeout: t('lounge.memberDialogs.titleTimeout', { name }),
    kick: t('lounge.memberDialogs.titleKick', { name }),
    ban: t('lounge.memberDialogs.titleBan', { name }),
  }[action];
  const verb = {
    timeout: t('lounge.memberDialogs.verbTimeout'),
    kick: t('lounge.memberDialogs.verbKick'),
    ban: t('lounge.memberDialogs.verbBan'),
  }[action];

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const why = reason.trim();
    try {
      if (action === 'timeout') {
        const until = new Date(Date.now() + seconds * 1000).toISOString();
        await timeoutRequest(guildId, userId, until, why);
      } else if (action === 'kick') {
        await kickMember(guildId, userId, why || undefined);
        useStore.getState().applyDispatch('GUILD_MEMBER_REMOVE', {
          guild_id: guildId,
          user: { id: userId },
        });
      } else {
        await banMember(guildId, userId, {
          ...(why ? { reason: why } : {}),
          ...(seconds ? { delete_message_seconds: seconds } : {}),
        });
        useStore.getState().applyDispatch('GUILD_MEMBER_REMOVE', {
          guild_id: guildId,
          user: { id: userId },
        });
      }
      onClose();
    } catch (err) {
      setError(
        errorText(
          err,
          {
            timeout: t('lounge.memberDialogs.errorTimeout', { name }),
            kick: t('lounge.memberDialogs.errorKick', { name }),
            ban: t('lounge.memberDialogs.errorBan', { name }),
          }[action],
        ),
      );
      setBusy(false);
    }
  };

  const options = action === 'timeout' ? TIMEOUT_OPTIONS : BAN_HISTORY_OPTIONS;
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={title}
      description={
        action === 'timeout'
          ? t('lounge.memberDialogs.descTimeout')
          : action === 'kick'
            ? t('lounge.memberDialogs.descKick')
            : t('lounge.memberDialogs.descBan')
      }
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        {action !== 'kick' && (
          <label className="lng-field">
            <span className="lng-field-label">
              {action === 'timeout'
                ? t('lounge.memberDialogs.duration')
                : t('lounge.memberDialogs.deleteHistory')}
            </span>
            <select
              className="lng-select"
              value={seconds}
              onChange={(e) => setSeconds(Number(e.target.value))}
            >
              {options.map((o) => (
                <option key={o.seconds} value={o.seconds}>
                  {t(o.labelKey)}
                </option>
              ))}
            </select>
          </label>
        )}
        <TextInput
          label={t('lounge.memberDialogs.reasonLabel')}
          value={reason}
          maxLength={512}
          onChange={(e) => setReason(e.target.value)}
        />
        {error && (
          <p role="alert" className="lng-form-error">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.memberDialogs.cancel')}
          </Button>
          <Button
            type="submit"
            variant={action === 'timeout' ? 'primary' : 'danger'}
            loading={busy}
          >
            {verb}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
