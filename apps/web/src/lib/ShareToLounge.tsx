// "Share to Lounge" (D-226): posts a Library series or a Cinema title into a channel or DM the person picks.
// The message is the `<library:id>` / `<cinema:id>` token (with an optional note), which the Lounge draws as a
// rich card (`lounge/msg/RichCards.tsx`). Used by the Library series page and the Cinema title page.
import { Button, Modal, Notice, Select, TextArea, useToast } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import { DM_GUILD, useStore } from '../store';
import { memberKey } from '../store/reducer';
import { useChannels, useGuilds } from '../store/hooks';
import { PERM, basePermissions, computeChannelBits } from '../lounge/liveMap';
import { usePermEngine } from '../lounge/permEngine';
import type { Channel } from '../store/types';

/** Channel types a message can be posted to: text, DM, group DM, announcement. */
const POSTABLE = new Set([0, 1, 3, 5]);

export interface ShareTarget {
  /** Absolute URL of the thing shared (for copying). */
  url: string;
  /** The rich-card token posted to the Lounge: `<library:id>` or `<cinema:id>` (D-226). */
  card: string;
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
  const engine = usePermEngine();
  const me = useStore((s) => s.currentUserId);
  const guild = useStore((s) => (place === DM_GUILD ? undefined : s.guilds[place]));
  const viewer = useStore((s) => (me ? s.members[memberKey(place, me)] : undefined));
  const roles = useStore((s) => s.roles);
  const roleIds = useStore((s) => s.roleIdsByGuild[place]);

  useEffect(() => {
    if (open && guilds.length === 0) void loadGuilds().catch(() => undefined);
  }, [open, guilds.length, loadGuilds]);
  // A server's channels arrive with its GUILD_CREATE; fetch them when the store has none yet.
  useEffect(() => {
    if (open && place !== DM_GUILD && channels.length === 0)
      void loadGuild(place).catch(() => undefined);
  }, [open, place, channels.length, loadGuild]);

  // A server channel is offered only where the person can send and use library embeds (D-241):
  // the same bits the server checks. Until the member is loaded the server decides, so nothing is hidden.
  const canPost = useMemo(() => {
    if (place === DM_GUILD || !me) return () => true;
    const roleById = new Map(
      (roleIds ?? []).flatMap((id) => (roles[id] ? [[id, roles[id]] as const] : [])),
    );
    const owner = guild?.owner_id === me;
    if (!viewer && !owner) return () => true;
    const need = PERM.SEND_MESSAGES | PERM.USE_LIBRARY_EMBEDS;
    return (c: Channel) => {
      const bits =
        (engine
          ? computeChannelBits(engine, {
              guildId: place,
              ownerId: guild?.owner_id,
              viewerId: me,
              viewer,
              roleById,
              channel: c,
            })
          : undefined) ?? basePermissions(place, guild?.owner_id, me, roleById, viewer?.roles);
      return (bits & need) === need;
    };
  }, [place, me, guild, viewer, roles, roleIds, engine]);

  const options = useMemo(
    () =>
      channels
        .filter((c) => POSTABLE.has(c.type) && !c.is_message_request && canPost(c))
        .map((c) => ({
          value: c.id,
          label: place === DM_GUILD ? dmName(c) : `# ${c.name ?? c.id}`,
        })),
    [channels, place, canPost],
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
      const content = [note.trim(), target.card].filter(Boolean).join('\n');
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

  // D-226: "Copy link" stays next to posting the card.
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(target.url);
      toast.show({ title: t('share.copied') });
    } catch {
      toast.show({ title: t('share.copyFailed') });
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
          <Button variant="ghost" onClick={() => void copy()}>
            {t('share.copyLink')}
          </Button>
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
