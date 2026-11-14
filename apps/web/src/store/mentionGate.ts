import type { Message } from './types';

/** Which kinds of mention a message holds for the viewer (D-721). */
export type MentionKind = { direct: boolean; everyone: boolean; role: boolean };

let gate: ((msg: Message, kind: MentionKind) => boolean) | null = null;

/** The Lounge registers its rules (blocked, ignored, suppress switches); `null` removes them. */
export const setMentionGate = (fn: typeof gate) => {
  gate = fn;
};

/** Whether a live message counts toward the viewer's mention badge. */
export const mentionCounts = (msg: Message, kind: MentionKind): boolean =>
  gate ? gate(msg, kind) : kind.direct || kind.everyone || kind.role;
