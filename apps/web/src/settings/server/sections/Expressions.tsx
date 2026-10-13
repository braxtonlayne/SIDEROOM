import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  FileDrop,
  Modal,
  MultiSelect,
  Tabs,
  TextInput,
  type FileRejection,
} from '@sideroom/ui';
import { useState } from 'react';
import { useGuild } from '../../../store';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

const { EXPRESSION_DEFAULTS: LIMITS } = api;
const IMAGE_ACCEPT = '.png,.jpg,.jpeg,.gif,.webp';

export function readDataUri(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error(`Could not read ${file.name}.`));
    r.readAsDataURL(file);
  });
}

/** `my cool-image.png` -> `my_cool_image` (emoji names are 2-32 letters, digits and underscores). */
export function emojiNameFromFile(filename: string): string {
  const base = filename
    .replace(/\.[^.]+$/, '')
    .replace(/[^A-Za-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '');
  const name = base.slice(0, 32);
  return name.length >= 2 ? name : `${name}_emoji`.slice(0, 32);
}

export const validEmojiName = (n: string) => /^[A-Za-z0-9_]{2,32}$/.test(n);

const rejectText = (list: FileRejection[], maxBytes: number) =>
  list
    .map((r) =>
      r.reason === 'size'
        ? `${r.file.name} is larger than ${Math.round(maxBytes / 1024)} KB.`
        : r.reason === 'type'
          ? `${r.file.name} is not a PNG, JPEG, GIF or WebP image.`
          : `${r.file.name} was skipped.`,
    )
    .join(' ');

export function ExpressionsSection({ guildId }: SectionProps) {
  const emojis = useResource(`emojis:${guildId}`, () => api.listEmojis(guildId));
  const stickers = useResource(`stickers:${guildId}`, () => api.listStickers(guildId));
  useGuildEvents(guildId, ['GUILD_EMOJIS_UPDATE'], () => void emojis.reload());
  useGuildEvents(guildId, ['GUILD_STICKERS_UPDATE'], () => void stickers.reload());
  return (
    <>
      <SectionHeader
        title="Emoji & Stickers"
        description="Add custom emoji and stickers anyone in the server can use."
      />
      <Tabs
        label="Expressions"
        items={[
          {
            id: 'emoji',
            label: 'Emoji',
            content: (
              <Async res={emojis} label="Loading emoji">
                {(list) => <EmojiTab guildId={guildId} list={list} reload={emojis.reload} />}
              </Async>
            ),
          },
          {
            id: 'stickers',
            label: 'Stickers',
            content: (
              <Async res={stickers} label="Loading stickers">
                {(list) => <StickerTab guildId={guildId} list={list} reload={stickers.reload} />}
              </Async>
            ),
          },
        ]}
      />
    </>
  );
}

function EmojiTab({
  guildId,
  list,
  reload,
}: {
  guildId: string;
  list: api.EmojiObject[];
  reload: () => Promise<void>;
}) {
  const { roles } = useGuild(guildId);
  const action = useAction();
  const [rejected, setRejected] = useState<string | null>(null);
  const [toDelete, setToDelete] = useState<api.EmojiObject | null>(null);
  const staticCount = list.filter((e) => !e.animated).length;
  const animatedCount = list.filter((e) => e.animated).length;
  const roleOptions = roles
    .filter((r) => r.id !== guildId)
    .map((r) => ({ value: r.id, label: r.name }));

  const upload = async (files: File[]) => {
    setRejected(null);
    await action.run(async () => {
      const failures: string[] = [];
      for (const file of files) {
        try {
          const image = await readDataUri(file);
          await api.createEmoji(guildId, { name: emojiNameFromFile(file.name), image });
        } catch (e) {
          failures.push(`${file.name}: ${api.errorText(e)}`);
        }
      }
      await reload();
      if (failures.length) throw new Error(failures.join(' '));
    }, 'Emoji uploaded.');
  };

  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await api.deleteEmoji(guildId, toDelete.id);
      return true;
    });
    if (ok) {
      setToDelete(null);
      await reload();
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">
        Emoji must be PNG, JPEG, GIF or WebP, up to {LIMITS.maxEmojiBytes / 1024} KB each. Names use
        letters, numbers and underscores.
      </p>
      <p>
        <Badge>
          Static {staticCount} / {LIMITS.staticEmoji}
        </Badge>{' '}
        <Badge>
          Animated {animatedCount} / {LIMITS.animatedEmoji}
        </Badge>
      </p>
      <FileDrop
        label="Upload emoji"
        accept={IMAGE_ACCEPT}
        multiple
        maxSize={LIMITS.maxEmojiBytes}
        disabled={action.busy}
        onFiles={(f) => void upload(f)}
        onReject={(r) => setRejected(rejectText(r, LIMITS.maxEmojiBytes))}
      />
      {rejected && <ActionStatus error={rejected} />}
      <ActionStatus error={action.error} done={action.done} />
      {list.length === 0 ? (
        <EmptyState
          title="No custom emoji yet"
          description="Upload an image to create the first one."
        />
      ) : (
        <table className="ss-table">
          <thead>
            <tr>
              <th scope="col">Emoji</th>
              <th scope="col">Name</th>
              <th scope="col">Uploaded by</th>
              <th scope="col">Roles allowed</th>
              <th scope="col">
                <span className="ss-sr">Delete</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {list.map((e) => (
              <EmojiRow
                key={e.id}
                guildId={guildId}
                emoji={e}
                roleOptions={roleOptions}
                onChanged={reload}
                onDelete={() => setToDelete(e)}
              />
            ))}
          </tbody>
        </table>
      )}
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={`Delete :${toDelete?.name ?? ''}:`}
        description="Existing messages keep showing the name but no image."
        confirmLabel="Delete emoji"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function EmojiRow({
  guildId,
  emoji,
  roleOptions,
  onChanged,
  onDelete,
}: {
  guildId: string;
  emoji: api.EmojiObject;
  roleOptions: { value: string; label: string }[];
  onChanged: () => Promise<void>;
  onDelete: () => void;
}) {
  const [name, setName] = useState(emoji.name);
  const action = useAction();
  const valid = validEmojiName(name);
  const dirty = name !== emoji.name;
  const save = async (body: { name?: string; roles?: string[] }) => {
    const ok = await action.run(async () => {
      await api.modifyEmoji(guildId, emoji.id, body);
      return true;
    });
    if (ok) await onChanged();
  };
  return (
    <tr>
      <td>
        {/* The image key uses a separate id, so the CDN URL cannot be derived from the emoji id. */}
        <span className="ss-emoji-img" aria-hidden="true" />
        {emoji.animated && <Badge variant="accent">GIF</Badge>}
      </td>
      <td>
        <span className="ss-actions">
          <TextInput
            label={`Name of :${emoji.name}:`}
            value={name}
            maxLength={32}
            error={dirty && !valid ? 'Use 2 to 32 letters, numbers or underscores.' : undefined}
            onChange={(e) => setName(e.target.value)}
          />
          <Button
            size="sm"
            variant="secondary"
            disabled={!dirty || !valid}
            loading={action.busy}
            aria-label={`Save name for :${emoji.name}:`}
            onClick={() => void save({ name })}
          >
            Save
          </Button>
        </span>
        <ActionStatus error={action.error} />
      </td>
      <td>{api.userName(emoji.user)}</td>
      <td>
        <MultiSelect
          aria-label={`Roles allowed to use :${emoji.name}:`}
          placeholder="Everyone"
          options={roleOptions}
          values={emoji.roles}
          onValuesChange={(roles) => void save({ roles })}
        />
      </td>
      <td>
        <Button size="sm" variant="danger" aria-label={`Delete :${emoji.name}:`} onClick={onDelete}>
          Delete
        </Button>
      </td>
    </tr>
  );
}

function StickerTab({
  guildId,
  list,
  reload,
}: {
  guildId: string;
  list: api.StickerObject[];
  reload: () => Promise<void>;
}) {
  const action = useAction();
  const [pending, setPending] = useState<{ file: File; image: string } | null>(null);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [tags, setTags] = useState('');
  const [rejected, setRejected] = useState<string | null>(null);
  const [editing, setEditing] = useState<api.StickerObject | null>(null);
  const [toDelete, setToDelete] = useState<api.StickerObject | null>(null);

  const pick = async (files: File[]) => {
    setRejected(null);
    const file = files[0];
    if (!file) return;
    try {
      const image = await readDataUri(file);
      setPending({ file, image });
      setName((n) => n || file.name.replace(/\.[^.]+$/, '').slice(0, 30));
    } catch (e) {
      setRejected(api.errorText(e));
    }
  };

  const create = async () => {
    if (!pending) return;
    const made = await action.run(
      () =>
        api.createSticker(guildId, {
          name: name.trim(),
          ...(description.trim() ? { description: description.trim() } : {}),
          tags: tags.trim(),
          file: pending.image,
        }),
      'Sticker uploaded.',
    );
    if (made) {
      setPending(null);
      setName('');
      setDescription('');
      setTags('');
      await reload();
    }
  };

  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await api.deleteSticker(guildId, toDelete.id);
      return true;
    });
    if (ok) {
      setToDelete(null);
      await reload();
    }
  };

  const nameOk = name.trim().length >= 2 && name.trim().length <= 30;
  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">
        Stickers are PNG, APNG, GIF or Lottie files up to {LIMITS.maxStickerBytes / 1024} KB.
      </p>
      <p>
        <Badge>
          Stickers {list.length} / {LIMITS.stickers}
        </Badge>
      </p>
      <FileDrop
        label="Choose sticker file"
        accept={`${IMAGE_ACCEPT},.json`}
        maxSize={LIMITS.maxStickerBytes}
        onFiles={(f) => void pick(f)}
        onReject={(r) => setRejected(rejectText(r, LIMITS.maxStickerBytes))}
      />
      {rejected && <ActionStatus error={rejected} />}
      {pending && (
        <div className="ss-card">
          <h2>Upload {pending.file.name}</h2>
          <TextInput
            label="Sticker name"
            value={name}
            maxLength={30}
            error={name && !nameOk ? 'Use 2 to 30 characters.' : undefined}
            onChange={(e) => setName(e.target.value)}
          />
          <TextInput
            label="Related emoji"
            hint="Used to suggest the sticker. Type one or more emoji or keywords."
            value={tags}
            maxLength={200}
            onChange={(e) => setTags(e.target.value)}
          />
          <TextInput
            label="Description"
            value={description}
            maxLength={100}
            onChange={(e) => setDescription(e.target.value)}
          />
          <div className="ss-actions">
            <Button variant="ghost" onClick={() => setPending(null)}>
              Cancel
            </Button>
            <Button
              disabled={!nameOk || !tags.trim()}
              loading={action.busy}
              onClick={() => void create()}
            >
              Upload sticker
            </Button>
          </div>
        </div>
      )}
      <ActionStatus error={action.error} done={action.done} />
      {list.length === 0 ? (
        <EmptyState
          title="No stickers yet"
          description="Upload a file to create the first sticker."
        />
      ) : (
        <ul className="ss-list" aria-label="Stickers">
          {list.map((s) => (
            <li key={s.id} className="ss-row">
              <span className="ss-emoji-img" aria-hidden="true" />
              <div className="ss-grow">
                <div className="ss-name">{s.name}</div>
                <div className="ss-meta">
                  {s.tags} {s.description ? `- ${s.description}` : ''} - by {api.userName(s.user)}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                aria-label={`Edit sticker ${s.name}`}
                onClick={() => setEditing(s)}
              >
                Edit
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={`Delete sticker ${s.name}`}
                onClick={() => setToDelete(s)}
              >
                Delete
              </Button>
            </li>
          ))}
        </ul>
      )}
      {editing && (
        <StickerEdit
          guildId={guildId}
          sticker={editing}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await reload();
          }}
        />
      )}
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={`Delete ${toDelete?.name ?? 'sticker'}`}
        confirmLabel="Delete sticker"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function StickerEdit({
  guildId,
  sticker,
  onClose,
  onSaved,
}: {
  guildId: string;
  sticker: api.StickerObject;
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const [name, setName] = useState(sticker.name);
  const [tags, setTags] = useState(sticker.tags);
  const [description, setDescription] = useState(sticker.description ?? '');
  const action = useAction();
  const save = async () => {
    const ok = await action.run(async () => {
      await api.modifyStickerReq(guildId, sticker.id, {
        name: name.trim(),
        tags: tags.trim(),
        description: description.trim() || null,
      });
      return true;
    });
    if (ok) await onSaved();
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={`Edit ${sticker.name}`}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            loading={action.busy}
            disabled={name.trim().length < 2}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <TextInput
          label="Sticker name"
          value={name}
          maxLength={30}
          onChange={(e) => setName(e.target.value)}
        />
        <TextInput
          label="Related emoji"
          value={tags}
          maxLength={200}
          onChange={(e) => setTags(e.target.value)}
        />
        <TextInput
          label="Description"
          value={description}
          maxLength={100}
          onChange={(e) => setDescription(e.target.value)}
        />
      </div>
    </Modal>
  );
}
