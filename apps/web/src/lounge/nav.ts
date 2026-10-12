import { useNavigate, useParams } from '@tanstack/react-router';
import { useCallback } from 'react';
import type { Snowflake } from './data';
import { HOME } from './channels';

export interface LoungeRoute {
  guildId: Snowflake | undefined;
  channelId: Snowflake | undefined;
  messageId: Snowflake | undefined;
  threadId: Snowflake | undefined;
  isHome: boolean;
}

export function useLoungeRoute(): LoungeRoute {
  const p = useParams({ strict: false }) as {
    guildId?: string;
    channelId?: string;
    messageId?: string;
    threadId?: string;
  };
  return {
    ...p,
    guildId: p.guildId,
    channelId: p.channelId,
    messageId: p.messageId,
    threadId: p.threadId,
    isHome: p.guildId === HOME,
  };
}

export function useLoungeGoto() {
  const navigate = useNavigate();
  return useCallback(
    (guildId: Snowflake, channelId?: Snowflake) => {
      if (channelId)
        void navigate({ to: '/channels/$guildId/$channelId', params: { guildId, channelId } });
      else void navigate({ to: '/channels/$guildId', params: { guildId } });
    },
    [navigate],
  );
}
