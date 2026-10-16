// Entrance hero image (H-41, D-239): the operator's picture for the door slot on the landing page, with the alt
// text (`hero-alt`). Upload and removal need MANAGE_ENTRANCE and a fresh second factor (through `withReauth`).
import { Button, ConfirmDialog, Notice, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { resetEntranceHero } from '../../routes/entrance/entranceCopy';
import { ActionStatus, ReasonDialog, useAction } from '../ui';

/** The server takes 8 MiB before it shrinks the image. */
export const HERO_MAX_BYTES = 8 * 1024 * 1024;
export const HERO_ALT_MAX = 200;
const ACCEPT = 'image/png,image/jpeg,image/webp';

function readDataUri(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error ?? new Error('read failed'));
    r.readAsDataURL(file);
  });
}

export function HeroBlock({
  alt,
  onAltSaved,
}: {
  alt: string | undefined;
  onAltSaved: (next: string | undefined) => void;
}) {
  const t = useT();
  const fileRef = useRef<HTMLInputElement>(null);
  const [version, setVersion] = useState(0);
  const [present, setPresent] = useState<boolean | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [altText, setAltText] = useState(alt ?? '');
  const action = useAction();
  useEffect(() => setAltText(alt ?? ''), [alt]);

  // Does the server have an image? (404 means the shipped art shows.)
  useEffect(() => {
    let live = true;
    void fetch(`/api/v1/sr/entrance/hero?v=${version}`, { cache: 'no-cache' })
      .then((r) => {
        void r.body?.cancel().catch(() => undefined);
        if (live) setPresent(r.ok);
      })
      .catch(() => live && setPresent(false));
    return () => {
      live = false;
    };
  }, [version]);

  const pick = (f: File | null) => {
    setNotice(null);
    if (f && f.size > HERO_MAX_BYTES) {
      setFile(null);
      setFileError(t('admin.entranceHero.tooLarge'));
      return;
    }
    setFileError(null);
    setFile(f);
  };
  const changed = () => {
    resetEntranceHero();
    setVersion((v) => v + 1);
  };
  const altDirty = altText.trim() !== (alt ?? '').trim();
  return (
    <section className="ss-card" aria-labelledby="ent-hero-title">
      <h2 id="ent-hero-title">{t('admin.entranceHero.title')}</h2>
      <p className="ss-sub">{t('admin.entranceHero.hint')}</p>
      {present ? (
        <img
          src={`/api/v1/sr/entrance/hero?v=${version}`}
          alt={alt || t('admin.entranceHero.previewAlt')}
          style={{ maxInlineSize: '100%', maxBlockSize: 220, objectFit: 'cover' }}
        />
      ) : (
        <p className="ss-sub">{present === null ? '' : t('admin.entranceHero.none')}</p>
      )}
      <label className="ss-field">
        <span>{t('admin.entranceHero.fileLabel')}</span>
        <input
          ref={fileRef}
          type="file"
          accept={ACCEPT}
          onChange={(e) => pick(e.target.files?.[0] ?? null)}
        />
      </label>
      {fileError && (
        <Notice variant="danger" role="alert">
          {fileError}
        </Notice>
      )}
      <div className="adm-actions">
        <Button disabled={!file} onClick={() => setUploading(true)}>
          {t('admin.entranceHero.upload')}
        </Button>
        <Button variant="ghost" disabled={!present} onClick={() => setRemoving(true)}>
          {t('admin.entranceHero.remove')}
        </Button>
      </div>
      <TextInput
        label={t('admin.entranceHero.altLabel')}
        value={altText}
        maxLength={HERO_ALT_MAX}
        hint={t('admin.entranceHero.altHint', { max: HERO_ALT_MAX })}
        onChange={(e) => {
          setAltText(e.target.value);
          setNotice(null);
        }}
      />
      <div className="adm-actions">
        <Button
          variant="secondary"
          disabled={!altDirty || action.busy}
          onClick={() => {
            const next = altText.trim();
            void action
              .run(async () => {
                if (next) await api.putEntranceCopy('hero-alt', next);
                else await api.deleteEntranceCopy('hero-alt');
                return true;
              })
              .then((ok) => {
                if (ok) {
                  setNotice(t('admin.entranceHero.altSaved'));
                  onAltSaved(next || undefined);
                }
              });
          }}
        >
          {t('admin.entranceHero.saveAlt')}
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />

      <ReasonDialog
        open={uploading}
        title={t('admin.entranceHero.uploadTitle')}
        description={t('admin.entranceHero.uploadDescription')}
        confirmLabel={t('admin.entranceHero.upload')}
        reasonOptional
        reasonLabel={t('admin.entrance.reasonLabel')}
        onCancel={() => setUploading(false)}
        onConfirm={async (reason) => {
          if (!file) return;
          await api.putEntranceHero(await readDataUri(file), reason);
          setUploading(false);
          setFile(null);
          if (fileRef.current) fileRef.current.value = '';
          setNotice(t('admin.entranceHero.uploaded'));
          changed();
        }}
      />
      <ConfirmDialog
        open={removing}
        danger
        title={t('admin.entranceHero.removeTitle')}
        description={t('admin.entranceHero.removeDescription')}
        confirmLabel={t('admin.entranceHero.remove')}
        loading={action.busy}
        onCancel={() => setRemoving(false)}
        onConfirm={() => {
          void action
            .run(async () => {
              await api.deleteEntranceHero();
              return true;
            })
            .then((ok) => {
              setRemoving(false);
              if (ok) {
                setNotice(t('admin.entranceHero.removed'));
                changed();
              }
            });
        }}
      />
    </section>
  );
}
