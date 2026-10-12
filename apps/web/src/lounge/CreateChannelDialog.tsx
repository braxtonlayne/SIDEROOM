import { Button, Modal, Switch, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { rest, useStore } from '../store';
import { useGotoGuild } from './AddServerDialog';
import { useChannels, useCurrentUser, useGuild } from './data';
import { slugChannelName } from './inviteFlow';

const VIEW_CHANNEL = (1n << 10n).toString();

type Kind = 'text' | 'voice';
const KIND_TYPE: Record<Kind, number> = { text: 0, voice: 2 };

/** Create Channel / Create Category (POST /guilds/{id}/channels). Renders nothing without MANAGE_CHANNELS. */
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
  const me = useCurrentUser();
  const goto = useGotoGuild();
  const [kind, setKind] = useState<Kind>('text');
  const [name, setName] = useState('');
  const [isPrivate, setPrivate] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isCategory = !!categoryOnly;

  // Client-side hiding is a convenience; the server enforces MANAGE_CHANNELS.
  if (!guild?.permissions.manageChannels) return null;

  const parent = parentId ? channels.find((c) => c.id === parentId) : undefined;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const clean = isCategory || kind === 'voice' ? name.trim() : slugChannelName(name);
    if (!clean) return setError(`Enter a ${isCategory ? 'category' : 'channel'} name.`);
    const body: rest.CreateChannelBody = {
      name: clean,
      type: isCategory ? 4 : KIND_TYPE[kind],
      ...(parentId && !isCategory ? { parent_id: parentId } : {}),
      ...(isPrivate
        ? {
            // @everyone (role id = guild id) cannot see it; the creator keeps access.
            permission_overwrites: [
              { id: guildId, type: 0, allow: '0', deny: VIEW_CHANNEL },
              { id: me.id, type: 1, allow: VIEW_CHANNEL, deny: '0' },
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
            {(['text', 'voice'] as const).map((k) => (
              <label key={k} className="lng-kind-opt" data-selected={kind === k || undefined}>
                <input
                  type="radio"
                  name="channel-type"
                  value={k}
                  checked={kind === k}
                  onChange={() => setKind(k)}
                />
                <span>
                  <strong>{k === 'text' ? 'Text' : 'Voice'}</strong>
                  <small>
                    {k === 'text'
                      ? 'Send messages, images and opinions'
                      : 'Hang out together with voice and video'}
                  </small>
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
          placeholder={isCategory ? 'New Category' : kind === 'text' ? 'new-channel' : 'New Voice'}
          error={error}
          onChange={(e) => setName(e.target.value)}
        />
        <Switch
          label={isCategory ? 'Private Category' : 'Private Channel'}
          description="Only you and members you add can view it."
          checked={isPrivate}
          onChange={(e) => setPrivate(e.target.checked)}
        />
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
