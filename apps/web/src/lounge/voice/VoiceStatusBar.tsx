import { useEffect, useRef, useState } from 'react';
import { Button, Icon, Modal, TextInput } from '@sideroom/ui';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { MAX_STATUS, setChannelStatus, useChannelStatus } from '../../voice/channelStatus';
import type { Channel } from '../data';
import { useVoicePerms } from './perms';
import './voice-p13.css';

/** The status line of a voice channel in the sidebar: what the people in it are up to. */
export function ChannelStatusLine({ channelId }: { channelId: string }) {
  const status = useChannelStatus((s) => s.byChannel[channelId]);
  if (!status) return null;
  return (
    <p className="lng-voice-status" data-testid={`channel-status-${channelId}`} title={status}>
      {status}
    </p>
  );
}

function StatusDialog({
  open,
  initial,
  onClose,
  onSave,
}: {
  open: boolean;
  initial: string;
  onClose: () => void;
  onSave: (status: string | null) => void;
}) {
  const t = useT();
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (open) setText(initial);
  }, [open, initial]);
  const save = (value: string | null) => {
    onSave(value);
    onClose();
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('lounge.voiceStatus.dialogTitle')}
      description={t('lounge.voiceStatus.dialogHelp')}
      initialFocusRef={ref as React.RefObject<HTMLElement>}
      footer={
        <>
          {initial && (
            <Button variant="secondary" onClick={() => save(null)}>
              {t('lounge.voiceStatus.clear')}
            </Button>
          )}
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.voiceStatus.cancel')}
          </Button>
          <Button onClick={() => save(text.trim() || null)}>{t('lounge.voiceStatus.save')}</Button>
        </>
      }
    >
      <TextInput
        ref={ref}
        label={t('lounge.voiceStatus.label')}
        value={text}
        maxLength={MAX_STATUS}
        placeholder={t('lounge.voiceStatus.placeholder')}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') save(text.trim() || null);
        }}
      />
    </Modal>
  );
}

/**
 * The channel status under a voice or stage channel's header (VOX-UI-015): the current status and a pencil
 * to set or clear it. Setting needs a seat in the channel and Set Voice Channel Status, or Manage Channels;
 * the server decides, this only hides the pencil from people it would refuse.
 */
export function VoiceStatusBar({ channel, here }: { channel: Channel; here: boolean }) {
  const t = useT();
  const status = useChannelStatus((s) => s.byChannel[channel.id]) ?? '';
  const perms = useVoicePerms(channel.guildId, channel.id);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState('');
  const canEdit = perms.manageChannels || (here && perms.setVoiceStatus);
  if (!status && !canEdit) return null;

  const save = (next: string | null) => {
    setError('');
    setChannelStatus(channel.id, next).catch((e: unknown) => {
      setError(
        e instanceof ApiError && e.status === 403
          ? t('lounge.voiceStatus.errPermission')
          : t('lounge.voiceStatus.errOther'),
      );
    });
  };

  return (
    <div className="vc-status-bar">
      <p className="vc-status-text" data-testid="voice-status" data-empty={!status || undefined}>
        {status || t('lounge.voiceStatus.none')}
      </p>
      {canEdit && (
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.voiceStatus.edit')}
          onClick={() => setOpen(true)}
        >
          <Icon name="edit" size={16} />
        </button>
      )}
      {error && (
        <p role="alert" className="vc-inline-error">
          {error}
        </p>
      )}
      <StatusDialog open={open} initial={status} onClose={() => setOpen(false)} onSave={save} />
    </div>
  );
}
