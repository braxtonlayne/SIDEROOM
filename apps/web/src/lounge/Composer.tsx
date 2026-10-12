import { Icon, Popover, Tooltip, useToast } from '@sideroom/ui';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ChangeEvent, KeyboardEvent } from 'react';
import { EmojiPicker } from './components/EmojiPicker';
import {
  insertAt,
  MAX_MESSAGE_LENGTH,
  toggleWrap,
  typingLabel,
  WRAP_KEYS,
  WRAP_SHIFT_KEYS,
  type TextEdit,
} from './composerText';
import {
  useCurrentUser,
  useLoungeActions,
  useTyping,
  useUsers,
  type Message,
  type Snowflake,
} from './data';
import { drafts } from './drafts';
import { currentMarkdownEngine, loadMarkdownEngine, type MarkdownEngine } from './markdown';

/** Shortcodes become emoji at send time (Q89): the parser leaves `:name:` literal. */
function expandForSend(text: string, then: (content: string) => void) {
  const engine: MarkdownEngine | null = currentMarkdownEngine();
  if (engine) {
    then(engine.expandShortcodes(text));
    return;
  }
  // The engine is still loading: wait for it rather than sending literal shortcodes.
  loadMarkdownEngine().then(
    (loaded) => then(loaded.expandShortcodes(text)),
    () => then(text),
  );
}

/** Test hook: forgets every unsent draft (memory and localStorage). */
export const resetComposerDrafts = () => drafts.clear();

export interface ComposerProps {
  channelId: Snowflake;
  /** "#general" or a person's name; used in the placeholder. */
  target: string;
  canSend: boolean;
  replyTo: Message | null;
  onCancelReply: () => void;
  editing: Message | null;
  onCancelEdit: () => void;
  /** ArrowUp in an empty composer: edit my last message. */
  onEditLast: () => void;
}

export function Composer({
  channelId,
  target,
  canSend,
  replyTo,
  onCancelReply,
  editing,
  onCancelEdit,
  onEditLast,
}: ComposerProps) {
  const actions = useLoungeActions();
  const me = useCurrentUser();
  const users = useUsers();
  const typingIds = useTyping(channelId);
  const toast = useToast();
  const ref = useRef<HTMLTextAreaElement | null>(null);
  const [text, setText] = useState(() => drafts.get(channelId) ?? '');
  const pendingSel = useRef<{ s: number; e: number } | null>(null);
  const lastTyping = useRef(0);

  // Entering edit mode loads the message; leaving it restores the draft.
  useEffect(() => {
    if (editing) {
      setText(editing.content);
      ref.current?.focus();
    } else {
      setText(drafts.get(channelId) ?? '');
    }
  }, [editing, channelId]);

  useEffect(() => {
    if (replyTo) ref.current?.focus();
  }, [replyTo]);

  // Auto-grow up to ~40% of the viewport, then scroll inside the box.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = 'auto';
    const max = Math.max(120, Math.round(window.innerHeight * 0.4));
    el.style.height = `${Math.min(el.scrollHeight, max)}px`;
    el.style.overflowY = el.scrollHeight > max ? 'auto' : 'hidden';
  }, [text, canSend]);

  useLayoutEffect(() => {
    if (pendingSel.current && ref.current) {
      ref.current.setSelectionRange(pendingSel.current.s, pendingSel.current.e);
      pendingSel.current = null;
    }
  });

  const apply = useCallback(
    (edit: TextEdit) => {
      pendingSel.current = { s: edit.selStart, e: edit.selEnd };
      setText(edit.value);
      if (!editing) drafts.set(channelId, edit.value);
    },
    [channelId, editing],
  );

  const sel = () => {
    const el = ref.current;
    return { s: el?.selectionStart ?? text.length, e: el?.selectionEnd ?? text.length };
  };

  const submit = (content: string) => {
    if (editing) {
      actions.editMessage(channelId, editing.id, content);
      onCancelEdit();
    } else {
      actions.sendMessage(channelId, content, replyTo?.id ?? null);
      drafts.delete(channelId);
      setText('');
      onCancelReply();
    }
  };

  const send = () => {
    const content = text.trim();
    if (!content || content.length > MAX_MESSAGE_LENGTH) return;
    expandForSend(content, submit);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
      return;
    }
    if (e.key === 'Escape') {
      if (editing) {
        e.preventDefault();
        e.stopPropagation();
        onCancelEdit();
      } else if (replyTo) {
        e.preventDefault();
        e.stopPropagation();
        onCancelReply();
      }
      return;
    }
    if (e.key === 'ArrowUp' && text === '' && !editing) {
      e.preventDefault();
      onEditLast();
      return;
    }
    if (mod && !e.altKey) {
      const k = e.key.toLowerCase();
      const mark = e.shiftKey ? WRAP_SHIFT_KEYS[k] : WRAP_KEYS[k];
      if (mark) {
        e.preventDefault();
        const { s, e: end } = sel();
        apply(toggleWrap(text, s, end, mark));
      }
    }
  };

  const onChange = (e: ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value);
    if (!editing) drafts.set(channelId, e.target.value);
    const now = Date.now();
    if (e.target.value && now - lastTyping.current > 8000) {
      lastTyping.current = now;
      actions.startTyping(channelId);
    }
  };

  const typing = typingLabel(
    typingIds.filter((id) => id !== me.id).map((id) => users[id]?.displayName ?? 'Someone'),
  );
  const remaining = MAX_MESSAGE_LENGTH - text.length;
  const showCounter = remaining <= 200;

  if (!canSend) {
    return (
      <div className="lng-composer-wrap">
        <div className="lng-composer lng-composer-locked" role="note">
          <Icon name="lock" size={20} />
          You do not have permission to send messages in this channel.
        </div>
      </div>
    );
  }

  return (
    <div className="lng-composer-wrap" data-drop-target>
      {replyTo && (
        <div className="lng-bar" role="status">
          <span>
            Replying to <strong>{replyTo.author.displayName}</strong>
          </span>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Cancel reply"
            onClick={onCancelReply}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      )}
      {editing && (
        <div className="lng-bar" role="status">
          <span>
            Editing message. <span className="lng-muted">Esc to cancel, Enter to save</span>
          </span>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Cancel edit"
            onClick={onCancelEdit}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      )}
      <div className="lng-composer">
        <Tooltip content="Upload a File">
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Upload a File"
            onClick={() =>
              toast.show({
                title: 'Uploads are not available yet',
                variant: 'info',
                duration: 2500,
              })
            }
          >
            <Icon name="plus" size={20} />
          </button>
        </Tooltip>
        <textarea
          ref={ref}
          className="lng-textarea"
          data-composer
          rows={1}
          aria-label={`Message ${target}`}
          placeholder={`Message ${target}`}
          value={text}
          onChange={onChange}
          onKeyDown={onKeyDown}
        />
        {showCounter && (
          <span
            className="lng-counter"
            data-over={remaining < 0 || undefined}
            role="status"
            aria-label={`${remaining} characters remaining`}
          >
            {remaining}
          </span>
        )}
        <Popover
          label="Emoji picker"
          placement="top-end"
          trigger={
            <button type="button" className="lng-icon-btn" aria-label="Open emoji picker">
              <Icon name="smile" size={20} />
            </button>
          }
        >
          <EmojiPicker
            onPick={(emoji) => {
              const { s, e } = sel();
              apply(insertAt(text, s, e, emoji));
              ref.current?.focus();
            }}
          />
        </Popover>
        <Tooltip content="Send message">
          <button
            type="button"
            className="lng-icon-btn lng-send"
            aria-label="Send message"
            disabled={!text.trim() || remaining < 0}
            onClick={send}
          >
            <Icon name="arrow-right" size={20} />
          </button>
        </Tooltip>
      </div>
      <div className="lng-typing" aria-live="polite">
        {typing}
      </div>
    </div>
  );
}
