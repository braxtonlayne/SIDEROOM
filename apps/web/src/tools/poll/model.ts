/**
 * UTL-COL-001: the poll builder as keys of a tool room.
 *
 *   poll      { question, hours, multi }
 *   opt.<id>  { text, emoji, pos }
 *
 * "Post" turns it into the existing poll message (`POST /channels/{id}/messages` with a `poll`).
 */
import { newId } from '../tierlist/model';

export const POLL_MIN_OPTIONS = 2;
export const POLL_MAX_OPTIONS = 10;
export const POLL_MAX_QUESTION = 300;
export const POLL_MAX_OPTION = 55;
export const POLL_HOURS = [1, 4, 8, 24, 72, 168] as const;

export interface PollOption {
  id: string;
  text: string;
  emoji: string;
  pos: number;
}
export interface PollDraft {
  question: string;
  hours: number;
  multi: boolean;
  options: PollOption[];
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

export function parsePoll(state: Record<string, unknown>): PollDraft {
  const head = isObj(state.poll) ? state.poll : {};
  const options: PollOption[] = [];
  for (const [key, v] of Object.entries(state)) {
    if (!key.startsWith('opt.') || !isObj(v)) continue;
    options.push({
      id: key.slice(4),
      text: typeof v.text === 'string' ? v.text : '',
      emoji: typeof v.emoji === 'string' ? v.emoji : '',
      pos: typeof v.pos === 'number' && Number.isFinite(v.pos) ? v.pos : 0,
    });
  }
  options.sort((a, b) => a.pos - b.pos || (a.id < b.id ? -1 : 1));
  return {
    question: typeof head.question === 'string' ? head.question : '',
    hours: typeof head.hours === 'number' ? head.hours : 24,
    multi: head.multi === true,
    options,
  };
}

/** The keys that change when one setting of the head changes (the other head fields are kept). */
export function setHead(
  state: Record<string, unknown>,
  change: Partial<Pick<PollDraft, 'question' | 'hours' | 'multi'>>,
): Record<string, unknown> {
  const cur = parsePoll(state);
  return {
    poll: {
      question: (change.question ?? cur.question).slice(0, POLL_MAX_QUESTION),
      hours: change.hours ?? cur.hours,
      multi: change.multi ?? cur.multi,
    },
  };
}

export function setOption(
  state: Record<string, unknown>,
  id: string,
  change: Partial<Pick<PollOption, 'text' | 'emoji'>>,
): Record<string, unknown> | null {
  const cur = parsePoll(state).options.find((o) => o.id === id);
  if (!cur) return null;
  return {
    [`opt.${id}`]: {
      text: (change.text ?? cur.text).slice(0, POLL_MAX_OPTION),
      emoji: (change.emoji ?? cur.emoji).slice(0, 8),
      pos: cur.pos,
    },
  };
}

export function addOption(state: Record<string, unknown>): Record<string, unknown> | null {
  const { options } = parsePoll(state);
  if (options.length >= POLL_MAX_OPTIONS) return null;
  const last = options[options.length - 1];
  return { [`opt.${newId()}`]: { text: '', emoji: '', pos: last ? last.pos + 1024 : 1024 } };
}

export function removeOption(
  state: Record<string, unknown>,
  id: string,
): Record<string, unknown> | null {
  const { options } = parsePoll(state);
  if (options.length <= POLL_MIN_OPTIONS) return null;
  return { [`opt.${id}`]: null };
}

/** Why the draft cannot be posted yet, or `null`. Codes map to catalog keys in the UI. */
export function pollProblem(d: PollDraft): 'question' | 'options' | 'long' | null {
  if (!d.question.trim()) return 'question';
  if (d.question.length > POLL_MAX_QUESTION) return 'long';
  const filled = d.options.filter((o) => o.text.trim());
  if (filled.length < POLL_MIN_OPTIONS) return 'options';
  if (filled.some((o) => o.text.length > POLL_MAX_OPTION)) return 'long';
  return null;
}

/** The `poll` of `POST /channels/{id}/messages` (the Discord shape the Create Poll modal also sends). */
export function toMessagePoll(d: PollDraft) {
  return {
    question: { text: d.question.trim() },
    answers: d.options
      .filter((o) => o.text.trim())
      .map((o) => ({
        poll_media: { text: o.text.trim(), ...(o.emoji ? { emoji: { name: o.emoji } } : {}) },
      })),
    duration: d.hours,
    allow_multiselect: d.multi,
  };
}

/** The starting keys: a question and two empty options. */
export function starterPoll(): Record<string, unknown> {
  return {
    poll: { question: '', hours: 24, multi: false },
    [`opt.${newId()}`]: { text: '', emoji: '', pos: 1024 },
    [`opt.${newId()}`]: { text: '', emoji: '', pos: 2048 },
  };
}
