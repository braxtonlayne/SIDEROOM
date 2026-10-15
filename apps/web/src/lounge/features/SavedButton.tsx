import { Icon, Popover, Tabs } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { formatDateTime, useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeGoto, useLoungeGotoMessage } from '../nav';
import { useSaved } from '../shell/savedMessages';
import { editScheduled, listBookmarks, type BookmarkEntry } from './api';
import './features.css';

type Jump = (guild: string | null, channel: string, message?: string) => void;

function Bookmarks({ onJump }: { onJump: Jump }) {
  const t = useT();
  const marks = useSaved((s) => s.bookmarks);
  const toggle = useSaved((s) => s.toggleBookmark);
  const setNote = useSaved((s) => s.setBookmarkNote);
  const [entries, setEntries] = useState<BookmarkEntry[] | null>(null);
  const [failed, setFailed] = useState(false);
  const count = Object.keys(marks).length;

  // Reload when the set changes (a bookmark added elsewhere, or removed here).
  useEffect(() => {
    let live = true;
    listBookmarks({ limit: 50 })
      .then((list) => {
        if (live) setEntries(list);
      })
      .catch(() => {
        if (live) setFailed(true);
      });
    return () => {
      live = false;
    };
  }, [count]);

  if (failed)
    return (
      <p className="lng-muted" role="alert">
        {t('lounge.saved.error')}
      </p>
    );
  if (entries === null)
    return (
      <p className="lng-muted" role="status">
        {t('lounge.saved.loading')}
      </p>
    );
  if (entries.length === 0) return <p className="lng-muted">{t('lounge.saved.bookmarksEmpty')}</p>;
  return (
    <ul className="lng-saved-list">
      {entries.map(({ bookmark: b, message: m }) => (
        <li key={b.message_id} className="lng-saved-item">
          <div className="lng-saved-head">
            <strong>
              {m ? (m.author.global_name ?? m.author.username) : t('lounge.saved.unavailable')}
            </strong>
            {m && <span className="lng-muted">{formatDateTime(new Date(m.timestamp))}</span>}
          </div>
          {m && (
            <p className="lng-saved-body">
              {m.content.trim() ||
                (m.attachments?.length ? t('lounge.saved.attachment') : t('lounge.saved.noText'))}
            </p>
          )}
          <label className="lng-saved-note">
            <span className="lng-muted">{t('lounge.saved.note')}</span>
            <input
              defaultValue={b.note}
              maxLength={500}
              aria-label={t('lounge.saved.noteLabel')}
              placeholder={t('lounge.saved.notePlaceholder')}
              onBlur={(e) => {
                if (e.target.value.trim() !== b.note)
                  void setNote(b.message_id, e.target.value.trim());
              }}
            />
          </label>
          <div className="lng-saved-actions">
            {m && (
              <button
                type="button"
                className="lng-link-btn"
                onClick={() => onJump(b.guild_id, b.channel_id, b.message_id)}
              >
                {t('lounge.saved.jump')}
              </button>
            )}
            <button
              type="button"
              className="lng-link-btn"
              onClick={() =>
                void toggle({
                  message_id: b.message_id,
                  channel_id: b.channel_id,
                  guild_id: b.guild_id,
                  preview: '',
                })
              }
            >
              {t('lounge.saved.remove')}
            </button>
          </div>
        </li>
      ))}
    </ul>
  );
}

function Reminders({ onJump }: { onJump: Jump }) {
  const t = useT();
  const reminders = useSaved((s) => s.reminders);
  const cancel = useSaved((s) => s.cancelReminder);
  if (reminders.length === 0)
    return <p className="lng-muted">{t('lounge.saved.remindersEmpty')}</p>;
  return (
    <ul className="lng-saved-list">
      {reminders.map((r) => (
        <li key={r.id} className="lng-saved-item">
          <div className="lng-saved-head">
            <strong>{formatDateTime(new Date(r.remind_at))}</strong>
          </div>
          <p className="lng-saved-body">{r.note || t('lounge.saved.reminderNoNote')}</p>
          <div className="lng-saved-actions">
            {r.channel_id && r.message_id && (
              <button
                type="button"
                className="lng-link-btn"
                onClick={() => onJump(r.guild_id, r.channel_id!, r.message_id!)}
              >
                {t('lounge.saved.jump')}
              </button>
            )}
            <button type="button" className="lng-link-btn" onClick={() => void cancel(r.id)}>
              {t('lounge.saved.cancel')}
            </button>
          </div>
        </li>
      ))}
    </ul>
  );
}

function Scheduled({ onJump }: { onJump: Jump }) {
  const t = useT();
  const scheduled = useSaved((s) => s.scheduled);
  const cancel = useSaved((s) => s.cancelScheduled);
  const replace = useSaved((s) => s.replaceScheduled);
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState(false);
  if (scheduled.length === 0)
    return <p className="lng-muted">{t('lounge.saved.scheduledEmpty')}</p>;
  const save = async (id: string) => {
    setError(false);
    try {
      replace(await editScheduled(id, { content: draft.trim() }));
      setEditing(null);
    } catch {
      setError(true);
    }
  };
  return (
    <ul className="lng-saved-list">
      {scheduled.map((r) => (
        <li key={r.id} className="lng-saved-item">
          <div className="lng-saved-head">
            <strong>{formatDateTime(new Date(r.send_at))}</strong>
          </div>
          {editing === r.id ? (
            <>
              <textarea
                className="lng-input"
                value={draft}
                maxLength={2000}
                rows={3}
                aria-label={t('lounge.saved.editScheduledLabel')}
                onChange={(e) => setDraft(e.target.value)}
              />
              {error && (
                <p role="alert" className="lng-saved-failed">
                  {t('lounge.saved.error')}
                </p>
              )}
              <div className="lng-saved-actions">
                <button
                  type="button"
                  className="lng-link-btn"
                  disabled={!draft.trim()}
                  onClick={() => void save(r.id)}
                >
                  {t('lounge.saved.save')}
                </button>
                <button type="button" className="lng-link-btn" onClick={() => setEditing(null)}>
                  {t('lounge.saved.discard')}
                </button>
              </div>
            </>
          ) : (
            <>
              <p className="lng-saved-body">{r.content}</p>
              <div className="lng-saved-actions">
                <button
                  type="button"
                  className="lng-link-btn"
                  onClick={() => {
                    setDraft(r.content);
                    setEditing(r.id);
                  }}
                >
                  {t('lounge.saved.edit')}
                </button>
                <button
                  type="button"
                  className="lng-link-btn"
                  onClick={() => onJump(r.guild_id, r.channel_id)}
                >
                  {t('lounge.saved.openChannel')}
                </button>
                <button type="button" className="lng-link-btn" onClick={() => void cancel(r.id)}>
                  {t('lounge.saved.cancel')}
                </button>
              </div>
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Header button: bookmarks (with notes), reminders and scheduled messages (FTR-BKM-001, FTR-REM-001, FTR-SCH-001). */
export function SavedButton() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const jumpToMessage = useLoungeGotoMessage();
  const jumpToChannel = useLoungeGoto();
  const reminders = useSaved((s) => s.reminders.length);
  const scheduled = useSaved((s) => s.scheduled.length);
  const goto: Jump = (guild, channel, message) => {
    setOpen(false);
    if (message) jumpToMessage(guild ?? HOME, channel, message);
    else jumpToChannel(guild ?? HOME, channel);
  };
  return (
    <Popover
      label={t('lounge.saved.title')}
      placement="bottom-end"
      onOpenChange={setOpen}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.saved.title')}
          title={t('lounge.saved.title')}
        >
          <Icon name="bookmark" size={20} />
        </button>
      }
    >
      <div className="lng-saved-pop">
        <h2 className="lng-pop-title">{t('lounge.saved.title')}</h2>
        {open && (
          <Tabs
            label={t('lounge.saved.tabs')}
            variant="pill"
            items={[
              {
                id: 'bookmarks',
                label: t('lounge.saved.bookmarks'),
                content: <Bookmarks onJump={goto} />,
              },
              {
                id: 'reminders',
                label: t('lounge.saved.reminders', { count: reminders }),
                content: <Reminders onJump={goto} />,
              },
              {
                id: 'scheduled',
                label: t('lounge.saved.scheduled', { count: scheduled }),
                content: <Scheduled onJump={goto} />,
              },
            ]}
          />
        )}
      </div>
    </Popover>
  );
}
