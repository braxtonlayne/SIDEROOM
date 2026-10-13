import { useToast } from '@sideroom/ui';
import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useLoungeGoto } from '../nav';
import { dueReminders, useSaved, useSavedSync } from './savedMessages';

export const REMINDER_CHECK_MS = 15_000;

/**
 * Shows reminders when they come due (LNG-MSGUI-026 Remind Me). Mounted once in the shell: it also keeps the
 * saved maps in step with the settings blob. Reminders that fell due while the app was closed show on load.
 */
export function ReminderTicker() {
  const mock = useContext(LoungeDataContext);
  const toast = useToast();
  const goto = useLoungeGoto();
  useSavedSync(!mock);
  // Re-check whenever the list changes (the settings blob loads after mount; a new reminder may be due).
  const reminders = useSaved((s) => s.reminders);

  useEffect(() => {
    if (mock) return;
    const check = () => {
      const { reminders, dismissReminder } = useSaved.getState();
      for (const r of dueReminders(reminders)) {
        dismissReminder(r.message_id);
        toast.show({
          title: 'Reminder',
          description: (
            <>
              <strong>{r.author}</strong>: {r.preview}{' '}
              <a
                href={`/channels/${r.guild_id ?? '@me'}/${r.channel_id}/${r.message_id}`}
                onClick={(e) => {
                  e.preventDefault();
                  goto(r.guild_id ?? '@me', r.channel_id);
                }}
              >
                Go to message
              </a>
            </>
          ),
          variant: 'info',
          duration: 0,
        });
      }
    };
    check();
    const t = setInterval(check, REMINDER_CHECK_MS);
    return () => clearInterval(t);
  }, [mock, toast, goto, reminders]);
  return null;
}
