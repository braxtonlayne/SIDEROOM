import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  FileDrop,
  Modal,
  MultiSelect,
  Switch,
  Tabs,
  TextInput,
  type FileRejection,
} from '@sideroom/ui';
import { useState } from 'react';
import { resetDefaultPacks } from '../../../lounge/components/ExpressionPicker';
import { t as translate, useT } from '../../../i18n';
import { useGuild } from '../../../store';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';
import { SoundboardTab } from './SoundboardTab';

const { EXPRESSION_DEFAULTS: LIMITS } = api;
const IMAGE_ACCEPT = '.png,.jpg,.jpeg,.gif,.webp';

export function readDataUri(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () =>
      reject(new Error(translate('settings.expressions.readFailed', { name: file.name })));
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
        ? translate('settings.expressions.tooLarge', {
            name: r.file.name,
            kb: String(Math.round(maxBytes / 1024)),
          })
        : r.reason === 'type'
          ? translate('settings.expressions.badType', { name: r.file.name })
          : translate('settings.expressions.skipped', { name: r.file.name }),
    )
    .join(' ');

export function ExpressionsSection({ guildId }: SectionProps) {
  const t = useT();
  const emojis = useResource(`emojis:${guildId}`, () => api.listEmojis(guildId));
  const stickers = useResource(`stickers:${guildId}`, () => api.listStickers(guildId));
  useGuildEvents(guildId, ['GUILD_EMOJIS_UPDATE'], () => void emojis.reload());
  useGuildEvents(guildId, ['GUILD_STICKERS_UPDATE'], () => void stickers.reload());
  return (
    <>
      <SectionHeader
        title={t('settings.expressions.title')}
        description={t('settings.expressions.description')}
      />
      <Tabs
        label={t('settings.expressions.tabsLabel')}
        items={[
          {
            id: 'emoji',
            label: t('settings.expressions.tabEmoji'),
            content: (
              <Async res={emojis} label={t('settings.expressions.loadingEmoji')}>
                {(list) => <EmojiTab guildId={guildId} list={list} reload={emojis.reload} />}
              </Async>
            ),
          },
          {
            id: 'stickers',
            label: t('settings.expressions.tabStickers'),
            content: (
              <Async res={stickers} label={t('settings.expressions.loadingStickers')}>
                {(list) => <StickerTab guildId={guildId} list={list} reload={stickers.reload} />}
              </Async>
            ),
          },
          {
            id: 'soundboard',
            label: t('settings.soundboard.tab'),
            content: <SoundboardTab guildId={guildId} />,
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
  const t = useT();
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
    }, t('settings.expressions.emojiUploaded'));
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
        {t('settings.expressions.emojiRules', { kb: String(LIMITS.maxEmojiBytes / 1024) })}
      </p>
      <p>
        <Badge>
          {t('settings.expressions.staticCount', {
            count: String(staticCount),
            max: String(LIMITS.staticEmoji),
          })}
        </Badge>{' '}
        <Badge>
          {t('settings.expressions.animatedCount', {
            count: String(animatedCount),
            max: String(LIMITS.animatedEmoji),
          })}
        </Badge>
      </p>
      <FileDrop
        label={t('settings.expressions.uploadEmoji')}
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
          title={t('settings.expressions.noEmojiTitle')}
          description={t('settings.expressions.noEmojiDescription')}
        />
      ) : (
        <table className="ss-table">
          <thead>
            <tr>
              <th scope="col">{t('settings.expressions.colEmoji')}</th>
              <th scope="col">{t('settings.expressions.colName')}</th>
              <th scope="col">{t('settings.expressions.colUploadedBy')}</th>
              <th scope="col">{t('settings.expressions.colRoles')}</th>
              <th scope="col">
                <span className="ss-sr">{t('settings.expressions.delete')}</span>
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
        title={t('settings.expressions.deleteNamed', { name: toDelete?.name ?? '' })}
        description={t('settings.expressions.deleteEmojiDescription')}
        confirmLabel={t('settings.expressions.deleteEmoji')}
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
  const t = useT();
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
        {emoji.animated && <Badge variant="accent">{t('settings.expressions.animatedBadge')}</Badge>}
      </td>
      <td>
        <span className="ss-actions">
          <TextInput
            label={t('settings.expressions.nameOf', { name: emoji.name })}
            value={name}
            maxLength={32}
            error={dirty && !valid ? t('settings.expressions.nameRule') : undefined}
            onChange={(e) => setName(e.target.value)}
          />
          <Button
            size="sm"
            variant="secondary"
            disabled={!dirty || !valid}
            loading={action.busy}
            aria-label={t('settings.expressions.saveNameFor', { name: emoji.name })}
            onClick={() => void save({ name })}
          >
            {t('settings.expressions.save')}
          </Button>
        </span>
        <ActionStatus error={action.error} />
      </td>
      <td>{api.userName(emoji.user)}</td>
      <td>
        <MultiSelect
          aria-label={t('settings.expressions.rolesAllowedFor', { name: emoji.name })}
          placeholder={t('settings.expressions.everyone')}
          options={roleOptions}
          values={emoji.roles}
          onValuesChange={(roles) => void save({ roles })}
        />
      </td>
      <td>
        <Button
          size="sm"
          variant="danger"
          aria-label={t('settings.expressions.deleteNamed', { name: emoji.name })}
          onClick={onDelete}
        >
          {t('settings.expressions.delete')}
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
  const t = useT();
  const action = useAction();
  const [pending, setPending] = useState<{ file: File; image: string } | null>(null);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [tags, setTags] = useState('');
  const [rejected, setRejected] = useState<string | null>(null);
  const [editing, setEditing] = useState<api.StickerObject | null>(null);
  const [toDelete, setToDelete] = useState<api.StickerObject | null>(null);
  const hideRes = useResource(`default-stickers:${guildId}`, () => api.getDefaultStickers(guildId));
  const hide = useAction();
  const toggleDefaults = async (hidden: boolean) => {
    const done = await hide.run(() => api.setDefaultStickers(guildId, hidden));
    if (done) {
      resetDefaultPacks(guildId);
      hideRes.set(done);
    }
  };

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
      t('settings.expressions.stickerUploaded'),
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
      <Switch
        label={t('settings.expressions.hideDefaultStickers')}
        checked={hideRes.data?.hidden ?? false}
        disabled={hide.busy || !hideRes.data}
        onChange={(e) => void toggleDefaults(e.target.checked)}
      />
      <p className="ss-sub">{t('settings.expressions.hideDefaultStickersHint')}</p>
      <ActionStatus error={hide.error} done={null} />
      <p className="ss-sub">
        {t('settings.expressions.stickerRules', { kb: String(LIMITS.maxStickerBytes / 1024) })}
      </p>
      <p>
        <Badge>
          {t('settings.expressions.stickerCount', {
            count: String(list.length),
            max: String(LIMITS.stickers),
          })}
        </Badge>
      </p>
      <FileDrop
        label={t('settings.expressions.chooseSticker')}
        accept={`${IMAGE_ACCEPT},.json`}
        maxSize={LIMITS.maxStickerBytes}
        onFiles={(f) => void pick(f)}
        onReject={(r) => setRejected(rejectText(r, LIMITS.maxStickerBytes))}
      />
      {rejected && <ActionStatus error={rejected} />}
      {pending && (
        <div className="ss-card">
          <h2>{t('settings.expressions.uploadNamed', { name: pending.file.name })}</h2>
          <TextInput
            label={t('settings.expressions.stickerName')}
            value={name}
            maxLength={30}
            error={name && !nameOk ? t('settings.expressions.stickerNameRule') : undefined}
            onChange={(e) => setName(e.target.value)}
          />
          <TextInput
            label={t('settings.expressions.relatedEmoji')}
            hint={t('settings.expressions.relatedEmojiHint')}
            value={tags}
            maxLength={200}
            onChange={(e) => setTags(e.target.value)}
          />
          <TextInput
            label={t('settings.expressions.descriptionLabel')}
            value={description}
            maxLength={100}
            onChange={(e) => setDescription(e.target.value)}
          />
          <div className="ss-actions">
            <Button variant="ghost" onClick={() => setPending(null)}>
              {t('settings.expressions.cancel')}
            </Button>
            <Button
              disabled={!nameOk || !tags.trim()}
              loading={action.busy}
              onClick={() => void create()}
            >
              {t('settings.expressions.uploadSticker')}
            </Button>
          </div>
        </div>
      )}
      <ActionStatus error={action.error} done={action.done} />
      {list.length === 0 ? (
        <EmptyState
          title={t('settings.expressions.noStickersTitle')}
          description={t('settings.expressions.noStickersDescription')}
        />
      ) : (
        <ul className="ss-list" aria-label={t('settings.expressions.tabStickers')}>
          {list.map((s) => (
            <li key={s.id} className="ss-row">
              <span className="ss-emoji-img" aria-hidden="true" />
              <div className="ss-grow">
                <div className="ss-name">{s.name}</div>
                <div className="ss-meta">
                  {t('settings.expressions.stickerMeta', {
                    tags: s.tags,
                    description: s.description
                      ? t('settings.expressions.stickerDescription', { description: s.description })
                      : '',
                    name: api.userName(s.user),
                  })}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('settings.expressions.editSticker', { name: s.name })}
                onClick={() => setEditing(s)}
              >
                {t('settings.expressions.edit')}
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={t('settings.expressions.deleteStickerNamed', { name: s.name })}
                onClick={() => setToDelete(s)}
              >
                {t('settings.expressions.delete')}
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
        title={t('settings.expressions.deleteTitle', {
          name: toDelete?.name ?? t('settings.expressions.stickerWord'),
        })}
        confirmLabel={t('settings.expressions.deleteSticker')}
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
  const t = useT();
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
      title={t('settings.expressions.editTitle', { name: sticker.name })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.expressions.cancel')}
          </Button>
          <Button
            loading={action.busy}
            disabled={name.trim().length < 2}
            onClick={() => void save()}
          >
            {t('settings.expressions.save')}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <TextInput
          label={t('settings.expressions.stickerName')}
          value={name}
          maxLength={30}
          onChange={(e) => setName(e.target.value)}
        />
        <TextInput
          label={t('settings.expressions.relatedEmoji')}
          value={tags}
          maxLength={200}
          onChange={(e) => setTags(e.target.value)}
        />
        <TextInput
          label={t('settings.expressions.descriptionLabel')}
          value={description}
          maxLength={100}
          onChange={(e) => setDescription(e.target.value)}
        />
      </div>
    </Modal>
  );
}
