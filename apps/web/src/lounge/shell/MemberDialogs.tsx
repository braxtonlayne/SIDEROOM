import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { banMember, kickMember } from '../../settings/server/api';
import { useStore } from '../../store';
import { useGuild, useMembers } from '../data';

/** Timeout lengths offered in the timeout dialog (seconds). */
export const TIMEOUT_OPTIONS: { label: string; seconds: number }[] = [
  { label: '60 secs', seconds: 60 },
  { label: '5 mins', seconds: 300 },
  { label: '10 mins', seconds: 600 },
  { label: '1 hour', seconds: 3600 },
  { label: '1 day', seconds: 86400 },
  { label: '1 week', seconds: 604800 },
];

/** Message history removed along with a ban (seconds). */
export const BAN_HISTORY_OPTIONS: { label: string; seconds: number }[] = [
  { label: "Don't delete any", seconds: 0 },
  { label: 'Previous hour', seconds: 3600 },
  { label: 'Previous 24 hours', seconds: 86400 },
  { label: 'Previous 7 days', seconds: 604800 },
];

export type MemberAction = 'timeout' | 'kick' | 'ban';

const errorText = (e: unknown, fallback: string) =>
  e instanceof ApiError && e.status === 403
    ? 'You do not have permission to do that to this member.'
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
  const guild = useGuild(guildId);
  const member = useMembers(guildId).find((m) => m.user.id === userId);
  const name = member?.nick || member?.user.displayName || 'this member';
  const [reason, setReason] = useState('');
  const [seconds, setSeconds] = useState(action === 'timeout' ? 3600 : 0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!guild) return null;

  const title = { timeout: `Timeout ${name}`, kick: `Kick ${name}`, ban: `Ban ${name}` }[action];
  const verb = { timeout: 'Timeout', kick: 'Kick', ban: 'Ban' }[action];

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
      setError(errorText(err, `Could not ${verb.toLowerCase()} ${name}. Try again.`));
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
          ? 'They cannot send messages, react or join voice until the timeout ends.'
          : action === 'kick'
            ? 'They can rejoin with a new invite.'
            : 'They cannot rejoin until you unban them.'
      }
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        {action !== 'kick' && (
          <label className="lng-field">
            <span className="lng-field-label">
              {action === 'timeout' ? 'Duration' : 'Delete message history'}
            </span>
            <select
              className="lng-select"
              value={seconds}
              onChange={(e) => setSeconds(Number(e.target.value))}
            >
              {options.map((o) => (
                <option key={o.seconds} value={o.seconds}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
        )}
        <TextInput
          label="Reason (optional)"
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
            Cancel
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
