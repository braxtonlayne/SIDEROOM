import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../../i18n';
import { MAX_MESSAGE_LENGTH } from '../composerText';
import { currentMarkdownEngine } from '../markdown';

/**
 * In-place editor for an own message (LNG-MSGUI-027): Enter saves, Esc cancels, an emptied message asks
 * to be deleted instead. While a save is out the editor is disabled; when it resolves false the editor stays
 * open with the typed text and says so (SEC-AUD-135, Discord keeps the editor open on a failed edit).
 */
export function InlineEdit({
  initial,
  onSave,
  onCancel,
  onEmpty,
}: {
  initial: string;
  /** Resolves true when saved (the caller then closes the editor), false when it failed. */
  onSave: (content: string) => Promise<boolean> | void;
  onCancel: () => void;
  /** Saving an empty edit: the caller shows the delete prompt. */
  onEmpty: () => void;
}) {
  const t = useT();
  const errId = useId();
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement | null>(null);
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

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
    if (saving) return;
    const content = text.trim();
    if (!content) return onEmpty();
    if (content.length > MAX_MESSAGE_LENGTH) return;
    if (content === initial.trim()) return onCancel();
    const result = onSave(currentMarkdownEngine()?.expandShortcodes(content) ?? content);
    if (!result) return;
    setSaving(true);
    setFailed(false);
    void result.then((ok) => {
      if (!alive.current || ok) return;
      setSaving(false);
      setFailed(true);
      // The disabled field lost focus: give it back so the member can fix the text and try again.
      setTimeout(() => ref.current?.focus(), 0);
    });
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
        disabled={saving}
        aria-busy={saving || undefined}
        {...(failed ? { 'aria-describedby': errId } : {})}
        onChange={(e) => {
          setText(e.target.value);
          setFailed(false);
        }}
        onKeyDown={onKeyDown}
      />
      {failed && (
        <p role="alert" id={errId} className="lng-failed">
          {t('lounge.inlineEdit.notSaved')}
        </p>
      )}
      {text.trim().length > MAX_MESSAGE_LENGTH && (
        <p role="alert" className="lng-failed">
          {t('lounge.composer.overLimit', { count: text.trim().length - MAX_MESSAGE_LENGTH })}
        </p>
      )}
      <p className="lng-muted lng-inline-hint">
        {t('lounge.inlineEdit.escapeTo')}{' '}
        <button type="button" className="lng-link-btn" onClick={onCancel}>
          {t('lounge.inlineEdit.cancel')}
        </button>{' '}
        {t('lounge.inlineEdit.enterTo')}{' '}
        <button type="button" className="lng-link-btn" disabled={saving} onClick={save}>
          {t('lounge.inlineEdit.save')}
        </button>
      </p>
    </div>
  );
}
