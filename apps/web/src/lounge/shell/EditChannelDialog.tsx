import { Button, Modal, Spinner } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useCallback, useState } from 'react';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import * as api from '../../settings/server/api';
import { FormContext, useResource, useViewer, type FormHandle } from '../../settings/server/hooks';
import { ChannelEditor } from '../../settings/server/sections/Channels';
import { LoadError } from '../../settings/server/ui';
import { useLoungeRoute } from '../nav';

/**
 * Edit Channel / Edit Category (LNG-CHN-017). The same editor Server Settings uses (overview, permission
 * overwrites matrix, delete), in a modal with its own Save / Reset bar.
 */
export function EditChannelDialog({
  guildId,
  channelId,
  onClose,
}: {
  guildId: string;
  channelId: string;
  onClose: () => void;
}) {
  const t = useT();
  const res = useResource(`channels:${guildId}`, () => api.listChannels(guildId));
  const viewer = useViewer(guildId);
  const [form, setForm] = useState<FormHandle | null>(null);
  const navigate = useNavigate();
  const { channelId: current } = useLoungeRoute();
  const channels = res.data ?? [];
  const channel = channels.find((c) => c.id === channelId);
  const categories = channels.filter((c) => c.type === 4);

  const onDeleted = useCallback(() => {
    const st = useStore.getState();
    for (const c of Object.values(st.channels))
      if (c.parent_id === channelId)
        st.applyDispatch('CHANNEL_UPDATE', { id: c.id, guild_id: guildId, parent_id: null });
    st.applyDispatch('CHANNEL_DELETE', { id: channelId, guild_id: guildId });
    onClose();
    if (current === channelId)
      void navigate({ to: '/channels/$guildId', params: { guildId }, replace: true });
  }, [channelId, guildId, onClose, current, navigate]);

  const onChanged = useCallback(async () => {
    await res.reload();
  }, [res]);

  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={
        channel?.type === 4
          ? t('lounge.editChannel.categoryTitle')
          : t('lounge.editChannel.channelTitle')
      }
      footer={
        <>
          {form?.dirty && (
            <Button variant="ghost" onClick={() => form.reset()}>
              {t('lounge.editChannel.reset')}
            </Button>
          )}
          {form?.dirty && (
            <Button loading={form.saving} onClick={() => form.save()}>
              {t('lounge.editChannel.saveChanges')}
            </Button>
          )}
          {!form?.dirty && <Button onClick={onClose}>{t('lounge.editChannel.done')}</Button>}
        </>
      }
    >
      {res.error ? (
        <LoadError message={res.error} onRetry={() => void res.reload()} />
      ) : !channel ? (
        <p role="status">
          <Spinner label={t('lounge.editChannel.loading')} />
        </p>
      ) : (
        <FormContext.Provider value={setForm}>
          <ChannelEditor
            key={channel.id}
            guildId={guildId}
            viewer={viewer}
            channel={channel}
            channels={channels}
            categories={categories}
            onDirty={() => undefined}
            onChanged={onChanged}
            onDeleted={onDeleted}
          />
        </FormContext.Provider>
      )}
    </Modal>
  );
}
