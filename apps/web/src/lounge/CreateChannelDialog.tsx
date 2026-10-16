import { Button, Checkbox, Modal, Switch, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { rest, useStore } from '../store';
import { useGotoGuild } from './AddServerDialog';
import { useChannels, useCurrentUser, useGuild, useMembers, useRoles } from './data';
import { slugChannelName } from './inviteFlow';

const VIEW_CHANNEL = (1n << 10n).toString();

type Kind = 'text' | 'voice' | 'party' | 'announcement' | 'forum' | 'stage' | 'media';

const KINDS: { kind: Kind; type: number; labelKey: string; blurbKey: string; slug: boolean }[] = [
  {
    kind: 'text',
    type: 0,
    labelKey: 'lounge.createChannelDialog.kindText',
    blurbKey: 'lounge.createChannelDialog.kindTextBlurb',
    slug: true,
  },
  {
    kind: 'voice',
    type: 2,
    labelKey: 'lounge.createChannelDialog.kindVoice',
    blurbKey: 'lounge.createChannelDialog.kindVoiceBlurb',
    slug: false,
  },
  {
    // D-208: no channel type of its own; a voice channel with a persistent party bound to it.
    kind: 'party',
    type: 2,
    labelKey: 'lounge.createChannelDialog.kindParty',
    blurbKey: 'lounge.createChannelDialog.kindPartyBlurb',
    slug: false,
  },
  {
    kind: 'announcement',
    type: 5,
    labelKey: 'lounge.createChannelDialog.kindAnnouncement',
    blurbKey: 'lounge.createChannelDialog.kindAnnouncementBlurb',
    slug: true,
  },
  {
    kind: 'forum',
    type: 15,
    labelKey: 'lounge.createChannelDialog.kindForum',
    blurbKey: 'lounge.createChannelDialog.kindForumBlurb',
    slug: true,
  },
  {
    kind: 'stage',
    type: 13,
    labelKey: 'lounge.createChannelDialog.kindStage',
    blurbKey: 'lounge.createChannelDialog.kindStageBlurb',
    slug: false,
  },
  {
    kind: 'media',
    type: 16,
    labelKey: 'lounge.createChannelDialog.kindMedia',
    blurbKey: 'lounge.createChannelDialog.kindMediaBlurb',
    slug: true,
  },
];

/**
 * Create Channel / Create Category (POST /guilds/{id}/channels). Renders nothing without MANAGE_CHANNELS.
 * A private channel hides it from @everyone and shows a picker for the roles and members who may see it.
 */
export function CreateChannelDialog({
  guildId,
  parentId,
  category: categoryOnly,
  onClose,
}: {
  guildId: string;
  parentId?: string | undefined;
  category?: boolean | undefined;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const roles = useRoles(guildId);
  const members = useMembers(guildId);
  const me = useCurrentUser();
  const goto = useGotoGuild();
  const [kind, setKind] = useState<Kind>('text');
  const [name, setName] = useState('');
  const [isPrivate, setPrivate] = useState(false);
  const [allowed, setAllowed] = useState<{ roles: string[]; members: string[] }>({
    roles: [],
    members: [],
  });

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isCategory = !!categoryOnly;

  // Client-side hiding is a convenience; the server enforces MANAGE_CHANNELS.
  if (!guild?.permissions.manageChannels) return null;

  const parent = parentId ? channels.find((c) => c.id === parentId) : undefined;
  const def = KINDS.find((k) => k.kind === kind)!;
  const pickable = {
    roles: roles.filter((r) => r.id !== guildId),
    members: members.filter((m) => m.user.id !== me.id),
  };

  const toggle = (key: 'roles' | 'members', id: string, on: boolean) =>
    setAllowed((a) => ({
      ...a,
      [key]: on ? [...a[key].filter((x) => x !== id), id] : a[key].filter((x) => x !== id),
    }));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const clean = isCategory || !def.slug ? name.trim() : slugChannelName(name);
    if (!clean)
      return setError(
        t(
          isCategory
            ? 'lounge.createChannelDialog.categoryNameRequired'
            : 'lounge.createChannelDialog.channelNameRequired',
        ),
      );

    const body: rest.CreateChannelBody = {
      name: clean,
      type: isCategory ? 4 : def.type,
      // D-227: the room starts empty; the first person to join hosts it and picks what to watch.
      ...(!isCategory && kind === 'party' ? { sr_watch_party_room: true } : {}),
      ...(parentId && !isCategory ? { parent_id: parentId } : {}),
      ...(isPrivate
        ? {
            // @everyone (role id = guild id) cannot see it; the creator and the picked people can.
            permission_overwrites: [
              { id: guildId, type: 0, allow: '0', deny: VIEW_CHANNEL },
              { id: me.id, type: 1, allow: VIEW_CHANNEL, deny: '0' },
              ...allowed.roles.map((id) => ({ id, type: 0, allow: VIEW_CHANNEL, deny: '0' })),
              ...allowed.members.map((id) => ({ id, type: 1, allow: VIEW_CHANNEL, deny: '0' })),
            ],
          }
        : {}),
    };
    setBusy(true);
    setError(null);
    try {
      const created = await useStore.getState().createChannel(guildId, body);
      onClose();
      if (created.type === 0) goto(guildId, created.id);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('lounge.createChannelDialog.forbidden')
          : err instanceof ApiError
            ? err.message
            : t('lounge.createChannelDialog.createFailed'),
      );
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={
        isCategory
          ? t('lounge.createChannelDialog.createCategory')
          : t('lounge.createChannelDialog.createChannel')
      }
      description={
        parent ? t('lounge.createChannelDialog.inParent', { name: parent.name }) : undefined
      }
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)} noValidate>
        {!isCategory && (
          <fieldset className="lng-kind">
            <legend>{t('lounge.createChannelDialog.channelType')}</legend>
            {KINDS.map((k) => (
              <label
                key={k.kind}
                className="lng-kind-opt"
                data-selected={kind === k.kind || undefined}
              >
                <input
                  type="radio"
                  name="channel-type"
                  value={k.kind}
                  checked={kind === k.kind}
                  onChange={() => setKind(k.kind)}
                />
                <span>
                  <strong>{t(k.labelKey)}</strong>
                  <small>{t(k.blurbKey)}</small>
                </span>
              </label>
            ))}
          </fieldset>
        )}
        {!isCategory && kind === 'party' && (
          <p className="lng-party-title">{t('lounge.createChannelDialog.partyEmptyRoom')}</p>
        )}
        <TextInput
          label={
            isCategory
              ? t('lounge.createChannelDialog.categoryName')
              : t('lounge.createChannelDialog.channelName')
          }
          value={name}
          maxLength={100}
          autoFocus
          placeholder={
            isCategory
              ? t('lounge.createChannelDialog.newCategory')
              : def.slug
                ? t('lounge.createChannelDialog.newChannelSlug')
                : t('lounge.createChannelDialog.newChannel')
          }
          error={error}
          onChange={(e) => setName(e.target.value)}
        />
        {!isCategory && def.slug && name.trim() && slugChannelName(name) !== name.trim() && (
          <p className="lng-muted" aria-live="polite">
            {t('lounge.createChannelDialog.createdAs')}
            <strong>#{slugChannelName(name)}</strong>
          </p>
        )}
        <Switch
          label={
            isCategory
              ? t('lounge.createChannelDialog.privateCategory')
              : t('lounge.createChannelDialog.privateChannel')
          }
          description={t('lounge.createChannelDialog.privateDescription')}
          checked={isPrivate}
          onChange={(e) => setPrivate(e.target.checked)}
        />
        {isPrivate && (
          <fieldset className="lng-pickers">
            <legend className="lng-field-label">
              {t('lounge.createChannelDialog.whoCanView')}
            </legend>
            {pickable.roles.length === 0 && pickable.members.length === 0 && (
              <p className="lng-muted">{t('lounge.createChannelDialog.noOthers')}</p>
            )}
            {pickable.roles.length > 0 && (
              <ul className="lng-pick-list" aria-label={t('lounge.createChannelDialog.roles')}>
                {pickable.roles.map((r) => (
                  <li key={r.id} className="lng-pick-row">
                    <Checkbox
                      label={r.name}
                      checked={allowed.roles.includes(r.id)}
                      onChange={(ev) => toggle('roles', r.id, ev.target.checked)}
                    />
                  </li>
                ))}
              </ul>
            )}
            {pickable.members.length > 0 && (
              <ul className="lng-pick-list" aria-label={t('lounge.createChannelDialog.members')}>
                {pickable.members.map((m) => (
                  <li key={m.user.id} className="lng-pick-row">
                    <Checkbox
                      label={m.nick || m.user.displayName}
                      description={m.user.username}
                      checked={allowed.members.includes(m.user.id)}
                      onChange={(ev) => toggle('members', m.user.id, ev.target.checked)}
                    />
                  </li>
                ))}
              </ul>
            )}
          </fieldset>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.createChannelDialog.cancel')}
          </Button>
          <Button type="submit" loading={busy}>
            {isCategory
              ? t('lounge.createChannelDialog.createCategory')
              : t('lounge.createChannelDialog.createChannel')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
