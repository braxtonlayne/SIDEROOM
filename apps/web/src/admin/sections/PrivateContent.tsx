// Private Content Viewer (ADM-COM-003): staff with READ_PRIVATE_CONTENT read a conversation after giving a
// reason. The server demands a fresh second factor (the re-auth dialog opens through `withReauth`), writes the
// access to the hash chain before it returns any content, and keeps deleted messages for the retention window.
import { Badge, Button, Notice, SegmentedControl, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../api';
import { ApiProblem, SectionHeader, errorText, fmtMs } from '../ui';

type Mode = 'channel' | 'account' | 'guild';

const isId = (v: string) => /^\d{1,20}$/.test(v);

function channelTitle(c: api.PrivateChannelRef): string {
  if (c.name) return c.name;
  if (c.recipients.length) return c.recipients.map((r) => `@${r.username}`).join(', ');
  return c.id;
}

export function PrivateContentSection() {
  const t = useT();
  const [mode, setMode] = useState<Mode>('channel');
  const [target, setTarget] = useState('');
  const [reason, setReason] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [channels, setChannels] = useState<api.PrivateChannelRef[] | null>(null);
  const [page, setPage] = useState<api.PrivateMessagePage | null>(null);
  const [items, setItems] = useState<api.PrivateMessage[]>([]);

  const why = reason.trim();
  const ready = isId(target.trim()) && why.length > 0 && !busy;
  const targetLabel =
    mode === 'channel'
      ? t('admin.privateContent.targetChannel')
      : mode === 'account'
        ? t('admin.privateContent.targetAccount')
        : t('admin.privateContent.targetGuild');

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorText(e, t('admin.privateContent.error')));
    } finally {
      setBusy(false);
    }
  };

  const readChannel = (id: string, before?: string | null) =>
    run(async () => {
      const p = await api.readPrivateMessages(id, why, before);
      setPage(p);
      setItems((old) => (before ? [...old, ...p.items] : p.items));
    });

  const open = () =>
    run(async () => {
      setPage(null);
      setItems([]);
      setChannels(null);
      const id = target.trim();
      if (mode === 'channel') {
        const p = await api.readPrivateMessages(id, why);
        setPage(p);
        setItems(p.items);
      } else if (mode === 'account') {
        setChannels(await api.listPrivateDms(id, why));
      } else {
        setChannels(await api.listPrivateGuildChannels(id, why));
      }
    });

  return (
    <>
      <SectionHeader
        title={t('admin.privateContent.title')}
        description={t('admin.privateContent.description')}
      />
      <Notice variant="warning" className="ss-status" role="note">
        {t('admin.privateContent.logged')}
      </Notice>
      <form
        className="adm-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) void open();
        }}
      >
        <SegmentedControl
          label={t('admin.privateContent.mode')}
          value={mode}
          onValueChange={(v) => {
            setMode(v as Mode);
            setChannels(null);
            setPage(null);
            setItems([]);
          }}
          options={[
            { value: 'channel', label: t('admin.privateContent.modeChannel') },
            { value: 'account', label: t('admin.privateContent.modeAccount') },
            { value: 'guild', label: t('admin.privateContent.modeGuild') },
          ]}
        />
        <TextInput
          label={targetLabel}
          value={target}
          inputMode="numeric"
          onChange={(e) => setTarget(e.target.value)}
        />
        <TextArea
          label={t('admin.privateContent.reason')}
          value={reason}
          maxLength={500}
          rows={3}
          required
          onChange={(e) => setReason(e.target.value)}
        />
        <div>
          <Button type="submit" disabled={!ready} loading={busy}>
            {mode === 'channel'
              ? t('admin.privateContent.read')
              : t('admin.privateContent.findChannels')}
          </Button>
        </div>
      </form>
      <ApiProblem error={error} />

      {channels && (
        <ul className="ss-list" aria-label={t('admin.privateContent.channelsLabel')}>
          {channels.length === 0 && (
            <li className="ss-meta">{t('admin.privateContent.noChannels')}</li>
          )}
          {channels.map((c) => (
            <li key={c.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">{channelTitle(c)}</div>
                <div className="ss-meta">
                  {c.id}
                  {c.private ? ` · ${t('admin.privateContent.privateChannel')}` : ''}
                </div>
              </div>
              <Button size="sm" disabled={busy} onClick={() => void readChannel(c.id)}>
                {t('admin.privateContent.read')}
              </Button>
            </li>
          ))}
        </ul>
      )}

      {page && (
        <section aria-label={t('admin.privateContent.messagesLabel')}>
          <h3 className="ss-name">{channelTitle(page.channel)}</h3>
          <p className="ss-meta">
            {t('admin.privateContent.retention', { days: page.deleted_retention_days })}
          </p>
          {items.length === 0 && <p className="ss-meta">{t('admin.privateContent.noMessages')}</p>}
          <ol className="ss-list">
            {items.map((m) => (
              <li key={m.id} className="ss-row" data-visibility={m.visibility}>
                <div className="ss-grow">
                  <div className="ss-name">
                    {m.author_name} <span className="ss-meta">{fmtMs(m.created_at_ms)}</span>{' '}
                    {m.visibility !== 'visible' && (
                      <Badge variant="danger">
                        {m.visibility === 'staff_removed'
                          ? t('admin.privateContent.staffRemoved')
                          : t('admin.privateContent.userDeleted')}
                      </Badge>
                    )}
                    {m.edited_at_ms ? (
                      <Badge variant="neutral">{t('admin.privateContent.edited')}</Badge>
                    ) : null}
                  </div>
                  <div style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
                    {m.content}
                  </div>
                  {m.attachment_count > 0 && (
                    <div className="ss-meta">
                      {t('admin.privateContent.attachments', { count: m.attachment_count })}
                    </div>
                  )}
                  {m.edits.length > 0 && (
                    <details>
                      <summary className="ss-meta">
                        {t('admin.privateContent.history', { count: m.edits.length })}
                      </summary>
                      <ul>
                        {m.edits.map((e) => (
                          <li key={e.edited_at_ms}>
                            <span className="ss-meta">{fmtMs(e.edited_at_ms)}</span>{' '}
                            <span style={{ whiteSpace: 'pre-wrap' }}>{e.previous_content}</span>
                          </li>
                        ))}
                      </ul>
                    </details>
                  )}
                </div>
              </li>
            ))}
          </ol>
          {page.next_before && (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => void readChannel(page.channel.id, page.next_before)}
            >
              {t('admin.privateContent.older')}
            </Button>
          )}
        </section>
      )}
    </>
  );
}
