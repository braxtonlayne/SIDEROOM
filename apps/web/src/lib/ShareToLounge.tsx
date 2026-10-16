// "Share to Lounge": posts a link to a Library series or a Cinema title into a channel or DM the person picks.
// The message is the link (with an optional note); the Lounge renders SIDE ROOM links as rich cards. Used by
// the Library series page and the Cinema title page.
import { Button, Modal, Notice, Select, TextArea, useToast } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import { DM_GUILD, useStore } from '../store';
import { useChannels, useGuilds } from '../store/hooks';
import type { Channel } from '../store/types';

/** Channel types a message can be posted to: text, DM, group DM, announcement. */
const POSTABLE = new Set([0, 1, 3, 5]);

export interface ShareTarget {
  /** Absolute URL of the thing shared. */
  url: string;
  /** Its name, shown in the dialog. */
  title: string;
  /** "Library" or "Cinema", shown under the title. */
  kind: string;
}

const dmName = (c: Channel) =>
  c.name || (c.recipients ?? []).map((r) => r.global_name ?? r.username).join(', ') || c.id;

export function ShareToLoungeDialog({
  open,
  onClose,
  target,
}: {
  open: boolean;
  onClose: () => void;
  target: ShareTarget;
}) {
  const t = useT();
  const toast = useToast();
  const guilds = useGuilds();
  const loadGuilds = useStore((s) => s.loadGuilds);
  const loadGuild = useStore((s) => s.loadGuild);
  const sendMessage = useStore((s) => s.sendMessage);
  const [place, setPlace] = useState<string>(DM_GUILD);
  const [channelId, setChannelId] = useState<string>('');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const channels = useChannels(place);

  useEffect(() => {
    if (open && guilds.length === 0) void loadGuilds().catch(() => undefined);
  }, [open, guilds.length, loadGuilds]);
  // A server's channels arrive with its GUILD_CREATE; fetch them when the store has none yet.
  useEffect(() => {
    if (open && place !== DM_GUILD && channels.length === 0)
      void loadGuild(place).catch(() => undefined);
  }, [open, place, channels.length, loadGuild]);

  const options = useMemo(
    () =>
      channels
        .filter((c) => POSTABLE.has(c.type) && !c.is_message_request)
        .map((c) => ({
          value: c.id,
          label: place === DM_GUILD ? dmName(c) : `# ${c.name ?? c.id}`,
        })),
    [channels, place],
  );
  // Keep the pick valid when the place changes.
  useEffect(() => {
    if (!options.some((o) => o.value === channelId)) setChannelId(options[0]?.value ?? '');
  }, [options, channelId]);

  const placeOptions = [
    { value: DM_GUILD, label: t('share.directMessages') },
    ...guilds.map((g) => ({ value: g.id, label: g.name })),
  ];

  const send = async () => {
    if (!channelId) return;
    setBusy(true);
    setError(false);
    try {
      const content = [note.trim(), target.url].filter(Boolean).join('\n');
      await sendMessage(channelId, content);
      toast.show({ title: t('share.sent') });
      setNote('');
      onClose();
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal open={open} onClose={onClose} title={t('share.title')} size="sm">
      <div className="share-dialog">
        <p>
          <strong>{target.title}</strong>
          <br />
          <span className="share-kind">{target.kind}</span>
        </p>
        <span>{t('share.place')}</span>
        <Select
          aria-label={t('share.place')}
          options={placeOptions}
          value={place}
          onValueChange={setPlace}
        />
        <span>{t('share.channel')}</span>
        <Select
          aria-label={t('share.channel')}
          options={options}
          value={channelId || null}
          onValueChange={setChannelId}
          placeholder={t('share.noChannels')}
        />
        <TextArea
          label={t('share.note')}
          value={note}
          rows={2}
          maxLength={500}
          onChange={(e) => setNote(e.target.value)}
        />
        {error && <Notice variant="danger">{t('share.failed')}</Notice>}
        <div className="share-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('share.cancel')}
          </Button>
          <Button disabled={!channelId || busy} onClick={() => void send()}>
            {t('share.send')}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
