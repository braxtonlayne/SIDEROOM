import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../../i18n';
import { MAX_MESSAGE_LENGTH } from '../composerText';
import { currentMarkdownEngine } from '../markdown';

/**
 * In-place editor for an own message (LNG-MSGUI-027): Enter saves, Esc cancels, an emptied message asks
 * to be deleted instead.
 */
export function InlineEdit({
  initial,
  onSave,
  onCancel,
  onEmpty,
}: {
  initial: string;
  onSave: (content: string) => void;
  onCancel: () => void;
  /** Saving an empty edit: the caller shows the delete prompt. */
  onEmpty: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 320)}px`;
  }, [text]);

  const save = () => {
    const content = text.trim();
    if (!content) return onEmpty();
    if (content.length > MAX_MESSAGE_LENGTH) return;
    if (content === initial.trim()) return onCancel();
    onSave(currentMarkdownEngine()?.expandShortcodes(content) ?? content);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      save();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    }
  };

  return (
    <div className="lng-inline-edit">
      <textarea
        ref={ref}
        className="lng-textarea lng-inline-textarea"
        aria-label={t('lounge.inlineEdit.label')}
        rows={1}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
      />
      <p className="lng-muted lng-inline-hint">
        {t('lounge.inlineEdit.escapeTo')}{' '}
        <button type="button" className="lng-link-btn" onClick={onCancel}>
          {t('lounge.inlineEdit.cancel')}
        </button>{' '}
        {t('lounge.inlineEdit.enterTo')}{' '}
        <button type="button" className="lng-link-btn" onClick={save}>
          {t('lounge.inlineEdit.save')}
        </button>
      </p>
    </div>
  );
}
