export const CRASH_PATTERNS: {
  name: string;
  re: RegExp;
  reject?: (m: RegExpExecArray) => boolean;
}[];
/** Names of the secret patterns that match `text` (each once, never the matched text). */
export function findLeaks(text: string): string[];
