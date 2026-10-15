// Bespoke message picker for reaction roles (FTR-RR-001): choose a channel, then one of its recent messages, instead
// of copying a message id by hand. The value stays the message snowflake the module expects.
import { Select } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../../api/client';
import { formatDateTime, useT } from '../../../i18n';
import { useChannels } from '../../../store';

interface RecentMessage {
  id: string;
  content: string;
  timestamp: string;
  author: { username: string; global_name?: string | null };
}

/** Channel type codes that hold messages a panel can sit on: text, announcement. */
const MESSAGE_CHANNELS = new Set([0, 5]);

/** One line for the option list: who wrote it and the first words. */
export function messageLabel(m: RecentMessage, empty: string): string {
  const who = m.author.global_name || m.author.username;
  const text = m.content.replace(/\s+/g, ' ').trim();
  const snippet = text ? (text.length > 60 ? `${text.slice(0, 57)}...` : text) : empty;
  return `${who}: ${snippet}`;
}

export function MessagePicker({
  guildId,
  label,
  help,
  value,
  onChange,
}: {
  guildId: string;
  label: string;
  help?: string | undefined;
  value: string;
  onChange: (id: string) => void;
}) {
  const t = useT();
  const channels = useChannels(guildId).filter((c) => MESSAGE_CHANNELS.has(c.type));
  const [channel, setChannel] = useState('');
  const [messages, setMessages] = useState<RecentMessage[]>([]);
  const [state, setState] = useState<'idle' | 'loading' | 'failed'>('idle');

  useEffect(() => {
    if (!channel) return;
    let live = true;
    setState('loading');
    apiFetch<RecentMessage[]>(`/channels/${encodeURIComponent(channel)}/messages?limit=50`, {
      retries: 0,
    })
      .then((list) => {
        if (!live) return;
        setMessages(list);
        setState('idle');
      })
      .catch(() => live && setState('failed'));
    return () => {
      live = false;
    };
  }, [channel]);

  const options = messages.map((m) => ({
    value: m.id,
    label: `${messageLabel(m, t('settings.messagePicker.noText'))} (${formatDateTime(new Date(m.timestamp))})`,
  }));
  // A message chosen earlier that is not among the loaded ones still shows as the current choice.
  if (value && !options.some((o) => o.value === value))
    options.unshift({ value, label: t('settings.messagePicker.current', { id: value }) });

  return (
    <div role="group" aria-label={label}>
      <span className="ss-sub">{label}</span>
      <Select
        aria-label={t('settings.messagePicker.channel')}
        placeholder={t('settings.messagePicker.channel')}
        options={channels.map((c) => ({ value: c.id, label: c.name ?? c.id }))}
        value={channel}
        onValueChange={setChannel}
      />
      <Select
        aria-label={t('settings.messagePicker.message')}
        placeholder={
          state === 'loading'
            ? t('settings.messagePicker.loading')
            : t('settings.messagePicker.message')
        }
        searchable
        options={options}
        value={value}
        onValueChange={onChange}
      />
      {state === 'failed' && (
        <p className="ss-error-text" role="alert">
          {t('settings.messagePicker.failed')}
        </p>
      )}
      {channel && state === 'idle' && messages.length === 0 && (
        <p className="ss-meta">{t('settings.messagePicker.none')}</p>
      )}
      {help && <p className="ss-meta">{help}</p>}
    </div>
  );
}
