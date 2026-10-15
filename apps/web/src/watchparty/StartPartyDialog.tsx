// "Watch together" (WP-FE-008, 009): pick where the party lives (a server channel you can post in, a DM, or a
// private party with a share link) and, when no title was given (the voice call bar), which title to start.
import { Button, Modal, Notice } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useEffect, useMemo, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { useOptionalAgeGate, isAgeGateError } from '../lib/ageGate';
import { useAllChannels, useDms, useGuilds, useUsers } from '../lounge/data';
import { holdGateway } from '../store/connect';
import { httpPartyApi } from './api';
import { TitlePicker, type PickedTitle } from './TitlePicker';

export interface StartPartyDialogProps {
  open: boolean;
  onClose: () => void;
  /** The title to watch; without it the dialog asks. */
  title?: { titleId: string; episodeId?: string | null; variantId?: string | null; label: string };
  /** A Live TV channel to watch together (G-16): no title is asked for; everyone follows the live edge. */
  liveChannel?: { channelId: string; label: string };
  /** Preselect a channel or DM (the voice call bar passes its voice channel). */
  channelId?: string | null;
}

const PRIVATE = '__private';

export function StartPartyDialog({
  open,
  onClose,
  title,
  liveChannel,
  channelId,
}: StartPartyDialogProps) {
  const t = useT();
  const router = useRouter();
  const age = useOptionalAgeGate();
  const guilds = useGuilds();
  const channels = useAllChannels();
  const dms = useDms();
  const users = useUsers();
  const [picked, setPicked] = useState<PickedTitle | null>(null);
  const [dest, setDest] = useState(channelId ?? PRIVATE);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The destination lists come from the live store; make sure the gateway is up while the dialog is open.
  useEffect(() => {
    if (!open) return;
    return holdGateway();
  }, [open]);
  useEffect(() => {
    if (open) {
      setDest(channelId ?? PRIVATE);
      setPicked(null);
      setError(null);
    }
  }, [open, channelId]);

  const groups = useMemo(
    () =>
      guilds
        .map((g) => ({
          guild: g,
          list: (channels[g.id] ?? []).filter(
            (c) =>
              (c.type === 'text' || c.type === 'announcement'
                ? c.canSend
                : c.type === 'voice' && (c.permissions?.connect ?? true)) && !c.locked,
          ),
        }))
        .filter((g) => g.list.length > 0),
    [guilds, channels],
  );

  const chosen: { titleId: string; episodeId: string | null; variantId: string | null } | null =
    liveChannel
      ? { titleId: '', episodeId: null, variantId: null }
      : title
        ? {
            titleId: title.titleId,
            episodeId: title.episodeId ?? null,
            variantId: title.variantId ?? null,
          }
        : picked
          ? { titleId: picked.titleId, episodeId: picked.episodeId, variantId: null }
          : null;
  const label = liveChannel?.label ?? title?.label ?? picked?.label;

  const start = async () => {
    if (!chosen) return;
    setBusy(true);
    setError(null);
    const where = { channel_id: dest === PRIVATE ? null : dest };
    const input = liveChannel
      ? { tv_channel_id: liveChannel.channelId, ...where }
      : {
          title_id: chosen.titleId,
          episode_id: chosen.episodeId,
          variant_id: chosen.variantId,
          ...where,
        };
    try {
      let party;
      try {
        party = await httpPartyApi.create(input);
      } catch (e) {
        if (isAgeGateError(e) && age && (await age.ensureAdult()))
          party = await httpPartyApi.create(input);
        else throw e;
      }
      onClose();
      router.history.push(`/cinema/party/${party.id}`);
    } catch (e) {
      if (isAgeGateError(e)) setError(t('watchparty.startDialog.confirmAge'));
      else if (e instanceof ApiError && e.status === 403)
        setError(t('watchparty.startDialog.forbidden'));
      else if (e instanceof ApiError && e.status === 409) setError(e.message);
      else if (e instanceof ApiError && e.status === 404)
        setError(t('watchparty.startDialog.notAvailable'));
      else setError(t('watchparty.startDialog.failed'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('watchparty.startDialog.title')}
      description={
        label
          ? t('watchparty.startDialog.descriptionTitle', { label })
          : t('watchparty.startDialog.descriptionPick')
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('watchparty.startDialog.cancel')}
          </Button>
          <Button onClick={() => void start()} loading={busy} disabled={!chosen}>
            {t('watchparty.startDialog.start')}
          </Button>
        </>
      }
    >
      <div className="wp-start">
        {!title && !liveChannel && !picked && <TitlePicker onPick={setPicked} />}
        {!title && !liveChannel && picked && (
          <p className="wp-now">
            {picked.label}{' '}
            <Button size="sm" variant="ghost" onClick={() => setPicked(null)}>
              {t('watchparty.startDialog.change')}
            </Button>
          </p>
        )}
        <label className="wp-field">
          <span>{t('watchparty.startDialog.where')}</span>
          <select
            value={dest}
            onChange={(e) => setDest(e.target.value)}
            aria-label={t('watchparty.startDialog.whereToWatch')}
          >
            <option value={PRIVATE}>{t('watchparty.startDialog.private')}</option>
            {groups.map(({ guild, list }) => (
              <optgroup key={guild.id} label={guild.name}>
                {list.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.type === 'voice' ? t('watchparty.startDialog.voicePrefix') : '#'}
                    {c.name}
                  </option>
                ))}
              </optgroup>
            ))}
            {dms.length > 0 && (
              <optgroup label={t('watchparty.startDialog.directMessages')}>
                {dms.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name ||
                      d.recipientIds
                        .map((id) => users[id]?.displayName ?? t('watchparty.startDialog.someone'))
                        .join(', ')}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </label>
        <p className="wp-muted">
          {dest === PRIVATE
            ? t('watchparty.startDialog.hintPrivate')
            : t('watchparty.startDialog.hintChannel')}
        </p>
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
      </div>
    </Modal>
  );
}
