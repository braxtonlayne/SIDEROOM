import type { Message } from '../../store';
import type { MentionKind } from '../../store/mentionGate';
import { hiddenAuthorsNow } from './hiddenAuthors';
import { useNotifSettings } from './notifSettings';

/**
 * The Lounge's rule for the live mention badge (D-721, D-726), the server's rule in `message_sent`: nothing for a
 * blocked or ignored author; a direct mention always; `@everyone`/`@here` and a role mention only when the
 * person has not suppressed that kind for the server. Settings not loaded yet: it counts, and the server's
 * number replaces the count on the next load. Reads the stores at call time (no subscription, no request).
 */
export function loungeMentionGate(msg: Message, kind: MentionKind): boolean {
  const hidden = hiddenAuthorsNow();
  if (hidden.blocked.has(msg.author.id) || hidden.ignored.has(msg.author.id)) return false;
  if (kind.direct) return true;
  const scope = msg.guild_id ? useNotifSettings.getState().byGuild[msg.guild_id] : undefined;
  return (kind.everyone && !scope?.suppressEveryone) || (kind.role && !scope?.suppressRoles);
}
