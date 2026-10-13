/** "Convert emoticons to emoji" (User Settings > Text & Images): `:)` becomes a smiley when the message is sent. */
const EMOTICONS: Record<string, string> = {
  ':)': '🙂',
  ':-)': '🙂',
  '(:': '🙂',
  ':(': '🙁',
  ':-(': '🙁',
  ":'(": '😢',
  ':D': '😄',
  ':-D': '😄',
  ';)': '😉',
  ';-)': '😉',
  ':P': '😛',
  ':-P': '😛',
  ':p': '😛',
  ':O': '😮',
  ':o': '😮',
  ':/': '😕',
  ':|': '😐',
  '<3': '❤️',
  xD: '😆',
};

/**
 * Replaces emoticons that stand alone between spaces (or at the ends). Text with a backtick is left as it is,
 * so code is never rewritten, and so is anything that is part of a longer word or a link (`http://`).
 */
export function convertEmoticons(text: string): string {
  if (text.includes('`')) return text;
  return text.replace(/(^|\s)(\S{2,3})(?=\s|$)/g, (m, lead: string, token: string) => {
    const emoji = EMOTICONS[token];
    return emoji ? lead + emoji : m;
  });
}
