import { Button, Modal, Spinner, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { formatTime, useT } from '../../i18n';
import { useStore } from '../../store';
import { useGuild } from '../data';
import {
  getIncidents,
  getSecurity,
  patchSecurity,
  pauseActive,
  putIncidentActions,
  type IncidentsData,
  type SecuritySettings,
} from './joinPipeline';

/** A pause lasts a day: a minute short of it, so a slow clock never lands past the server's 24 hour bound. */
const PAUSE_MS = 24 * 3_600_000 - 60_000;

const NONE: IncidentsData = {
  invites_disabled_until: null,
  dms_disabled_until: null,
  raid_detected_at: null,
};

/**
 * Security Actions (LNG-GLD-034, D-243): the quick switches moderators reach for during a raid. "Pause invites" and
 * "Pause DMs" run for 24 hours (`PUT /guilds/{id}/incident-actions`); a paused DM between two members who are not
 * friends is refused, it does not go to Message Requests. Pausing invites with no end stays its own switch, and the
 * join-rate alert can pause invites by itself.
 */
export function SecurityActionsDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const [s, setS] = useState<SecuritySettings | null>(null);
  const [inc, setInc] = useState<IncidentsData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    Promise.all([getSecurity(guildId), getIncidents(guildId)]).then(
      ([settings, incidents]) => {
        if (!live) return;
        setS(settings);
        setInc(incidents ?? NONE);
      },
      () => live && setError(t('lounge.securityActions.loadError')),
    );
    return () => {
      live = false;
    };
  }, [guildId, t]);

  const fail = (e: unknown) =>
    setError(
      e instanceof ApiError && e.message ? e.message : t('lounge.securityActions.saveError'),
    );

  const flip = async (patch: Partial<SecuritySettings>) => {
    setBusy(true);
    setError(null);
    try {
      setS(await patchSecurity(guildId, patch));
    } catch (e) {
      fail(e);
    }
    setBusy(false);
  };

  /** Starts or ends one of the two day-long pauses; the other keeps its time. */
  const timed = async (which: 'invites_disabled_until' | 'dms_disabled_until', on: boolean) => {
    if (!inc) return;
    setBusy(true);
    setError(null);
    const keep = (v: string | null) => (pauseActive(v) ? v : null);
    const body = {
      invites_disabled_until: keep(inc.invites_disabled_until),
      dms_disabled_until: keep(inc.dms_disabled_until),
      [which]: on ? new Date(Date.now() + PAUSE_MS).toISOString() : null,
    };
    try {
      const out = await putIncidentActions(guildId, body);
      setInc(out);
      // The channel list shows "Invites paused until ..." from the guild object.
      useStore.getState().applyDispatch('GUILD_UPDATE', { id: guildId, incidents_data: out });
    } catch (e) {
      fail(e);
    }
    setBusy(false);
  };

  const invitesUntil =
    inc && pauseActive(inc.invites_disabled_until) ? inc.invites_disabled_until : null;
  const dmsUntil = inc && pauseActive(inc.dms_disabled_until) ? inc.dms_disabled_until : null;
  const until = (v: string) =>
    t('lounge.securityActions.pausedUntil', { time: formatTime(Date.parse(v)) });

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.securityActions.title')}
      description={t('lounge.securityActions.subtitle', { name: guild?.name ?? '' })}
      footer={<Button onClick={onClose}>{t('lounge.securityActions.close')}</Button>}
    >
      {!s && !error && <Spinner label={t('lounge.securityActions.loading')} />}
      {s && inc && (
        <>
          <Switch
            label={t('lounge.securityActions.pauseInvitesDay')}
            description={
              invitesUntil ? until(invitesUntil) : t('lounge.securityActions.pauseInvitesDayHint')
            }
            checked={!!invitesUntil}
            disabled={busy}
            onChange={(e) => void timed('invites_disabled_until', e.target.checked)}
          />
          <Switch
            label={t('lounge.securityActions.pauseDmsDay')}
            description={dmsUntil ? until(dmsUntil) : t('lounge.securityActions.pauseDmsDayHint')}
            checked={!!dmsUntil}
            disabled={busy}
            onChange={(e) => void timed('dms_disabled_until', e.target.checked)}
          />
          <Switch
            label={t('lounge.securityActions.pauseInvitesIndefinitely')}
            description={t('lounge.securityActions.pauseInvitesIndefinitelyHint')}
            checked={s.invites_paused}
            disabled={busy}
            onChange={(e) => void flip({ invites_paused: e.target.checked })}
          />
          <Switch
            label={t('lounge.securityActions.autoPause')}
            description={t('lounge.securityActions.autoPauseHint')}
            checked={s.raid_auto_pause_invites !== false}
            disabled={busy}
            onChange={(e) => void flip({ raid_auto_pause_invites: e.target.checked })}
          />
        </>
      )}
      {error && <p role="alert">{error}</p>}
    </Modal>
  );
}
