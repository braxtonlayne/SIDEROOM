import type { MenuEntry } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../../i18n';
import { indexedCommands } from './commands';
import { newNonce, sendInteraction } from './interactions';
import { useAppCommands } from './useAppCommands';
import { useReportOutcome } from './useReportOutcome';

/**
 * The "Apps" submenu of a message (message commands) or a person (user commands), DEV-BOT-009. Empty outside
 * servers and when no installed app offers a context command of that kind; otherwise one entry per command that
 * starts the interaction with `target_id` set to the message or user.
 */
export function useAppsMenu(
  kind: 2 | 3,
  where: { guildId: string | undefined; channelId: string | undefined },
  targetId: string,
): MenuEntry[] {
  const t = useT();
  const report = useReportOutcome();
  const { guildId, channelId } = where;
  const index = useAppCommands(channelId, !!guildId);
  return useMemo(() => {
    const list = indexedCommands(index, kind);
    if (!guildId || !channelId || list.length === 0) return [];
    const items: MenuEntry[] = list.map(({ command, app }) => ({
      label: `${command.name} (${app.name})`,
      onSelect: () => {
        void sendInteraction({
          type: 2,
          application_id: command.application_id,
          channel_id: channelId,
          guild_id: guildId,
          nonce: newNonce(),
          data: { id: command.id, type: kind, target_id: targetId },
        }).then(report);
      },
    }));
    return [{ type: 'submenu', label: t('lounge.apps.menu.apps'), items } as MenuEntry];
  }, [index, kind, guildId, channelId, targetId, t, report]);
}
