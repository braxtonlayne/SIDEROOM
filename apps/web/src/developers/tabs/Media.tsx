import { Button, TextInput, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState, type FormEvent } from 'react';
import { useT } from '../../i18n';
import {
  createAppAsset,
  createAppEmoji,
  deleteAppAsset,
  deleteAppEmoji,
  errorText,
  isUnavailable,
  keys,
  listAppAssets,
  listAppEmojis,
  renameAppEmoji,
} from '../api';
import type { AppAsset, AppEmoji, DevApp } from '../types';
import { LoadError, Loading, NotAvailable } from '../ui';

/** Largest emoji upload the server takes (256 KiB). */
export const MAX_EMOJI_BYTES = 256 * 1024;
/** Largest Rich Presence asset upload (2 MiB). */
export const MAX_ASSET_BYTES = 2 * 1024 * 1024;

/** The file as a `data:` URI, which is how the API takes images. */
export function readDataUri(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error ?? new Error('Could not read the file.'));
    r.readAsDataURL(file);
  });
}

/** Suggest a valid emoji name from a file name: letters, digits and underscores, 2 to 32 long. */
export function suggestName(fileName: string, emoji: boolean): string {
  const base = fileName.replace(/\.[^.]+$/, '');
  const cleaned = emoji
    ? base.replace(/[^A-Za-z0-9_]+/g, '_')
    : base.toLowerCase().replace(/[^a-z0-9_-]+/g, '_');
  const name = cleaned.replace(/^_+|_+$/g, '').slice(0, 32);
  return emoji && name.length < 2 ? '' : name;
}

/**
 * Emojis of the application (DEV-PORT-003). They belong to the application, not to a server. The owner uploads
 * PNG, JPEG, GIF or WebP files up to 256 KiB; the server shrinks them.
 */
export function EmojisTab({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const qc = useQueryClient();
  const fileRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const q = useQuery({ queryKey: keys.emojis(app.id), queryFn: () => listAppEmojis(app.id) });
  const refresh = () => void qc.invalidateQueries({ queryKey: keys.emojis(app.id) });

  const add = useMutation({
    mutationFn: async () => {
      if (!file) throw new Error(t('developers.media.chooseFile'));
      return createAppEmoji(app.id, name.trim(), await readDataUri(file));
    },
    onSuccess: (e) => {
      setName('');
      setFile(null);
      if (fileRef.current) fileRef.current.value = '';
      setError(null);
      refresh();
      toast.show({ title: t('developers.media.emojiAdded', { name: e.name }), variant: 'success' });
    },
    onError: (e) => setError(errorText(e, t('developers.media.addFailed'))),
  });
  const rename = useMutation({
    mutationFn: (v: { id: string; name: string }) => renameAppEmoji(app.id, v.id, v.name),
    onSuccess: () => {
      setRenaming(null);
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t('developers.media.renameFailed'))),
  });
  const remove = useMutation({
    mutationFn: (e: AppEmoji) => deleteAppEmoji(app.id, e.id),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t('developers.media.removeFailed'))),
  });

  const pick = (f: File | null) => {
    setError(null);
    if (f && f.size > MAX_EMOJI_BYTES) {
      setFile(null);
      setError(t('developers.media.emojiTooLarge', { kb: MAX_EMOJI_BYTES / 1024 }));
      return;
    }
    setFile(f);
    if (f && !name) setName(suggestName(f.name, true));
  };
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (name.trim() && file) add.mutate();
  };

  return (
    <section className="dev-card" aria-labelledby="dev-emoji-h">
      <h2 id="dev-emoji-h" className="dev-card-title">
        {t('developers.media.emojiHeading')}
      </h2>
      <p className="dev-muted">{t('developers.media.emojiIntro')}</p>
      <form className="dev-actions" onSubmit={onSubmit}>
        <label className="dev-file">
          <span>{t('developers.media.image')}</span>
          <input
            ref={fileRef}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            onChange={(e) => pick(e.target.files?.[0] ?? null)}
          />
        </label>
        <TextInput
          label={t('developers.media.emojiName')}
          hint={t('developers.media.emojiNameHint')}
          value={name}
          maxLength={32}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" loading={add.isPending} disabled={!name.trim() || !file}>
          {t('developers.media.upload')}
        </Button>
      </form>
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      {q.isPending ? (
        <Loading what={t('developers.media.emojis')} />
      ) : q.error && isUnavailable(q.error) ? (
        <NotAvailable>{t('developers.media.unavailable')}</NotAvailable>
      ) : q.error ? (
        <LoadError
          what={t('developers.media.emojis')}
          error={q.error}
          onRetry={() => void q.refetch()}
        />
      ) : q.data.length === 0 ? (
        <p className="dev-muted">{t('developers.media.emojiNone')}</p>
      ) : (
        <ul className="dev-uri-list" aria-label={t('developers.media.emojiListLabel')}>
          {q.data.map((e) => (
            <li key={e.id}>
              <span className="dev-media-row">
                <img className="dev-emoji" src={e.url} alt="" width={32} height={32} />
                {renaming?.id === e.id ? (
                  <TextInput
                    label={t('developers.media.emojiName')}
                    value={renaming.name}
                    maxLength={32}
                    onChange={(ev) => setRenaming({ id: e.id, name: ev.target.value })}
                  />
                ) : (
                  <code>:{e.name}:</code>
                )}
              </span>
              <span className="dev-actions">
                {renaming?.id === e.id ? (
                  <>
                    <Button
                      size="sm"
                      loading={rename.isPending}
                      onClick={() => rename.mutate(renaming)}
                    >
                      {t('developers.media.save')}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setRenaming(null)}>
                      {t('developers.media.cancel')}
                    </Button>
                  </>
                ) : (
                  <>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label={t('developers.media.renameLabel', { name: e.name })}
                      onClick={() => setRenaming({ id: e.id, name: e.name })}
                    >
                      {t('developers.media.rename')}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={remove.isPending}
                      aria-label={t('developers.media.deleteLabel', { name: e.name })}
                      onClick={() => remove.mutate(e)}
                    >
                      {t('developers.media.delete')}
                    </Button>
                  </>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/**
 * Rich Presence art assets (DEV-PORT-003): named images (large or small) an activity can refer to. PNG, JPEG or
 * WebP up to 2 MiB; the server shrinks them to 1024 pixels.
 */
export function RichPresenceTab({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const qc = useQueryClient();
  const fileRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState('');
  const [kind, setKind] = useState<1 | 2>(1);
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);
  const q = useQuery({ queryKey: keys.assets(app.id), queryFn: () => listAppAssets(app.id) });
  const refresh = () => void qc.invalidateQueries({ queryKey: keys.assets(app.id) });

  const add = useMutation({
    mutationFn: async () => {
      if (!file) throw new Error(t('developers.media.chooseFile'));
      return createAppAsset(app.id, name.trim(), kind, await readDataUri(file));
    },
    onSuccess: (a) => {
      setName('');
      setFile(null);
      if (fileRef.current) fileRef.current.value = '';
      setError(null);
      refresh();
      toast.show({ title: t('developers.media.assetAdded', { name: a.name }), variant: 'success' });
    },
    onError: (e) => setError(errorText(e, t('developers.media.addFailed'))),
  });
  const remove = useMutation({
    mutationFn: (a: AppAsset) => deleteAppAsset(app.id, a.id),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t('developers.media.removeFailed'))),
  });

  const pick = (f: File | null) => {
    setError(null);
    if (f && f.size > MAX_ASSET_BYTES) {
      setFile(null);
      setError(t('developers.media.assetTooLarge', { mb: MAX_ASSET_BYTES / (1024 * 1024) }));
      return;
    }
    setFile(f);
    if (f && !name) setName(suggestName(f.name, false));
  };
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (name.trim() && file) add.mutate();
  };

  return (
    <section className="dev-card" aria-labelledby="dev-assets-h">
      <h2 id="dev-assets-h" className="dev-card-title">
        {t('developers.media.assetHeading')}
      </h2>
      <p className="dev-muted">{t('developers.media.assetIntro')}</p>
      <form className="dev-actions" onSubmit={onSubmit}>
        <label className="dev-file">
          <span>{t('developers.media.image')}</span>
          <input
            ref={fileRef}
            type="file"
            accept="image/png,image/jpeg,image/webp"
            onChange={(e) => pick(e.target.files?.[0] ?? null)}
          />
        </label>
        <TextInput
          label={t('developers.media.assetName')}
          hint={t('developers.media.assetNameHint')}
          value={name}
          maxLength={32}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="dev-file">
          <span>{t('developers.media.assetType')}</span>
          <select value={kind} onChange={(e) => setKind(e.target.value === '2' ? 2 : 1)}>
            <option value="1">{t('developers.media.assetLarge')}</option>
            <option value="2">{t('developers.media.assetSmall')}</option>
          </select>
        </label>
        <Button type="submit" loading={add.isPending} disabled={!name.trim() || !file}>
          {t('developers.media.upload')}
        </Button>
      </form>
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      {q.isPending ? (
        <Loading what={t('developers.media.assets')} />
      ) : q.error && isUnavailable(q.error) ? (
        <NotAvailable>{t('developers.media.unavailable')}</NotAvailable>
      ) : q.error ? (
        <LoadError
          what={t('developers.media.assets')}
          error={q.error}
          onRetry={() => void q.refetch()}
        />
      ) : q.data.length === 0 ? (
        <p className="dev-muted">{t('developers.media.assetNone')}</p>
      ) : (
        <ul className="dev-uri-list" aria-label={t('developers.media.assetListLabel')}>
          {q.data.map((a) => (
            <li key={a.id}>
              <span className="dev-media-row">
                <img className="dev-asset" src={a.url} alt="" width={48} height={48} />
                <code>{a.name}</code>
                <span className="dev-muted">
                  {a.type === 2
                    ? t('developers.media.assetSmall')
                    : t('developers.media.assetLarge')}
                </span>
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={remove.isPending}
                aria-label={t('developers.media.deleteLabel', { name: a.name })}
                onClick={() => remove.mutate(a)}
              >
                {t('developers.media.delete')}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
