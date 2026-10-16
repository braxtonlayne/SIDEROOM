// Wire types of the Activities routes (docs/11 section 3). Ids are strings.
export interface ActivityParticipant {
  user_id: string;
  joined_at_ms: number;
}

export interface ActivityInstance {
  id: string;
  application_id: string;
  application_name: string;
  guild_id: string;
  channel_id: string;
  /** `voice` or `text`. */
  channel_type: 'voice' | 'text';
  /** A development build (live entry URL), only for the owner and testers. */
  development: boolean;
  started_by: string;
  created_at_ms: number;
  participants: ActivityParticipant[];
}

export interface ShelfItem {
  application_id: string;
  name: string;
  description: string;
  /** A development build only the owner and testers see. */
  development: boolean;
}

export interface Shelf {
  /** `[activities] enabled` on the server. */
  enabled: boolean;
  items: ShelfItem[];
}

export interface ChannelActivity {
  enabled: boolean;
  instance: ActivityInstance | null;
}

/** `SR_ACTIVITY_INSTANCE_UPDATE`: the running instance of a channel, or `null` once it ended. */
export interface InstanceUpdateEvent {
  channel_id: string;
  instance: ActivityInstance | null;
}
