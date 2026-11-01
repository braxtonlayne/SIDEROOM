// ARC-GST-002 (wave 43, D-397): "Share link" for a running Arcade lobby. Any member inside the lobby can make a
// noindex `/play/<lobby_token>#key=<ephemeral_game_key>` link; the key is made in this browser and is never sent to the
// server. The link comes with a QR code so a phone can scan it and act as a controller or a drawing pad. A guest who
// opens it sees only the game: no chat, members, profiles, shelves or the call.
import { Button, Modal, QRCode } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useStore } from '../store/store';
import { activityDataPort } from '../voice/dataPort';
import { startBridge, stopBridge } from './guestBridge';
import {
  createGuestLink,
  forgetLink,
  httpGuestLinkApi,
  linkHref,
  loadLink,
  type GuestLinkApi,
  type StoredLink,
} from './guestLinks';
import type { ActivityInstance } from './types';

/** The manifest capability a bundle declares to be openable by a guest (D-399). */
export const GUEST_PLAY = 'guest_play';
export const INGAME_VOICE = 'ingame_voice';

/** Can this member share a guest link for the instance? Needs a voice-channel lobby (the data lane) and the flag. */
export function canShare(instance: ActivityInstance): boolean {
  const caps = instance.capabilities ?? [];
  return (
    instance.channel_type === 'voice' && caps.includes(GUEST_PLAY) && !caps.includes(INGAME_VOICE)
  );
}

function seat(instance: ActivityInstance, link: StoredLink) {
  startBridge({
    instanceId: instance.id,
    lobbyToken: link.lobbyToken,
    gameKey: link.gameKey,
    hostId: instance.started_by,
    selfId: useStore.getState().currentUserId ?? '',
    room: activityDataPort,
  });
}

/** Seat the bridge again after a reload of this tab, when it still holds a live link for the instance. */
export function restoreGuestBridge(instance: ActivityInstance): void {
  if (!canShare(instance)) return;
  const link = loadLink(instance.id);
  if (link) seat(instance, link);
}

export function ShareLinkDialog({
  instance,
  open,
  onClose,
  api = httpGuestLinkApi,
  origin = window.location.origin,
}: {
  instance: ActivityInstance;
  open: boolean;
  onClose: () => void;
  api?: GuestLinkApi;
  origin?: string;
}) {
  const t = useT();
  const [link, setLink] = useState<StoredLink | null>(() => loadLink(instance.id));
  const [busy, setBusy] = useState<'create' | 'revoke' | null>(null);
  const [error, setError] = useState<'create' | 'revoke' | null>(null);
  const [copied, setCopied] = useState(false);
  const [revoked, setRevoked] = useState(false);
  const field = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) setLink(loadLink(instance.id));
  }, [open, instance.id]);

  const create = async () => {
    setBusy('create');
    setError(null);
    setRevoked(false);
    try {
      const made = await createGuestLink(api, instance.id);
      setLink(made);
      seat(instance, made);
    } catch {
      setError('create');
    } finally {
      setBusy(null);
    }
  };

  const revoke = async () => {
    if (!link) return;
    setBusy('revoke');
    setError(null);
    try {
      await api.revoke(instance.id, link.lobbyToken);
      stopBridge(instance.id);
      forgetLink(instance.id);
      setLink(null);
      setRevoked(true);
    } catch {
      setError('revoke');
    } finally {
      setBusy(null);
    }
  };

  const href = link ? linkHref(origin, link) : '';
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(href);
      setCopied(true);
    } catch {
      field.current?.select();
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('guestshare.title')}
      description={t('guestshare.intro')}
      footer={<Button onClick={onClose}>{t('guestshare.close')}</Button>}
    >
      <div className="act-share">
        {!link && (
          <Button loading={busy === 'create'} onClick={() => void create()}>
            {busy === 'create' ? t('guestshare.creating') : t('guestshare.create')}
          </Button>
        )}
        {link && (
          <>
            <label className="act-share-field">
              <span>{t('guestshare.link.label')}</span>
              <input ref={field} readOnly value={href} onFocus={(e) => e.currentTarget.select()} />
            </label>
            <div className="act-share-row">
              <Button size="sm" onClick={() => void copy()}>
                {copied ? t('guestshare.copied') : t('guestshare.copy')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={busy === 'revoke'}
                onClick={() => void revoke()}
              >
                {busy === 'revoke' ? t('guestshare.revoking') : t('guestshare.revoke')}
              </Button>
            </div>
            <div className="act-share-qr">
              <QRCode value={href} label={t('guestshare.qr.label')} size={176} />
              <p className="act-muted">{t('guestshare.qr.hint')}</p>
            </div>
            <p className="act-muted">
              {t('guestshare.expires', { when: new Date(link.expiresAt).toLocaleString() })}
            </p>
            <p className="act-muted">{t('guestshare.key.note')}</p>
          </>
        )}
        {revoked && <p role="status">{t('guestshare.revoked')}</p>}
        {error && <p role="alert">{t(`guestshare.error.${error}`)}</p>}
      </div>
    </Modal>
  );
}
