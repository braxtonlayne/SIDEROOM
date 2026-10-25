// Server Settings > Features > RSS/Atom poster (FTR-RSS-001): the feeds the server follows, where each posts, its
// keyword filters and poll interval, and whether the last fetch worked. The server checks every address against its
// SSRF policy, so a refused address comes back as an error under the form.
import { Button, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import {
  addFeed,
  deleteFeed,
  listFeeds,
  patchFeed,
  type RssFeed,
} from '../../../lounge/features/api';
import { useChannels } from '../../../store';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';

/** Channel type codes a feed can post into: text and announcement. */
const POSTABLE = new Set([0, 5]);

export const INTERVALS = [
  { secs: 300, key: 'settings.rss.every5m' },
  { secs: 900, key: 'settings.rss.every15m' },
  { secs: 1800, key: 'settings.rss.every30m' },
  { secs: 3600, key: 'settings.rss.every1h' },
  { secs: 21600, key: 'settings.rss.every6h' },
  { secs: 86400, key: 'settings.rss.every24h' },
] as const;

/** "a, b ,, c" -> ["a", "b", "c"]. */
export const keywordsOf = (raw: string): string[] =>
  raw
    .split(',')
    .map((k) => k.trim())
    .filter(Boolean);

export function RssPanel({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`rss-${guildId}`, () => listFeeds(guildId));
  const channels = useChannels(guildId).filter((c) => POSTABLE.has(c.type));
  const [url, setUrl] = useState('');
  const [channel, setChannel] = useState('');
  const [include, setInclude] = useState('');
  const [exclude, setExclude] = useState('');
  const [secs, setSecs] = useState<number>(3600);
  const act = useAction();

  const channelName = (id: string) => channels.find((c) => c.id === id)?.name ?? id;

  const add = () =>
    void act.run(async () => {
      await addFeed(guildId, {
        url: url.trim(),
        channel_id: channel,
        include_keywords: keywordsOf(include),
        exclude_keywords: keywordsOf(exclude),
        poll_interval_secs: secs,
      });
      setUrl('');
      setInclude('');
      setExclude('');
      await res.reload();
    }, t('settings.rss.added'));

  const toggle = (f: RssFeed, enabled: boolean) =>
    void act.run(async () => {
      await patchFeed(guildId, f.id, { enabled });
      await res.reload();
    });
  const remove = (f: RssFeed) =>
    void act.run(async () => {
      await deleteFeed(guildId, f.id);
      await res.reload();
    }, t('settings.rss.removed'));

  return (
    <section aria-label={t('settings.rss.title')}>
      <h4 className="ss-sub">{t('settings.rss.title')}</h4>
      <Async res={res} label={t('settings.rss.loading')}>
        {(feeds) =>
          feeds.length === 0 ? (
            <p className="ss-meta">{t('settings.rss.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('settings.rss.title')}>
              {feeds.map((f) => (
                <li key={f.id} className="ss-row">
                  <span>
                    <strong>{f.title || f.url}</strong>
                    <br />
                    <span className="ss-meta">
                      {t('settings.rss.line', { url: f.url, channel: channelName(f.channel_id) })}
                    </span>
                    <br />
                    <span className="ss-meta">
                      {f.last_checked_at
                        ? t('settings.rss.checked', {
                            time: formatDateTime(new Date(f.last_checked_at)),
                            status: f.last_status,
                          })
                        : t('settings.rss.notChecked')}
                      {f.failures > 0
                        ? ` ${t('settings.rss.failures', { count: f.failures })}`
                        : ''}
                    </span>
                  </span>
                  <Switch
                    label={t('settings.rss.enable', { name: f.title || f.url })}
                    checked={f.enabled}
                    disabled={act.busy}
                    onChange={(e) => toggle(f, e.target.checked)}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={act.busy}
                    aria-label={t('settings.rss.remove', { name: f.title || f.url })}
                    onClick={() => remove(f)}
                  >
                    {t('settings.rss.removeShort')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <div className="adm-form">
        <TextInput
          label={t('settings.rss.url')}
          type="url"
          placeholder={t('settings.rss.urlPlaceholder')}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />
        <Select
          aria-label={t('settings.rss.channel')}
          placeholder={t('settings.rss.channel')}
          options={channels.map((c) => ({ value: c.id, label: c.name ?? c.id }))}
          value={channel}
          onValueChange={setChannel}
        />
        <TextInput
          label={t('settings.rss.include')}
          hint={t('settings.rss.keywordHint')}
          value={include}
          onChange={(e) => setInclude(e.target.value)}
        />
        <TextInput
          label={t('settings.rss.exclude')}
          hint={t('settings.rss.keywordHint')}
          value={exclude}
          onChange={(e) => setExclude(e.target.value)}
        />
        <Select
          aria-label={t('settings.rss.interval')}
          options={INTERVALS.map((i) => ({ value: String(i.secs), label: t(i.key) }))}
          value={String(secs)}
          onValueChange={(v) => setSecs(Number(v))}
        />
        <Button disabled={!url.trim() || !channel || act.busy} onClick={add}>
          {t('settings.rss.add')}
        </Button>
      </div>
      <ActionStatus error={act.error} done={act.done} />
    </section>
  );
}
