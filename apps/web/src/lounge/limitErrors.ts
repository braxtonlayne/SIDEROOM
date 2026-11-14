/**
 * The two server limits (D-659, D-660) as the member sees them. The server answers a create or a join that
 * would pass a limit with `400` and Discord's code: `30001` (the account is in the most servers it may be in;
 * the message ends with the number in force, "Maximum number of guilds reached (200)") or `30019` (the server
 * has the most members it may have). There is no pre-check and no counter in the UI: the server decides, and
 * this turns its answer into a sentence. Used by the create-server dialog, Join a Server, the invite page and
 * card, the Server Directory, Server Settings > applications, and the add-an-app consent page.
 */
import { ApiError } from '../api/client';
import type { useT } from '../i18n';

export const MAX_GUILDS = 30001;
export const MAX_GUILD_MEMBERS = 30019;

type Translate = ReturnType<typeof useT>;

/** What the member was doing: `approve` is a manager approving someone else's application; `bot` adds an app. */
export type LimitContext = 'self' | 'approve' | 'bot';

/** The number in force from "Maximum number of guilds reached (200)", or null when it cannot be read. */
export function limitFromMessage(message: string | undefined): number | null {
  const m = /\((\d{1,7})\)\s*$/.exec(message ?? '');
  return m?.[1] ? Number(m[1]) : null;
}

/** The sentence for a limit answer, or null when `e` is not one (the caller then shows what it showed before). */
export function limitErrorText(
  e: unknown,
  t: Translate,
  context: LimitContext = 'self',
): string | null {
  if (!(e instanceof ApiError)) return null;
  const code = e.body?.code;
  if (code === MAX_GUILD_MEMBERS) {
    return context === 'bot' ? t('lounge.limits.serverFullBot') : t('lounge.limits.serverFull');
  }
  if (code === MAX_GUILDS) {
    if (context === 'approve') return t('lounge.limits.applicantAtLimit');
    const count = limitFromMessage(e.body?.message ?? e.message);
    return count === null
      ? t('lounge.limits.maxServersNoCount')
      : t('lounge.limits.maxServers', { count });
  }
  return null;
}
