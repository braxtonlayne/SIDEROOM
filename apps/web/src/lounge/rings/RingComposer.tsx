// Post a ring: text, photo, clip or track, to mutual friends or to the servers chosen. The checks here only spare a
// round trip; the server enforces every one of them (size, 60 seconds, the daily limit, membership of the servers).
import { Button, Checkbox, Modal, Radio, RadioGroup, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useGuilds } from '../data';
import { postRing, uploadRing, type RingAudience, type RingKind } from './api';
import { refreshRings } from './store';
import './rings.css';

const MAX_PHOTO = 8 * 1024 * 1024;
const MAX_CLIP = 50 * 1024 * 1024;
const MAX_CLIP_MS = 60_000;

/** A clip's length from its own metadata. */
function clipLength(file: Blob): Promise<number> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const v = document.createElement('video');
    v.preload = 'metadata';
    v.onloadedmetadata = () => {
      URL.revokeObjectURL(url);
      resolve(v.duration * 1000);
    };
    v.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('unreadable'));
    };
    v.src = url;
  });
}

export function RingComposer({ onClose }: { onClose: () => void }) {
  const t = useT();
  const guilds = useGuilds();
  const [kind, setKind] = useState<RingKind>('text');
  const [text, setText] = useState('');
  const [title, setTitle] = useState('');
  const [artist, setArtist] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [audience, setAudience] = useState<RingAudience>('friends');
  const [chosen, setChosen] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = (id: string) =>
    setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));

  const submit = async () => {
    setError(null);
    const hasFile = kind === 'photo' || kind === 'clip';
    if (kind === 'text' && !text.trim()) return setError(t('lounge.rings.composer.errorNeedText'));
    if (kind === 'track' && !title.trim())
      return setError(t('lounge.rings.composer.errorNeedTitle'));
    if (hasFile && !file) return setError(t('lounge.rings.composer.errorNeedFile'));
    if (audience === 'guilds' && chosen.length === 0)
      return setError(t('lounge.rings.composer.errorNeedServers'));
    setBusy(true);
    try {
      const guildIds = audience === 'guilds' ? chosen : [];
      if (kind === 'text' || kind === 'track') {
        await postRing({
          kind,
          audience,
          guildIds,
          ...(kind === 'text' ? { text } : { track: { title, artist } }),
        });
      } else if (file) {
        if (kind === 'photo' && file.size > MAX_PHOTO) throw new Error('photoBig');
        let durationMs: number | undefined;
        if (kind === 'clip') {
          if (file.size > MAX_CLIP) throw new Error('clipBig');
          if (!/^video\/(mp4|webm)$/.test(file.type)) throw new Error('clipType');
          durationMs = await clipLength(file).catch(() => {
            throw new Error('clipType');
          });
          if (durationMs > MAX_CLIP_MS) throw new Error('clipLong');
        }
        await uploadRing({
          kind,
          file,
          audience,
          guildIds,
          ...(text.trim() ? { caption: text.trim() } : {}),
          ...(durationMs ? { durationMs } : {}),
        });
      }
      await refreshRings();
      onClose();
    } catch (e) {
      setBusy(false);
      if (e instanceof ApiError) {
        if (e.status === 409) return setError(t('lounge.rings.composer.errorLimit'));
        if (e.status === 413) return setError(t('lounge.rings.composer.errorTooBig'));
      } else if (e instanceof Error) {
        if (e.message === 'photoBig') return setError(t('lounge.rings.composer.errorPhotoBig'));
        if (e.message === 'clipBig') return setError(t('lounge.rings.composer.errorClipBig'));
        if (e.message === 'clipLong') return setError(t('lounge.rings.composer.errorClipLong'));
        if (e.message === 'clipType') return setError(t('lounge.rings.composer.errorClipType'));
      }
      setError(t('lounge.rings.composer.errorGeneric'));
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('lounge.rings.composer.title')}
      description={t('lounge.rings.composer.intro')}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.rings.composer.cancel')}
          </Button>
          <Button loading={busy} onClick={() => void submit()}>
            {t('lounge.rings.composer.post')}
          </Button>
        </>
      }
    >
      <div className="rng-form">
        <RadioGroup
          legend={t('lounge.rings.composer.kind')}
          orientation="horizontal"
          value={kind}
          onValueChange={(v) => {
            setKind(v as RingKind);
            setFile(null);
            setError(null);
          }}
        >
          <Radio value="text" label={t('lounge.rings.composer.kindText')} />
          <Radio value="photo" label={t('lounge.rings.composer.kindPhoto')} />
          <Radio value="clip" label={t('lounge.rings.composer.kindClip')} />
          <Radio value="track" label={t('lounge.rings.composer.kindTrack')} />
        </RadioGroup>

        {(kind === 'text' || kind === 'photo' || kind === 'clip') && (
          <TextArea
            label={
              kind === 'text' ? t('lounge.rings.composer.text') : t('lounge.rings.composer.caption')
            }
            hint={t('lounge.rings.composer.textHint')}
            maxLength={280}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        )}
        {(kind === 'photo' || kind === 'clip') && (
          <TextInput
            type="file"
            label={t('lounge.rings.composer.file')}
            hint={
              kind === 'photo'
                ? t('lounge.rings.composer.photoHint')
                : t('lounge.rings.composer.clipHint')
            }
            accept={
              kind === 'photo'
                ? 'image/png,image/jpeg,image/gif,image/webp'
                : 'video/mp4,video/webm'
            }
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
        )}
        {kind === 'track' && (
          <>
            <TextInput
              label={t('lounge.rings.composer.trackTitle')}
              maxLength={100}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <TextInput
              label={t('lounge.rings.composer.trackArtist')}
              maxLength={100}
              value={artist}
              onChange={(e) => setArtist(e.target.value)}
            />
          </>
        )}

        <RadioGroup
          legend={t('lounge.rings.composer.audience')}
          value={audience}
          onValueChange={(v) => setAudience(v as RingAudience)}
        >
          <Radio value="friends" label={t('lounge.rings.composer.audienceFriends')} />
          <Radio value="guilds" label={t('lounge.rings.composer.audienceGuilds')} />
        </RadioGroup>
        {audience === 'guilds' &&
          (guilds.length === 0 ? (
            <p className="rng-meta">{t('lounge.rings.composer.noServers')}</p>
          ) : (
            <ul className="rng-servers" aria-label={t('lounge.rings.composer.servers')}>
              {guilds.map((g) => (
                <li key={g.id}>
                  <Checkbox
                    label={g.name}
                    checked={chosen.includes(g.id)}
                    onChange={() => toggle(g.id)}
                  />
                </li>
              ))}
            </ul>
          ))}
        {error && (
          <p className="rng-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
