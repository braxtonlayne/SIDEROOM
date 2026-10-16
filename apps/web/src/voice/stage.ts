/**
 * Stage channels on the client (VOX-STG-002), over the backend of VOX-STG-001
 * (crates/sr-gateway/src/voice_stage.rs and crates/sr-api/src/routes/voice_live.rs):
 *
 *   GET /guilds/{id}/stage-instances             the stages running in a server
 *   POST /stage-instances {channel_id, topic}    start a stage (moderators)
 *   PATCH|DELETE /stage-instances/{channel_id}   change the topic / end the stage (moderators)
 *   PATCH /guilds/{id}/voice-states/@me          raise or lower a hand, step down
 *   PATCH /guilds/{id}/voice-states/{user}       invite to speak / move to the audience (moderators)
 *   STAGE_INSTANCE_CREATE / UPDATE / DELETE      `{ id, guild_id, channel_id, topic, privacy_level }`
 *
 * Everyone who joins is in the audience (`suppress: true` in their voice state) except moderators; a raised
 * hand is `request_to_speak_timestamp`. The SFU drops audio from suppressed members, so the client also keeps
 * a suppressed member's microphone shut (controller.ts).
 */
import { create } from 'zustand';
import { apiFetch } from '../api/client';

export interface StageInstance {
  channelId: string;
  guildId: string;
  topic: string;
}

interface StageState {
  /** Running stages by channel id. */
  instances: Record<string, StageInstance>;
  /** Servers whose list was loaded. */
  loaded: Record<string, boolean>;
}

export const useStage = create<StageState>()(() => ({ instances: {}, loaded: {} }));

export const resetStage = () => useStage.setState({ instances: {}, loaded: {} });

export function parseInstance(raw: unknown): StageInstance | null {
  const d = (raw ?? {}) as Record<string, unknown>;
  const channelId = typeof d.channel_id === 'string' ? d.channel_id : null;
  if (!channelId) return null;
  return {
    channelId,
    guildId: typeof d.guild_id === 'string' ? d.guild_id : '',
    topic: typeof d.topic === 'string' ? d.topic : '',
  };
}

/** Apply a gateway dispatch (also the `stage_instances` of GUILD_CREATE). Safe to call for every event. */
export function applyStageEvent(event: string, data: unknown): void {
  if (event === 'GUILD_CREATE') {
    const list = (data as { stage_instances?: unknown } | null)?.stage_instances;
    const guildId = (data as { id?: unknown } | null)?.id;
    if (!Array.isArray(list)) return;
    const found = list.map(parseInstance).filter((i): i is StageInstance => !!i);
    useStage.setState((s) => {
      const instances = Object.fromEntries(
        Object.entries(s.instances).filter(([, i]) => i.guildId !== guildId),
      );
      for (const i of found) instances[i.channelId] = i;
      return { instances };
    });
    return;
  }
  if (
    event !== 'STAGE_INSTANCE_CREATE' &&
    event !== 'STAGE_INSTANCE_UPDATE' &&
    event !== 'STAGE_INSTANCE_DELETE'
  )
    return;
  const inst = parseInstance(data);
  if (!inst) return;
  useStage.setState((s) => {
    const instances = { ...s.instances };
    if (event === 'STAGE_INSTANCE_DELETE') delete instances[inst.channelId];
    else instances[inst.channelId] = inst;
    return { instances };
  });
}

export async function loadStages(guildId: string): Promise<void> {
  const raw = await apiFetch<unknown[]>(`/guilds/${guildId}/stage-instances`);
  const found = (Array.isArray(raw) ? raw : [])
    .map(parseInstance)
    .filter((i): i is StageInstance => !!i);
  useStage.setState((s) => {
    const instances = Object.fromEntries(
      Object.entries(s.instances).filter(([, i]) => i.guildId !== guildId),
    );
    for (const i of found) instances[i.channelId] = { ...i, guildId };
    return { instances, loaded: { ...s.loaded, [guildId]: true } };
  });
}

// ---- actions -------------------------------------------------------------------------------------

export const startStage = (channelId: string, topic: string) =>
  apiFetch<unknown>('/stage-instances', {
    method: 'POST',
    json: { channel_id: channelId, topic, privacy_level: 2 },
    retries: 0,
  });

export const setStageTopic = (channelId: string, topic: string) =>
  apiFetch<unknown>(`/stage-instances/${channelId}`, {
    method: 'PATCH',
    json: { topic },
    retries: 0,
  });

export const endStage = (channelId: string) =>
  apiFetch<void>(`/stage-instances/${channelId}`, { method: 'DELETE', retries: 0 });

const selfState = (guildId: string, body: Record<string, unknown>) =>
  apiFetch<void>(`/guilds/${guildId}/voice-states/@me`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

/** Ask to speak (the server stamps the time); `false` lowers the hand again. */
export const raiseHand = (guildId: string, channelId: string, raised: boolean) =>
  selfState(guildId, {
    channel_id: channelId,
    request_to_speak_timestamp: raised ? new Date().toISOString() : null,
  });

/** A speaker goes back to the audience. */
export const stepDown = (guildId: string, channelId: string) =>
  selfState(guildId, { channel_id: channelId, suppress: true });

/** A moderator who is on the stage becomes a speaker without a hand (needs Mute Members). */
export const becomeSpeaker = (guildId: string, channelId: string) =>
  selfState(guildId, { channel_id: channelId, suppress: false });

/** A moderator invites someone to speak (`suppress: false`) or sends a speaker back to the audience. */
export const setSpeaker = (guildId: string, userId: string, speaker: boolean) =>
  apiFetch<void>(`/guilds/${guildId}/voice-states/${userId}`, {
    method: 'PATCH',
    json: { suppress: !speaker },
    retries: 0,
  });
