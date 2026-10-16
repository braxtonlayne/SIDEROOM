import { Button, ConfirmDialog, EmptyState, Modal, TextInput } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { apiFetch } from '../../../api/client';
import { useT } from '../../../i18n';
import { parseSound, soundAudio, type Sound } from '../../../voice/soundboard';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';

/** Limits mirrored from crates/sr-chat/src/settings/soundboard.rs. */
export const SOUND_LIMITS = { maxBytes: 512 * 1024, maxSeconds: 5, nameMin: 2, nameMax: 32 };

interface SoundRow extends Sound {
  uploader: string;
}

const listSounds = async (guildId: string): Promise<SoundRow[]> => {
  const raw = await apiFetch<{ items?: Record<string, unknown>[] }>(
    `/guilds/${guildId}/soundboard-sounds`,
    { retries: 0 },
  );
  return (raw.items ?? []).flatMap((o) => {
    const s = parseSound(o);
    if (!s) return [];
    const u = o.user as api.UserRef | undefined;
    return [{ ...s, guildId, uploader: u ? api.userName(u) : '' }];
  });
};

interface SoundBody {
  name?: string;
  volume?: number;
  emoji_name?: string | null;
  sound?: string;
}

const createSound = (guildId: string, b: SoundBody) =>
  apiFetch<unknown>(`/guilds/${guildId}/soundboard-sounds`, {
    method: 'POST',
    json: b,
    retries: 0,
  });
const patchSound = (guildId: string, id: string, b: SoundBody) =>
  apiFetch<unknown>(`/guilds/${guildId}/soundboard-sounds/${id}`, {
    method: 'PATCH',
    json: b,
    retries: 0,
  });
const deleteSound = (guildId: string, id: string) =>
  apiFetch<void>(`/guilds/${guildId}/soundboard-sounds/${id}`, { method: 'DELETE', retries: 0 });

/** Length of an audio file in seconds, read by the browser; rejects when it cannot decode the file. */
export function audioSeconds(file: Blob): Promise<number> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const a = new Audio();
    const done = () => URL.revokeObjectURL(url);
    a.preload = 'metadata';
    a.onloadedmetadata = () => {
      done();
      resolve(a.duration);
    };
    a.onerror = () => {
      done();
      reject(new Error('unreadable'));
    };
    a.src = url;
  });
}

const readDataUri = (file: Blob) =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error('read'));
    r.readAsDataURL(file);
  });

/**
 * Soundboard (LNG-SSET-025): the server's sounds with play, rename, volume and delete, and an upload dialog.
 * A file over five seconds is refused with its length instead of trimmed: the server keeps files as uploaded (D-201).
 */
export function SoundboardTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`sounds:${guildId}`, () => listSounds(guildId));
  useGuildEvents(guildId, ['GUILD_SOUNDBOARD_SOUNDS_UPDATE'], () => void res.reload());
  const [uploading, setUploading] = useState(false);
  const [toDelete, setToDelete] = useState<SoundRow | null>(null);
  const action = useAction();

  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await deleteSound(guildId, toDelete.id);
      return true;
    });
    if (ok) {
      setToDelete(null);
      await res.reload();
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">
        {t('settings.soundboard.rules', {
          kb: String(SOUND_LIMITS.maxBytes / 1024),
          seconds: String(SOUND_LIMITS.maxSeconds),
        })}
      </p>
      <div className="ss-actions">
        <Button onClick={() => setUploading(true)}>{t('settings.soundboard.upload')}</Button>
      </div>
      <ActionStatus error={action.error} done={action.done} />
      <Async res={res} label={t('settings.soundboard.loading')}>
        {(list) =>
          list.length === 0 ? (
            <EmptyState
              title={t('settings.soundboard.emptyTitle')}
              description={t('settings.soundboard.emptyDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.soundboard.listLabel')}>
              {list.map((s) => (
                <SoundItem
                  key={s.id}
                  guildId={guildId}
                  sound={s}
                  onChanged={res.reload}
                  onDelete={() => setToDelete(s)}
                />
              ))}
            </ul>
          )
        }
      </Async>
      {uploading && (
        <UploadDialog
          guildId={guildId}
          onClose={() => setUploading(false)}
          onUploaded={async () => {
            setUploading(false);
            await res.reload();
          }}
        />
      )}
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={t('settings.soundboard.deleteTitle', { name: toDelete?.name ?? '' })}
        description={t('settings.soundboard.deleteBody')}
        confirmLabel={t('settings.soundboard.delete')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function SoundItem({
  guildId,
  sound,
  onChanged,
  onDelete,
}: {
  guildId: string;
  sound: SoundRow;
  onChanged: () => Promise<void>;
  onDelete: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(sound.name);
  const [volume, setVolume] = useState(Math.round(sound.volume * 100));
  const action = useAction();
  const playing = useRef<HTMLAudioElement | null>(null);
  const validName =
    name.trim().length >= SOUND_LIMITS.nameMin && name.trim().length <= SOUND_LIMITS.nameMax;
  const dirty = name.trim() !== sound.name || volume !== Math.round(sound.volume * 100);

  const play = () =>
    void action.run(async () => {
      playing.current?.pause();
      const buf = await soundAudio(sound);
      const url = URL.createObjectURL(new Blob([buf]));
      const a = new Audio(url);
      a.volume = volume / 100;
      a.onended = () => URL.revokeObjectURL(url);
      playing.current = a;
      await a.play();
    });
  const save = async () => {
    const ok = await action.run(async () => {
      await patchSound(guildId, sound.id, { name: name.trim(), volume: volume / 100 });
      return true;
    }, t('settings.soundboard.saved'));
    if (ok) await onChanged();
  };

  return (
    <li className="ss-row">
      <Button
        size="sm"
        variant="secondary"
        aria-label={t('settings.soundboard.play', { name: sound.name })}
        onClick={play}
      >
        {t('settings.soundboard.playShort')}
      </Button>
      <span aria-hidden="true">{sound.emoji ?? ''}</span>
      <div className="ss-grow">
        <TextInput
          label={t('settings.soundboard.nameOf', { name: sound.name })}
          value={name}
          maxLength={SOUND_LIMITS.nameMax}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="ss-field-label" htmlFor={`ss-sound-vol-${sound.id}`}>
          {t('settings.soundboard.volumeOf', { name: sound.name, percent: volume })}
        </label>
        <input
          id={`ss-sound-vol-${sound.id}`}
          type="range"
          min={0}
          max={100}
          value={volume}
          onChange={(e) => setVolume(Number(e.target.value))}
        />
        {sound.uploader && (
          <div className="ss-meta">
            {t('settings.soundboard.uploadedBy', { name: sound.uploader })}
          </div>
        )}
        <ActionStatus error={action.error} done={action.done} />
      </div>
      <Button
        size="sm"
        disabled={!dirty || !validName || action.busy}
        aria-label={t('settings.soundboard.saveNamed', { name: sound.name })}
        onClick={() => void save()}
      >
        {t('settings.soundboard.save')}
      </Button>
      <Button
        size="sm"
        variant="danger"
        aria-label={t('settings.soundboard.deleteNamed', { name: sound.name })}
        onClick={onDelete}
      >
        {t('settings.soundboard.delete')}
      </Button>
    </li>
  );
}

function UploadDialog({
  guildId,
  onClose,
  onUploaded,
}: {
  guildId: string;
  onClose: () => void;
  onUploaded: () => Promise<void>;
}) {
  const t = useT();
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState('');
  const [emoji, setEmoji] = useState('');
  const [volume, setVolume] = useState(100);
  const [fileError, setFileError] = useState<string | null>(null);
  const action = useAction();
  const validName =
    name.trim().length >= SOUND_LIMITS.nameMin && name.trim().length <= SOUND_LIMITS.nameMax;

  const choose = async (f: File | undefined) => {
    setFile(null);
    setFileError(null);
    if (!f) return;
    if (f.size > SOUND_LIMITS.maxBytes) {
      setFileError(t('settings.soundboard.tooLarge', { kb: String(SOUND_LIMITS.maxBytes / 1024) }));
      return;
    }
    try {
      const seconds = await audioSeconds(f);
      if (seconds > SOUND_LIMITS.maxSeconds) {
        setFileError(
          t('settings.soundboard.tooLong', {
            seconds: seconds.toFixed(1),
            max: String(SOUND_LIMITS.maxSeconds),
          }),
        );
        return;
      }
    } catch {
      setFileError(t('settings.soundboard.unreadable'));
      return;
    }
    setFile(f);
    if (!name)
      setName(
        f.name
          .replace(/\.[^.]+$/, '')
          .replace(/[^A-Za-z0-9_ -]+/g, '')
          .slice(0, 32),
      );
  };

  const submit = async () => {
    if (!file) return;
    const ok = await action.run(async () => {
      await createSound(guildId, {
        name: name.trim(),
        sound: await readDataUri(file),
        volume: volume / 100,
        ...(emoji.trim() ? { emoji_name: emoji.trim() } : {}),
      });
      return true;
    });
    if (ok) await onUploaded();
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.soundboard.uploadTitle')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.soundboard.cancel')}
          </Button>
          <Button disabled={!file || !validName || action.busy} onClick={() => void submit()}>
            {t('settings.soundboard.uploadSubmit')}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <label className="ss-field-label" htmlFor="ss-sound-file">
          {t('settings.soundboard.file')}
        </label>
        <input
          id="ss-sound-file"
          type="file"
          accept=".mp3,.ogg,.wav,audio/mpeg,audio/ogg,audio/wav"
          onChange={(e) => void choose(e.target.files?.[0])}
        />
        {fileError && <ActionStatus error={fileError} />}
        <TextInput
          label={t('settings.soundboard.name')}
          value={name}
          maxLength={SOUND_LIMITS.nameMax}
          onChange={(e) => setName(e.target.value)}
        />
        <TextInput
          label={t('settings.soundboard.emoji')}
          value={emoji}
          maxLength={8}
          onChange={(e) => setEmoji(e.target.value)}
        />
        <label className="ss-field-label" htmlFor="ss-sound-new-vol">
          {t('settings.soundboard.volume', { percent: volume })}
        </label>
        <input
          id="ss-sound-new-vol"
          type="range"
          min={0}
          max={100}
          value={volume}
          onChange={(e) => setVolume(Number(e.target.value))}
        />
        <ActionStatus error={action.error} />
      </div>
    </Modal>
  );
}
