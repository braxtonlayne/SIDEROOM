// Labels of the moderation vocabulary, shared by the queue, the case page and the report dialog. Each table holds
// message keys that are resolved where they are rendered, never at module load.
import type { MessageKey } from '../../i18n';
import type { CaseState, Routing, Severity, TargetKind } from '../../moderation/api';

export const STATE_KEYS: Record<CaseState, MessageKey> = {
  open: 'admin.moderation.state.open',
  triage: 'admin.moderation.state.triage',
  investigating: 'admin.moderation.state.investigating',
  actioned: 'admin.moderation.state.actioned',
  dismissed: 'admin.moderation.state.dismissed',
  closed: 'admin.moderation.state.closed',
};

export const ROUTING_KEYS: Record<Routing, MessageKey> = {
  guild: 'admin.moderation.routing.guild',
  platform: 'admin.moderation.routing.platform',
  reviewers: 'admin.moderation.routing.reviewers',
};

export const SEVERITY_KEYS: Record<Severity, MessageKey> = {
  standard: 'admin.moderation.severity.standard',
  serious: 'admin.moderation.severity.serious',
};

export const TARGET_KEYS: Record<TargetKind, MessageKey> = {
  message: 'admin.moderation.target.message',
  user: 'admin.moderation.target.user',
  channel: 'admin.moderation.target.channel',
  guild: 'admin.moderation.target.guild',
  library: 'admin.moderation.target.library',
  cinema: 'admin.moderation.target.cinema',
  person: 'admin.moderation.target.person',
  other: 'admin.moderation.target.other',
};

/** The timeline entries the backend writes (`moderation_case_events.kind`). */
export const EVENT_KEYS: Record<string, MessageKey> = {
  case_opened: 'admin.moderation.event.caseOpened',
  report_attached: 'admin.moderation.event.reportAttached',
  state_changed: 'admin.moderation.event.stateChanged',
  note_added: 'admin.moderation.event.noteAdded',
  claimed: 'admin.moderation.event.claimed',
  released: 'admin.moderation.event.released',
  escalated: 'admin.moderation.event.escalated',
  hold_placed: 'admin.moderation.event.holdPlaced',
  hold_released: 'admin.moderation.event.holdReleased',
  warned: 'admin.moderation.event.warned',
  action_recorded: 'admin.moderation.event.actionRecorded',
  platform_ban: 'admin.moderation.event.platformBan',
  appeal_filed: 'admin.moderation.event.appealFiled',
  appeal_decided: 'admin.moderation.event.appealDecided',
  reopened: 'admin.moderation.event.reopened',
  continued_in: 'admin.moderation.event.continuedIn',
  action_performed: 'admin.moderation.event.actionPerformed',
  member_self_delete: 'admin.moderation.event.memberSelfDelete',
  strike_review_opened: 'admin.moderation.event.strikeReview',
  counter_notice: 'admin.moderation.event.takedownCounterNotice',
  assigned: 'admin.moderation.event.assigned',
  suspicion_raised: 'admin.moderation.event.suspicionRaised',
  suspicion_escalated: 'admin.moderation.event.suspicionEscalated',
  suspicion_cleared: 'admin.moderation.event.suspicionCleared',
};

/** The actions a moderator can record as taken with the server's own tools. */
export const RECORDABLE_ACTIONS: { id: string; key: MessageKey }[] = [
  { id: 'delete_message', key: 'admin.moderation.action.deleteMessage' },
  { id: 'restore_message', key: 'admin.moderation.action.restoreMessage' },
  { id: 'remove_attachment', key: 'admin.moderation.action.removeAttachment' },
  { id: 'timeout', key: 'admin.moderation.action.timeout' },
  { id: 'kick', key: 'admin.moderation.action.kick' },
  { id: 'guild_ban', key: 'admin.moderation.action.guildBan' },
  { id: 'mark_nsfw', key: 'admin.moderation.action.markNsfw' },
  { id: 'other', key: 'admin.moderation.action.other' },
];

/** Moves the lifecycle allows from a state (mirrors `sr_moderation::rules::can_transition`). */
export const MOVES: Record<CaseState, CaseState[]> = {
  open: ['triage', 'investigating', 'actioned', 'dismissed'],
  triage: ['investigating', 'actioned', 'dismissed'],
  investigating: ['triage', 'actioned', 'dismissed'],
  actioned: ['investigating', 'closed'],
  dismissed: ['investigating', 'closed'],
  closed: [],
};
