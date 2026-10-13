import { Button, Checkbox, Modal, Switch, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { rest, useStore } from '../store';
import { useGotoGuild } from './AddServerDialog';
import { useChannels, useCurrentUser, useGuild, useMembers, useRoles } from './data';
import { slugChannelName } from './inviteFlow';

const VIEW_CHANNEL = (1n << 10n).toString();

type Kind = 'text' | 'voice' | 'announcement' | 'forum' | 'stage' | 'media';

const KINDS: { kind: Kind; type: number; label: string; blurb: string; slug: boolean }[] = [
  { kind: 'text', type: 0, label: 'Text', blurb: 'Send messages, images and opinions', slug: true },
  {
    kind: 'voice',
    type: 2,
    label: 'Voice',
    blurb: 'Hang out together with voice and video',
    slug: false,
  },
  {
    kind: 'announcement',
    type: 5,
    label: 'Announcement',
    blurb: 'Important updates people can follow',
    slug: true,
  },
  { kind: 'forum', type: 15, label: 'Forum', blurb: 'Organized posts, one topic each', slug: true },
  {
    kind: 'stage',
    type: 13,
    label: 'Stage',
    blurb: 'Host talks with speakers and an audience',
    slug: false,
  },
  {
    kind: 'media',
    type: 16,
    label: 'Media',
    blurb: 'A gallery for sharing art and clips',
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
    if (!clean) return setError(`Enter a ${isCategory ? 'category' : 'channel'} name.`);
    const body: rest.CreateChannelBody = {
      name: clean,
      type: isCategory ? 4 : def.type,
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
          ? 'You do not have permission to create channels here.'
          : err instanceof ApiError
            ? err.message
            : 'Could not create the channel. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={isCategory ? 'Create Category' : 'Create Channel'}
      description={parent ? `in ${parent.name}` : undefined}
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)} noValidate>
        {!isCategory && (
          <fieldset className="lng-kind">
            <legend>Channel type</legend>
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
                  <strong>{k.label}</strong>
                  <small>{k.blurb}</small>
                </span>
              </label>
            ))}
          </fieldset>
        )}
        <TextInput
          label={isCategory ? 'Category name' : 'Channel name'}
          value={name}
          maxLength={100}
          autoFocus
          placeholder={isCategory ? 'New Category' : def.slug ? 'new-channel' : 'New Channel'}
          error={error}
          onChange={(e) => setName(e.target.value)}
        />
        {!isCategory && def.slug && name.trim() && slugChannelName(name) !== name.trim() && (
          <p className="lng-muted" aria-live="polite">
            It will be created as <strong>#{slugChannelName(name)}</strong>
          </p>
        )}
        <Switch
          label={isCategory ? 'Private Category' : 'Private Channel'}
          description="Only you and the roles and members you pick can view it."
          checked={isPrivate}
          onChange={(e) => setPrivate(e.target.checked)}
        />
        {isPrivate && (
          <fieldset className="lng-pickers">
            <legend className="lng-field-label">Who can view it</legend>
            {pickable.roles.length === 0 && pickable.members.length === 0 && (
              <p className="lng-muted">No other roles or members yet. Only you can view it.</p>
            )}
            {pickable.roles.length > 0 && (
              <ul className="lng-pick-list" aria-label="Roles">
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
              <ul className="lng-pick-list" aria-label="Members">
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
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            {isCategory ? 'Create Category' : 'Create Channel'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
