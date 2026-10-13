import { Button, Checkbox, Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import * as api from '../api';
import { ActionStatus, useAction } from '../ui';

export const PRUNE_DAYS = [1, 7, 30] as const;

/**
 * Copy for the inactivity rule (F-26, D-166). The backend tracks last-seen time (F-02), so the wording is
 * Discord's: members who have not been online in the window.
 */
export const LAST_SEEN_TRACKED = true;
export const pruneRule = (days: number) =>
  `Members who have not been online for ${days} ${days === 1 ? 'day' : 'days'}`;

/** Server Settings, Members: preview and run a prune (`GET/POST /guilds/{id}/prune`, KICK_MEMBERS). */
export function PruneDialog({
  guildId,
  roles,
  onClose,
  onDone,
}: {
  guildId: string;
  roles: { id: string; name: string }[];
  onClose: () => void;
  onDone: (removed: number) => void;
}) {
  const [days, setDays] = useState<number>(7);
  const [include, setInclude] = useState<string[]>([]);
  const [count, setCount] = useState<number | null>(null);
  const [countError, setCountError] = useState<string | null>(null);
  const action = useAction();
  const pickable = roles.filter((r) => r.id !== guildId);

  useEffect(() => {
    let live = true;
    setCount(null);
    setCountError(null);
    api
      .pruneCount(guildId, days, include)
      .then((r) => live && setCount(r.pruned))
      .catch((e: unknown) => live && setCountError(api.errorText(e, 'Could not count members.')));
    return () => {
      live = false;
    };
  }, [guildId, days, include]);

  const run = async () => {
    const out = await action.run(() => api.pruneMembers(guildId, days, include));
    if (out) onDone(out.pruned);
  };

  const noun = (n: number) => (n === 1 ? 'member' : 'members');
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title="Prune members"
      description={`${pruneRule(days)} and have no roles are removed. They can rejoin with a new invite.`}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={action.busy}
            disabled={count === null || count === 0}
            onClick={() => void run()}
          >
            {count ? `Prune ${count} ${noun(count)}` : 'Prune'}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <div>
          <label className="ss-field-label" htmlFor="ss-prune-days">
            Inactive for
          </label>
          <select
            id="ss-prune-days"
            className="ss-select"
            value={days}
            onChange={(e) => setDays(Number(e.target.value))}
          >
            {PRUNE_DAYS.map((d) => (
              <option key={d} value={d}>
                {d} {d === 1 ? 'day' : 'days'}
              </option>
            ))}
          </select>
        </div>
        {pickable.length > 0 && (
          <fieldset className="ss-fieldset">
            <legend className="ss-field-label">Also prune members with these roles</legend>
            <ul className="ss-list" aria-label="Roles to include">
              {pickable.map((r) => (
                <li key={r.id} className="ss-row">
                  <Checkbox
                    label={r.name}
                    checked={include.includes(r.id)}
                    onChange={(e) =>
                      setInclude((cur) =>
                        e.target.checked ? [...cur, r.id] : cur.filter((x) => x !== r.id),
                      )
                    }
                  />
                </li>
              ))}
            </ul>
          </fieldset>
        )}
        <p role="status" className="ss-sub" data-testid="prune-count">
          {countError ??
            (count === null
              ? 'Counting…'
              : count === 0
                ? 'No members would be removed.'
                : `${count} ${noun(count)} would be removed.`)}
        </p>
      </div>
    </Modal>
  );
}
