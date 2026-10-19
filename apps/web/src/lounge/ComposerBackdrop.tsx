import { useEffect, useLayoutEffect, useMemo, useRef, type RefObject } from 'react';
import { BACKDROP_MAX_CHARS, tokenizeComposer } from './composerText';

/** Whether the backdrop is drawn for this draft (off for very long drafts, D-299). */
export const backdropActive = (text: string): boolean =>
  text.length > 0 && text.length <= BACKDROP_MAX_CHARS;

/**
 * Highlight backdrop (LNG-CMP-001, D-299): the draft drawn in a `pre` under the textarea, whose own text is
 * transparent, with pills behind mention, channel, emoji and timestamp tokens. Same font, padding, wrapping and
 * scroll offset as the textarea; the textarea keeps the caret and every input behaviour.
 */
export function ComposerBackdrop({
  text,
  target,
}: {
  text: string;
  target: RefObject<HTMLTextAreaElement | null>;
}) {
  const pre = useRef<HTMLPreElement>(null);
  const tokens = useMemo(() => tokenizeComposer(text), [text]);

  const sync = () => {
    const ta = target.current;
    const el = pre.current;
    if (!ta || !el) return;
    // The textarea's scrollbar narrows its text column; give the mirror the same gutter.
    el.style.paddingInlineEnd = `calc(var(--lng-backdrop-pad, 4px) + ${ta.offsetWidth - ta.clientWidth}px)`;
    el.scrollTop = ta.scrollTop;
  };

  useLayoutEffect(sync, [text]);
  useEffect(() => {
    const ta = target.current;
    if (!ta) return;
    ta.addEventListener('scroll', sync);
    const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(sync);
    ro?.observe(ta);
    sync();
    return () => {
      ta.removeEventListener('scroll', sync);
      ro?.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  return (
    <pre ref={pre} className="lng-backdrop" aria-hidden="true" data-testid="composer-backdrop">
      {tokens.map((tk, i) =>
        tk.kind === 'text' ? (
          tk.text
        ) : (
          <mark key={i} className="lng-backdrop-token" data-kind={tk.kind}>
            {tk.text}
          </mark>
        ),
      )}
      {/* A trailing newline would collapse; keep the last line's height. */}
      {text.endsWith('\n') ? '​' : null}
    </pre>
  );
}
