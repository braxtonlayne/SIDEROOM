import { Button, DropdownMenu, Icon, Popover, Tooltip, useToast } from '@sideroom/ui';
import {
  forwardRef,
  useCallback,
  useEffect,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ChangeEvent, ClipboardEvent, KeyboardEvent } from 'react';
import { ApiError, apiFetch } from '../api/client';
import { announce } from '../a11y/announcer';
import { formatDateTime, formatTime, useT } from '../i18n';
import { useStore } from '../store';
import { usePrefsStore, useSavedPrefs } from '../settings/user/prefs';
import { convertEmoticons } from './emoticons';
import { EmojiPicker } from './components/EmojiPicker';
import { ExpressionPicker, type GuildSticker } from './components/ExpressionPicker';
import { customEmojiText } from './components/EmojiPicker';
import { EMOJI_CATEGORIES } from './components/emoji';
import {
  insertAt,
  MAX_MESSAGE_LENGTH,
  prefixLines,
  toggleWrap,
  typingLabel,
  WRAP_KEYS,
  WRAP_SHIFT_KEYS,
  type TextEdit,
} from './composerText';
import {
  useChannels,
  useCurrentUser,
  useLoungeActions,
  useMembers,
  useRoles,
  useTyping,
  useUsers,
  type Message,
  type NewPoll,
  type Snowflake,
} from './data';
import { drafts } from './drafts';
import { currentMarkdownEngine, loadMarkdownEngine, type MarkdownEngine } from './markdown';
import {
  applySuggestion,
  channelSuggestions,
  emojiSuggestions,
  findTrigger,
  mentionSuggestions,
  resolveTokens,
  type Suggestion,
  type Trigger,
} from './msg/autocomplete';
import { bridgedColor, findBridgedMember, useGuildBridgedMembers } from './bridge/members';
import { AutocompletePopup } from './msg/AutocompletePopup';
import { MarkdownPreview, StickerSuggestions } from './msg/ComposerExtras';
import { ScheduleDialog } from './features/ScheduleDialog';
import { CommandForm } from './apps/CommandForm';
import {
  indexedCommands,
  matchTyped,
  parseTypedArgs,
  runsImmediately,
  searchCommands,
  type IndexedCommand,
  type TypedArgs,
} from './apps/commands';
import { newNonce, sendInteraction } from './apps/interactions';
import { useAppCommands } from './apps/useAppCommands';
import { useActivitiesEnabled } from '../activities/hooks';
import { useReportOutcome } from './apps/useReportOutcome';
import { PollModal } from './msg/PollModal';
import { commandSuggestions, runSlash, type SlashContext } from './msg/slashCommands';
import { UploadTray } from './msg/UploadTray';
import { ComposerBackdrop, backdropActive } from './ComposerBackdrop';
import { isPatron } from './msg/uploadLimit';
import { queuedFiles } from './queuedFiles';
import { formatBytes } from './msg/format';
import {
  discardChatUpload,
  MAX_FILES,
  reattachUploads,
  uploadFiles,
  type PendingFile,
} from './msg/upload';
import { imageMeta } from './msg/blurhash';
import { VoiceRecorder, type VoiceClip } from './msg/VoiceRecorder';
import { VOICE_FLAG, voiceMimeType } from './msg/voice';
import { createDm } from './shell/api';
import { rememberDm } from './shell/dm';
import { useSaved } from './shell/savedMessages';
import { useLoungeUi } from './store';
import './msg/msg.css';

/** Shortcodes become emoji at send time (Q89): the parser leaves `:name:` literal. */
function expandForSend(text: string, then: (content: string) => void) {
  const engine: MarkdownEngine | null = currentMarkdownEngine();
  if (usePrefsStore.getState().prefs.convertEmoticons) text = convertEmoticons(text);
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

export interface ComposerHandle {
  /** Queue files from a drop or paste. */
  addFiles: (files: File[]) => void;
}

export interface ComposerProps {
  channelId: Snowflake;
  /** Undefined in DMs; mention and channel autocomplete read this guild. */
  guildId?: Snowflake | undefined;
  /** "#general" or a person's name; used in the placeholder. */
  target: string;
  canSend: boolean;
  /** Replaces "no permission" when the channel is read-only by design (rules, announcements). */
  readOnlyNote?: string | undefined;
  replyTo: Message | null;
  onCancelReply: () => void;
  /** ArrowUp in an empty composer: edit my last message. */
  onEditLast: () => void;
  /** Seconds between messages for members without Manage Messages. */
  slowmodeSeconds?: number | undefined;
  /** The viewer is exempt from slowmode. */
  bypassSlowmode?: boolean | undefined;
  /** May use @everyone / @here. */
  canMentionEveryone?: boolean | undefined;
  /** ISO end of the viewer's timeout in this server (they cannot send until then). */
  timedOutUntil?: string | null | undefined;
  /** May send /tts messages here. */
  canTts?: boolean | undefined;
  /** May start a thread in this channel (not in a DM or inside a thread). */
  canStartThread?: boolean | undefined;
  /** May offer "Start an Activity" here: a guild text channel (the server still checks the permission). */
  canStartActivity?: boolean | undefined;
  /** The channel grants USE_EXTERNAL_EMOJIS / USE_EXTERNAL_STICKERS (D-376); true in a DM. */
  canExternalEmoji?: boolean | undefined;
  canExternalStickers?: boolean | undefined;
}

const FORMATS: {
  id: string;
  labelKey: string;
  run: (v: string, s: number, e: number) => TextEdit;
}[] = [
  {
    id: 'bold',
    labelKey: 'lounge.composer.formatBold',
    run: (v, s, e) => toggleWrap(v, s, e, '**'),
  },
  {
    id: 'italic',
    labelKey: 'lounge.composer.formatItalic',
    run: (v, s, e) => toggleWrap(v, s, e, '*'),
  },
  {
    id: 'underline',
    labelKey: 'lounge.composer.formatUnderline',
    run: (v, s, e) => toggleWrap(v, s, e, '__'),
  },
  {
    id: 'strikethrough',
    labelKey: 'lounge.composer.formatStrikethrough',
    run: (v, s, e) => toggleWrap(v, s, e, '~~'),
  },
  {
    id: 'spoiler',
    labelKey: 'lounge.composer.formatSpoiler',
    run: (v, s, e) => toggleWrap(v, s, e, '||'),
  },
  {
    id: 'code',
    labelKey: 'lounge.composer.formatCode',
    run: (v, s, e) => toggleWrap(v, s, e, '`'),
  },
  {
    id: 'quote',
    labelKey: 'lounge.composer.formatQuote',
    run: (v, s, e) => prefixLines(v, s, e, '> '),
  },
  {
    id: 'list',
    labelKey: 'lounge.composer.formatList',
    run: (v, s, e) => prefixLines(v, s, e, '- '),
  },
];
const FORMAT_GLYPH: Record<string, string> = {
  bold: 'B',
  italic: 'I',
  underline: 'U',
  strikethrough: 'S',
  spoiler: '▒',
  code: '<>',
  quote: '❝',
  list: '•',
};

let fileCounter = 0;

export const Composer = forwardRef<ComposerHandle, ComposerProps>(function Composer(
  {
    channelId,
    guildId,
    target,
    canSend,
    readOnlyNote,
    replyTo,
    onCancelReply,
    onEditLast,
    slowmodeSeconds,
    bypassSlowmode,
    canMentionEveryone,
    timedOutUntil,
    canTts,
    canStartThread,
    canStartActivity,
    canExternalEmoji = true,
    canExternalStickers = true,
  },
  handle,
) {
  const t = useT();
  const savedPrefs = useSavedPrefs();
  const actions = useLoungeActions();
  const me = useCurrentUser();
  const users = useUsers();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  // In a channel the bridge carries our words to Discord, its members can be mentioned (owner direction).
  const bridgeDirection = useStore((s) => s.channels[channelId]?.sr_bridge?.direction);
  const bridgedAll = useGuildBridgedMembers(guildId);
  const bridgedMentions = useMemo(
    () =>
      bridgeDirection && bridgeDirection !== 'from_discord'
        ? bridgedAll.map((b) => ({ id: b.id, name: b.display_name, color: bridgedColor(b) }))
        : undefined,
    [bridgeDirection, bridgedAll],
  );
  const channels = useChannels(guildId);
  const typingIds = useTyping(channelId);
  const toast = useToast();
  const ref = useRef<HTMLTextAreaElement | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const acId = useId();
  const [text, setText] = useState(() => drafts.get(channelId) ?? '');
  const pendingSel = useRef<{ s: number; e: number } | null>(null);
  const lastTyping = useRef(0);
  const tokens = useRef(new Map<string, string>());

  const [files, setFiles] = useState<PendingFile[]>([]);
  const [sending, setSending] = useState(false);
  const abort = useRef<AbortController | null>(null);
  const [pollOpen, setPollOpen] = useState(false);
  // Slash commands of installed apps (DEV-BOT-008).
  const appIndex = useAppCommands(channelId, !!guildId);
  const activitiesOn = useActivitiesEnabled();
  const appCommands = useMemo(() => indexedCommands(appIndex, 1), [appIndex]);
  const reportOutcome = useReportOutcome();
  const [appForm, setAppForm] = useState<{ entry: IndexedCommand; prefill: TypedArgs } | null>(
    null,
  );
  const [ping, setPing] = useState(true);
  const [hasSel, setHasSel] = useState(false);
  const [caret, setCaret] = useState(0);
  const [acIndex, setAcIndex] = useState(0);
  const [acDismissed, setAcDismissed] = useState<number | null>(null);
  const [cooldownEnd, setCooldownEnd] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const [recording, setRecording] = useState(false);
  const voiceStop = useRef<(() => void) | null>(null);
  const openDialog = useLoungeUi((s) => s.openDialog);
  const canVoice = voiceMimeType() !== null;
  const slashCtx: SlashContext = {
    guild: !!guildId,
    threads: !!canStartThread,
    tts: !!canTts,
  };

  useEffect(() => {
    if (replyTo) {
      ref.current?.focus();
      setPing(true);
    }
  }, [replyTo]);

  // Slowmode countdown (LNG-CMP-010).
  const cooldownLeft = Math.max(0, Math.ceil((cooldownEnd - now) / 1000));
  useEffect(() => {
    if (cooldownEnd <= Date.now()) return;
    const timer = setInterval(() => {
      setNow(Date.now());
      if (Date.now() >= cooldownEnd) clearInterval(timer);
    }, 250);
    return () => clearInterval(timer);
  }, [cooldownEnd]);

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
      setCaret(edit.selEnd);
      drafts.set(channelId, edit.value);
    },
    [channelId],
  );

  const sel = () => {
    const el = ref.current;
    return { s: el?.selectionStart ?? text.length, e: el?.selectionEnd ?? text.length };
  };

  // ---- files ----
  // The size the server will accept for this viewer here (D-233); unknown until the guild or DM list is read.
  const maxUpload = useStore((s) =>
    guildId ? s.guilds[guildId]?.sr_max_upload_bytes : s.channels[channelId]?.sr_max_upload_bytes,
  );
  const addFiles = useCallback(
    (all: File[]) => {
      // Block oversized files before they upload (the server's 413 stays the real limit).
      const list = maxUpload === undefined ? all : all.filter((f) => f.size <= maxUpload);
      const refused = all.filter((f) => !list.includes(f));
      if (refused.length > 0) {
        const values = {
          name: refused[0]!.name || 'file',
          count: refused.length,
          max: formatBytes(maxUpload ?? 0),
        };
        const show = (patron: boolean) =>
          toast.show({
            title: t(
              patron ? 'lounge.composer.fileTooLarge' : 'lounge.composer.fileTooLargePatron',
              values,
            ),
            variant: 'warning',
            duration: 5000,
          });
        // Free members get the Patron hint; the lookup is shared with the tray and shows the plain text offline.
        void isPatron().then(show, () => show(true));
      }
      if (list.length === 0) return;
      setFiles((cur) => {
        const room = MAX_FILES - cur.length;
        if (list.length > room) {
          toast.show({
            title: t('lounge.composer.tooManyFiles', { max: MAX_FILES }),
            variant: 'info',
            duration: 3000,
          });
        }
        const added = list.slice(0, Math.max(0, room)).map((file): PendingFile => ({
          id: `f${Date.now().toString(36)}${fileCounter++}`,
          file,
          name: file.name || 'file',
          spoiler: false,
          description: '',
          progress: 0,
          status: 'queued',
        }));
        // Pixel size and BlurHash are read in the background; a send before they are ready goes without.
        for (const f of added)
          void imageMeta(f.file).then((meta) => {
            if (meta) setFiles((now) => now.map((x) => (x.id === f.id ? { ...x, meta } : x)));
          });
        return [...cur, ...added];
      });
    },
    [toast, t, maxUpload],
  );
  useImperativeHandle(handle, () => ({ addFiles }), [addFiles]);
  // Files shared into this channel from another app (Share Target, G-18) arrive as attachments.
  useEffect(() => {
    addFiles(queuedFiles.take(channelId));
  }, [channelId, addFiles]);

  const patchFile = useCallback((id: string, patch: Partial<PendingFile>) => {
    setFiles((cur) => cur.map((f) => (f.id === id ? { ...f, ...patch } : f)));
  }, []);

  // A multipart upload that a reload interrupted comes back into the tray and continues (D-271).
  useEffect(() => {
    const ctl = new AbortController();
    const ids = new Set<string>();
    void reattachUploads(channelId, {
      signal: ctl.signal,
      onUpdate: patchFile,
      onAdd: (found) => {
        for (const f of found) ids.add(f.id);
        setFiles((cur) => [...cur.filter((x) => !ids.has(x.id)), ...found].slice(0, MAX_FILES));
      },
    });
    return () => {
      ctl.abort();
      setFiles((cur) => cur.filter((f) => !ids.has(f.id)));
    };
  }, [channelId, patchFile]);
  const resuming = !sending && files.some((f) => f.resume && f.status === 'uploading');

  const removeFile = (id: string) => {
    if (sending) abort.current?.abort();
    const gone = files.find((f) => f.id === id);
    if (gone?.resume) discardChatUpload(channelId, gone.resume.uploadId);
    setFiles((cur) => cur.filter((f) => f.id !== id));
  };

  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = Array.from(e.clipboardData?.files ?? []);
    if (pasted.length) {
      e.preventDefault();
      addFiles(pasted);
    }
  };

  // ---- sending ----
  const finishSend = () => {
    drafts.delete(channelId);
    setText('');
    setCaret(0);
    tokens.current.clear();
    setFiles([]);
    onCancelReply();
    if (slowmodeSeconds && !bypassSlowmode) {
      const end = Date.now() + slowmodeSeconds * 1000;
      setCooldownEnd(end);
      setNow(Date.now());
    }
  };

  const submit = (content: string, poll?: NewPoll, more: { tts?: boolean } = {}) => {
    const base = {
      ...(replyTo && !ping ? { mentionRepliedUser: false } : {}),
      ...(poll ? { poll } : {}),
      ...(more.tts ? { tts: true } : {}),
    };
    if (files.length === 0) {
      actions.sendMessage(
        channelId,
        content,
        replyTo?.id ?? null,
        Object.keys(base).length ? base : undefined,
      );
      finishSend();
      return;
    }
    const ctl = new AbortController();
    abort.current = ctl;
    setSending(true);
    uploadFiles(channelId, files, { onUpdate: patchFile, signal: ctl.signal })
      .then((refs) => {
        actions.sendMessage(channelId, content, replyTo?.id ?? null, {
          ...base,
          attachments: refs,
        });
        // The tray is cleared in the same update as each file's 'done', so it never sees that state and
        // cannot announce it (the CI run of keyboard flow 5): say it here, once per uploaded file.
        for (const f of files) announce(t('a11y.announce.uploadDone', { name: f.name }));
        finishSend();
      })
      .catch((e: unknown) => {
        if (ctl.signal.aborted) return;
        const msg = e instanceof Error ? e.message : t('lounge.composer.uploadFailed');
        toast.show({ title: msg, variant: 'danger', duration: 5000 });
      })
      .finally(() => {
        setSending(false);
        abort.current = null;
      });
  };

  const [scheduleOpen, setScheduleOpen] = useState(false);
  const showSendButton = useSavedPrefs().showSendButton;
  const send = () => {
    if (sending || resuming || cooldownLeft > 0) return;
    let content = resolveTokens(text.trim(), tokens.current);
    if (!content && files.length === 0) return;
    if (content.length > MAX_MESSAGE_LENGTH) return;
    let tts = false;
    if (content.startsWith('/')) {
      const r = runSlash(content, slashCtx);
      if (r.kind === 'poll') {
        setPollOpen(true);
        return;
      }
      if (r.kind === 'error') {
        toast.show({ title: r.message, variant: 'danger', duration: 3000 });
        return;
      }
      if (r.kind === 'send') {
        content = r.content;
        tts = !!r.tts;
      } else if (r.kind !== 'none') {
        runAction(r);
        return;
      } else if (guildId) {
        const typed = matchTyped(content, appCommands);
        if (typed) {
          runAppCommand(typed.entry, parseTypedArgs(typed.entry.command, typed.rest));
          return;
        }
      }
    }
    expandForSend(content, (c) => submit(c, undefined, { tts }));
  };

  /** An app command typed in the box: commands without options run at once, the rest open the option form. */
  const runAppCommand = (entry: IndexedCommand, prefill: TypedArgs) => {
    drafts.delete(channelId);
    setText('');
    setCaret(0);
    tokens.current.clear();
    if (!guildId) return;
    if (!runsImmediately(entry.command)) {
      setAppForm({ entry, prefill });
      return;
    }
    void sendInteraction({
      type: 2,
      application_id: entry.command.application_id,
      channel_id: channelId,
      guild_id: guildId,
      nonce: newNonce(),
      data: { id: entry.command.id, options: [] },
    }).then(reportOutcome);
  };

  /** Commands that do something other than post their text. The box is cleared once they are accepted. */
  const runAction = (
    r: Exclude<ReturnType<typeof runSlash>, { kind: 'send' | 'poll' | 'none' | 'error' }>,
  ) => {
    const done = () => {
      drafts.delete(channelId);
      setText('');
      setCaret(0);
      tokens.current.clear();
    };
    const fail = (title: string) => toast.show({ title, variant: 'danger', duration: 4000 });
    switch (r.kind) {
      case 'nick':
        if (!guildId) return;
        apiFetch<unknown>(`/guilds/${guildId}/members/${me.id}`, {
          method: 'PATCH',
          json: { nick: r.nick },
          retries: 0,
        })
          .then(() => {
            useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
              guild_id: guildId,
              user: { id: me.id },
              nick: r.nick,
              roles: members.find((m) => m.user.id === me.id)?.roleIds ?? [],
            });
            toast.show({
              title: r.nick
                ? t('lounge.composer.nickSet', { nick: r.nick })
                : t('lounge.composer.nickReset'),
              variant: 'success',
              duration: 3000,
            });
          })
          .catch((e: unknown) =>
            fail(
              e instanceof ApiError && e.status === 403
                ? t('lounge.composer.nickForbidden')
                : t('lounge.composer.nickFailed'),
            ),
          );
        done();
        return;
      case 'thread':
        if (guildId) openDialog({ kind: 'createThread', guildId, channelId });
        done();
        return;
      case 'community':
        if (guildId) openDialog({ kind: 'community', guildId, tab: r.tab });
        done();
        return;
      case 'msg':
        createDm([r.userId])
          .then((dm) => {
            rememberDm(dm);
            actions.sendMessage(dm.id, r.content, null);
            toast.show({ title: t('lounge.composer.dmSent'), variant: 'success', duration: 2500 });
          })
          .catch(() => fail(t('lounge.composer.dmFailed')));
        done();
        return;
      case 'roll':
        // Dice are in WebAssembly (loaded on first use); the result is posted as an ordinary message.
        void import('../tools/dice/engine').then(async ({ rollMessage, rollErrorKey }) => {
          const out = await rollMessage(r.expression);
          if (!out.ok) {
            fail(t(rollErrorKey(out.code)));
            return;
          }
          actions.sendMessage(channelId, out.content, replyTo?.id ?? null);
          onCancelReply();
        });
        done();
        return;
      case 'remind': {
        const at = new Date(Date.now() + r.ms);
        useSaved
          .getState()
          .addReminder({ note: r.text, remindAt: at })
          .then(() =>
            toast.show({
              title: t('lounge.composer.remindAt', {
                time: formatTime(at, { hour: 'numeric', minute: '2-digit' }),
              }),
              variant: 'success',
              duration: 3000,
            }),
          )
          .catch(() => fail(t('lounge.composer.remindFailed')));
        done();
        return;
      }
    }
  };

  const sendSticker = (s: GuildSticker) => {
    actions.sendMessage(channelId, '', replyTo?.id ?? null, {
      stickerIds: [s.id],
      ...(replyTo && !ping ? { mentionRepliedUser: false } : {}),
    });
    onCancelReply();
    if (slowmodeSeconds && !bypassSlowmode) {
      setCooldownEnd(Date.now() + slowmodeSeconds * 1000);
      setNow(Date.now());
    }
  };

  const sendGif = (g: { url: string }) => {
    actions.sendMessage(channelId, g.url, replyTo?.id ?? null, {
      ...(replyTo && !ping ? { mentionRepliedUser: false } : {}),
    });
    onCancelReply();
    if (slowmodeSeconds && !bypassSlowmode) {
      setCooldownEnd(Date.now() + slowmodeSeconds * 1000);
      setNow(Date.now());
    }
  };

  const sendVoice = (clip: VoiceClip) => {
    const pending: PendingFile = {
      id: `f${Date.now().toString(36)}${fileCounter++}`,
      file: clip.file,
      name: clip.file.name,
      spoiler: false,
      description: '',
      progress: 0,
      status: 'queued',
    };
    const ctl = new AbortController();
    abort.current = ctl;
    setSending(true);
    uploadFiles(channelId, [pending], { onUpdate: () => undefined, signal: ctl.signal })
      .then(([ref]) => {
        if (!ref) return;
        actions.sendMessage(channelId, '', replyTo?.id ?? null, {
          flags: VOICE_FLAG,
          attachments: [{ ...ref, durationSecs: clip.durationSecs, waveform: clip.waveform }],
        });
        setRecording(false);
        onCancelReply();
      })
      .catch((e: unknown) => {
        if (ctl.signal.aborted) return;
        toast.show({
          title: e instanceof Error ? e.message : t('lounge.composer.voiceFailed'),
          variant: 'danger',
          duration: 5000,
        });
      })
      .finally(() => {
        setSending(false);
        abort.current = null;
      });
  };

  const sendAsFile = () => {
    const body = text.trim();
    if (!body) return;
    const file = new File([body], 'message.txt', { type: 'text/plain' });
    setText('');
    drafts.delete(channelId);
    setFiles((cur) => [
      ...cur,
      {
        id: `f${Date.now().toString(36)}${fileCounter++}`,
        file,
        name: 'message.txt',
        spoiler: false,
        description: '',
        progress: 0,
        status: 'queued',
      },
    ]);
  };

  // ---- autocomplete ----
  const trigger: Trigger | null =
    !acDismissed || acDismissed !== caret ? findTrigger(text, caret) : null;
  let suggestions: Suggestion[] = [];
  let acTitle = '';
  if (trigger?.kind === '@') {
    suggestions = mentionSuggestions(trigger.query, members, roles, {
      everyone: !!canMentionEveryone,
      bridged: bridgedMentions,
    });
    acTitle = t('lounge.composer.acMembers');
  } else if (trigger?.kind === '#') {
    suggestions = channelSuggestions(trigger.query, channels);
    acTitle = t('lounge.composer.acChannels');
  } else if (trigger?.kind === ':') {
    suggestions = emojiSuggestions(
      trigger.query,
      EMOJI_CATEGORIES.flatMap((c) => c.emoji),
    );
    acTitle = t('lounge.composer.acEmoji');
  } else if (trigger?.kind === '/') {
    suggestions = [
      ...commandSuggestions(trigger.query, slashCtx),
      ...searchCommands(appCommands, trigger.query).map(({ command, app }) => ({
        key: `app:${command.id}`,
        label: `/${command.name}`,
        hint: command.description
          ? t('lounge.apps.picker.hint', { app: app.name, description: command.description })
          : t('lounge.apps.picker.hintNoDescription', { app: app.name }),
        insert: `/${command.name} `,
      })),
    ];
    acTitle = t('lounge.composer.acCommands');
  }
  const acOpen = suggestions.length > 0;
  const activeIdx = Math.min(acIndex, Math.max(0, suggestions.length - 1));

  const pick = (s: Suggestion) => {
    if (!trigger) return;
    if (s.token) tokens.current.set(s.insert, s.token);
    const r = applySuggestion(text, trigger, caret, s);
    apply({ value: r.value, selStart: r.caret, selEnd: r.caret });
    setAcIndex(0);
    ref.current?.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const mod = e.ctrlKey || e.metaKey;
    if (acOpen) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const n = suggestions.length;
        setAcIndex((activeIdx + (e.key === 'ArrowDown' ? 1 : n - 1)) % n);
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        pick(suggestions[activeIdx]!);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setAcDismissed(caret);
        return;
      }
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
      return;
    }
    if (e.key === 'Escape') {
      if (replyTo) {
        e.preventDefault();
        e.stopPropagation();
        onCancelReply();
      }
      return;
    }
    if (e.key === 'ArrowUp' && text === '' && files.length === 0) {
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
    setCaret(e.target.selectionStart);
    setAcIndex(0);
    drafts.set(channelId, e.target.value);
    const now = Date.now();
    if (e.target.value && now - lastTyping.current > 8000) {
      lastTyping.current = now;
      actions.startTyping(channelId);
    }
  };

  const trackSelection = () => {
    const el = ref.current;
    if (!el) return;
    setCaret(el.selectionStart);
    setHasSel(el.selectionEnd > el.selectionStart);
  };

  const typingNames = typingIds
    .filter((id) => id !== me.id)
    .map(
      (id) =>
        // A Discord typer of this server first: its id is never resolved to an account here.
        (guildId ? findBridgedMember(id, guildId)?.display_name : undefined) ??
        users[id]?.displayName ??
        findBridgedMember(id)?.display_name ??
        t('lounge.composer.someone'),
    );
  const typing = typingLabel(typingNames);
  const remaining = MAX_MESSAGE_LENGTH - text.length;
  const showCounter = remaining <= 200;
  const hasContent = !!text.trim() || files.length > 0;

  if (!canSend || timedOutUntil) {
    return (
      <div className="lng-composer-wrap">
        <div className="lng-composer lng-composer-locked" role="note">
          <Icon name={timedOutUntil ? 'clock' : 'lock'} size={20} />
          {timedOutUntil
            ? t('lounge.composer.timedOut', {
                date: formatDateTime(new Date(timedOutUntil), {
                  dateStyle: 'medium',
                  timeStyle: 'short',
                }),
              })
            : (readOnlyNote ?? t('lounge.composer.noPermission'))}
        </div>
      </div>
    );
  }

  return (
    <div className="lng-composer-wrap" data-drop-target>
      {replyTo && (
        <div className="lng-bar" role="status">
          <span>
            {t('lounge.composer.replyingTo')}
            <strong>{replyTo.author.displayName}</strong>
          </span>
          <span className="lng-bar-actions">
            <button
              type="button"
              className="lng-ping"
              aria-pressed={ping}
              aria-label={t(ping ? 'lounge.composer.mentionOn' : 'lounge.composer.mentionOff', {
                name: replyTo.author.displayName,
              })}
              onClick={() => setPing((p) => !p)}
            >
              {ping ? t('lounge.composer.pingOn') : t('lounge.composer.pingOff')}
            </button>
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={t('lounge.composer.cancelReply')}
              onClick={onCancelReply}
            >
              <Icon name="close" size={16} />
            </button>
          </span>
        </div>
      )}
      <UploadTray files={files} busy={sending} onChange={patchFile} onRemove={removeFile} />
      {savedPrefs.stickerSuggestions && !acOpen && !files.length && (
        <StickerSuggestions guildId={guildId} text={text} onPick={sendSticker} />
      )}
      {savedPrefs.markdownPreview && <MarkdownPreview text={text} />}
      {hasSel && (
        <div className="lng-format" role="toolbar" aria-label={t('lounge.composer.textFormatting')}>
          {FORMATS.map((f) => (
            <button
              key={f.id}
              type="button"
              className="lng-format-btn"
              aria-label={t(f.labelKey)}
              // Keep the selection in the textarea while the button is pressed.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                const { s, e } = sel();
                apply(f.run(text, s, e));
                ref.current?.focus();
              }}
            >
              <span aria-hidden="true">{FORMAT_GLYPH[f.id]}</span>
            </button>
          ))}
        </div>
      )}
      <div className="lng-composer">
        {acOpen && (
          <AutocompletePopup
            id={acId}
            suggestions={suggestions}
            active={activeIdx}
            title={acTitle}
            onPick={pick}
            onHover={setAcIndex}
          />
        )}
        <DropdownMenu
          label={t('lounge.composer.addToMessage')}
          placement="top-start"
          items={[
            { label: t('lounge.composer.uploadFile'), onSelect: () => fileInput.current?.click() },
            ...(canStartThread && guildId
              ? [
                  {
                    label: t('lounge.composer.createThread'),
                    onSelect: () => openDialog({ kind: 'createThread', guildId, channelId }),
                  },
                ]
              : []),
            { label: t('lounge.composer.createPoll'), onSelect: () => setPollOpen(true) },
            ...(canStartActivity && activitiesOn
              ? [
                  {
                    label: t('lounge.composer.startActivity'),
                    onSelect: () => openDialog({ kind: 'startActivity', channelId }),
                  },
                ]
              : []),
            { type: 'separator' as const },
            { label: t('lounge.composer.useApps'), disabled: true },
            { label: t('lounge.composer.shareCinema'), disabled: true },
            { label: t('lounge.composer.shareLibrary'), disabled: true },
          ]}
          trigger={
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={t('lounge.composer.addToMessage')}
            >
              <Icon name="plus" size={20} />
            </button>
          }
        />
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          aria-label={t('lounge.composer.uploadFiles')}
          data-testid="file-input"
          onChange={(e) => {
            addFiles(Array.from(e.target.files ?? []));
            e.target.value = '';
          }}
        />
        {recording ? (
          <VoiceRecorder
            stopRef={voiceStop}
            onCancel={() => setRecording(false)}
            onSend={sendVoice}
          />
        ) : (
          <div className="lng-composer-field">
            {backdropActive(text) && <ComposerBackdrop text={text} target={ref} />}
            <textarea
              ref={ref}
              className="lng-textarea"
              data-backdrop={backdropActive(text) || undefined}
              data-composer
              rows={1}
              aria-label={t('lounge.composer.message', { target })}
              aria-controls={acOpen ? acId : undefined}
              aria-activedescendant={acOpen ? `${acId}-${activeIdx}` : undefined}
              placeholder={
                cooldownLeft > 0
                  ? t('lounge.composer.slowmodePlaceholder', { seconds: cooldownLeft })
                  : t('lounge.composer.message', { target })
              }
              value={text}
              onChange={onChange}
              onKeyDown={onKeyDown}
              onKeyUp={trackSelection}
              onSelect={trackSelection}
              onClick={trackSelection}
              onPaste={onPaste}
            />
          </div>
        )}
        {showCounter && !recording && (
          <span
            className="lng-counter"
            data-over={remaining < 0 || undefined}
            role="status"
            aria-label={t('lounge.composer.charsRemaining', { count: remaining })}
          >
            {remaining}
          </span>
        )}
        {canVoice && !recording && (
          <Tooltip content={t('lounge.composer.recordVoice')}>
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={t('lounge.composer.recordVoice')}
              onClick={(e) => {
                // Touch: hold to record (below); mouse and keyboard: click to start.
                if (e.detail === 0 || (e.nativeEvent as PointerEvent).pointerType !== 'touch')
                  setRecording(true);
              }}
              onPointerDown={(e) => {
                if (e.pointerType === 'touch') setRecording(true);
              }}
              onPointerUp={(e) => {
                if (e.pointerType === 'touch') voiceStop.current?.();
              }}
            >
              <Icon name="mic" size={20} />
            </button>
          </Tooltip>
        )}
        {!recording && (
          <>
            <Popover
              label={t('lounge.composer.gifPicker')}
              placement="top-end"
              trigger={
                <button
                  type="button"
                  className="lng-icon-btn"
                  aria-label={t('lounge.composer.openGifPicker')}
                >
                  <span aria-hidden="true" className="lng-gif-glyph">
                    {t('lounge.composer.gifGlyph')}
                  </span>
                </button>
              }
            >
              <ExpressionPicker
                initial="gifs"
                guildId={guildId}
                externalEmoji={canExternalEmoji}
                externalStickers={canExternalStickers}
                onEmoji={(emoji) => {
                  const { s, e } = sel();
                  apply(insertAt(text, s, e, emoji));
                  ref.current?.focus();
                }}
                onSticker={sendSticker}
                onGif={sendGif}
                onCustomEmoji={(custom) => {
                  const { s, e } = sel();
                  apply(insertAt(text, s, e, customEmojiText(custom)));
                  ref.current?.focus();
                }}
              />
            </Popover>
            <Popover
              label={t('lounge.composer.stickerPicker')}
              placement="top-end"
              trigger={
                <button
                  type="button"
                  className="lng-icon-btn"
                  aria-label={t('lounge.composer.openStickerPicker')}
                >
                  <Icon name="image" size={20} />
                </button>
              }
            >
              <ExpressionPicker
                initial="stickers"
                guildId={guildId}
                externalEmoji={canExternalEmoji}
                externalStickers={canExternalStickers}
                onEmoji={(emoji) => {
                  const { s, e } = sel();
                  apply(insertAt(text, s, e, emoji));
                  ref.current?.focus();
                }}
                onSticker={sendSticker}
                onGif={sendGif}
                onCustomEmoji={(custom) => {
                  const { s, e } = sel();
                  apply(insertAt(text, s, e, customEmojiText(custom)));
                  ref.current?.focus();
                }}
              />
            </Popover>
          </>
        )}
        <Tooltip content={t('lounge.composer.schedule')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.composer.schedule')}
            disabled={!hasContent || text.trim().startsWith('/')}
            onClick={() => setScheduleOpen(true)}
          >
            <Icon name="clock" size={20} />
          </button>
        </Tooltip>
        <Popover
          label={t('lounge.composer.emojiPicker')}
          placement="top-end"
          trigger={
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={t('lounge.composer.openEmojiPicker')}
            >
              <Icon name="smile" size={20} />
            </button>
          }
        >
          <EmojiPicker
            guildId={guildId}
            externalAllowed={canExternalEmoji}
            onPick={(emoji) => {
              const { s, e } = sel();
              apply(insertAt(text, s, e, emoji));
              ref.current?.focus();
            }}
            onPickCustom={(custom) => {
              const { s, e } = sel();
              apply(insertAt(text, s, e, customEmojiText(custom)));
              ref.current?.focus();
            }}
          />
        </Popover>
        {showSendButton && (
          <Tooltip content={t('lounge.composer.send')}>
            <button
              type="button"
              className="lng-icon-btn lng-send"
              aria-label={t('lounge.composer.send')}
              disabled={!hasContent || remaining < 0 || sending || resuming || cooldownLeft > 0}
              onClick={send}
            >
              <Icon name="arrow-right" size={20} />
            </button>
          </Tooltip>
        )}
      </div>
      {remaining < 0 && (
        <p className="lng-over" role="alert">
          {t('lounge.composer.overLimit', { count: -remaining })}{' '}
          <Button variant="link" size="sm" onClick={sendAsFile}>
            {t('lounge.composer.uploadAsFile')}
          </Button>
        </p>
      )}
      {cooldownLeft > 0 && (
        <p className="lng-muted lng-slow" role="status">
          {t('lounge.composer.slowmodeNote', { seconds: cooldownLeft })}
        </p>
      )}
      <div className="lng-typing" aria-live="polite">
        {typing && (
          <>
            <span className="lng-typing-dots" aria-hidden="true">
              <span />
              <span />
              <span />
            </span>
            {typing}
          </>
        )}
      </div>
      <ScheduleDialog
        open={scheduleOpen}
        channelId={channelId}
        content={resolveTokens(text.trim(), tokens.current)}
        onClose={() => setScheduleOpen(false)}
        onScheduled={() => {
          setScheduleOpen(false);
          finishSend();
        }}
      />
      {appForm && guildId && (
        <CommandForm
          entry={appForm.entry}
          prefill={appForm.prefill}
          channelId={channelId}
          guildId={guildId}
          onClose={() => setAppForm(null)}
        />
      )}
      <PollModal
        open={pollOpen}
        guildId={guildId}
        onClose={() => setPollOpen(false)}
        onCreate={(poll) => {
          setPollOpen(false);
          // The text box becomes the message that accompanies the poll.
          const content = resolveTokens(text.trim(), tokens.current);
          submit(content.startsWith('/poll') ? '' : content, poll);
        }}
      />
    </div>
  );
});
