// Server Settings > Discord bridge (BRG-UI-001). The entry exists only while the module is on (D-381). The owner
// pastes the bot token (sealed, never shown again) and switches the bridge off; the owner and MANAGE_GUILD map
// channels and post a test.
import { Button, Notice, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../../i18n';
import { useChannels } from '../../../store';
import { useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';
import * as api from './api';
import { useBridgeModule } from './moduleStore';
import { TeardownDialog } from './TeardownDialog';
import './bridge.css';

/** Text and announcement channels: the kinds a mirror can carry. */
const MAPPABLE = new Set([0, 5]);

const DIRECTIONS: api.BridgeDirection[] = ['both', 'to_discord', 'from_discord'];

export function BridgeSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const res = useResource(`bridge-${guildId}`, () => api.getBridge(guildId));
  // While the bot is connecting, look again every few seconds, so the screen says Connected on its own.
  const connecting = res.data?.state === 'connecting';
  const reload = res.reload;
  useEffect(() => {
    if (!connecting) return;
    const timer = setTimeout(() => void reload(), 3000);
    return () => clearTimeout(timer);
  }, [connecting, reload, res.data]);
  return (
    <>
      <SectionHeader
        title={t('settings.bridge.title')}
        description={t('settings.bridge.description')}
      />
      <Notice variant="info" title={t('settings.bridge.privacyTitle')}>
        {t('settings.bridge.privacy')}
      </Notice>
      <Async res={res} label={t('settings.bridge.loading')}>
        {(status) => (
          <div className="ss-form ss-bridge">
            <TokenCard
              guildId={guildId}
              isOwner={viewer.isOwner}
              status={status}
              onStatus={(next) => res.set(next)}
            />
            {status.token_set && (
              <MapCard
                guildId={guildId}
                status={status}
                onStatus={(next) => res.set(next)}
                reload={res.reload}
              />
            )}
            <SwitchOffCard guildId={guildId} isOwner={viewer.isOwner} />
          </div>
        )}
      </Async>
    </>
  );
}

// ---- the bot token and the invite link

function TokenCard({
  guildId,
  isOwner,
  status,
  onStatus,
}: {
  guildId: string;
  isOwner: boolean;
  status: api.BridgeStatus;
  onStatus: (next: api.BridgeStatus) => void;
}) {
  const t = useT();
  const act = useAction();
  const [token, setToken] = useState('');

  const save = () =>
    void act.run(async () => {
      const next = await api.putToken(guildId, token.trim());
      // The token was sent once; the field is cleared and the server keeps it sealed.
      setToken('');
      onStatus(next);
    }, t('settings.bridge.token.saved'));
  const revoke = () =>
    void act.run(async () => {
      await api.deleteToken(guildId);
      onStatus(await api.getBridge(guildId));
    }, t('settings.bridge.token.revoked'));

  return (
    <section className="ss-card" aria-labelledby="ss-bridge-token">
      <h2 id="ss-bridge-token">{t('settings.bridge.token.title')}</h2>
      {status.token_set ? (
        <>
          <p className="ss-sub" role="status">
            {status.state === 'connected'
              ? t('settings.bridge.token.connected', {
                  name: status.discord_guild_name ?? status.discord_guild_id ?? '',
                })
              : status.state === 'connecting'
                ? t('settings.bridge.token.connecting')
                : t('settings.bridge.token.problem', {
                    error:
                      status.problem === 'token_invalid'
                        ? t('settings.bridge.token.invalid')
                        : t('settings.bridge.token.disconnected'),
                  })}
          </p>
          {status.message_content_intent === false && (
            <Notice variant="warning" title={t('settings.bridge.token.intentTitle')}>
              {t('settings.bridge.token.intentBody')}
            </Notice>
          )}
          {status.invite_url && (
            <p>
              <a href={status.invite_url} target="_blank" rel="noopener noreferrer">
                {t('settings.bridge.token.invite')}
              </a>
              <span className="ss-sub"> {t('settings.bridge.token.inviteHint')}</span>
            </p>
          )}
          {isOwner ? (
            <div className="ss-actions">
              <Button size="sm" variant="secondary" disabled={act.busy} onClick={revoke}>
                {t('settings.bridge.token.revoke')}
              </Button>
            </div>
          ) : (
            <p className="ss-sub">{t('settings.bridge.token.ownerOnlyRevoke')}</p>
          )}
        </>
      ) : isOwner ? (
        <>
          <ol className="ss-bridge-steps">
            <li>
              {t('settings.bridge.token.step1')}{' '}
              <a href={api.DEVELOPER_PORTAL_URL} target="_blank" rel="noopener noreferrer">
                {t('settings.bridge.token.portal')}
              </a>
            </li>
            <li>{t('settings.bridge.token.step2')}</li>
            <li>{t('settings.bridge.token.step3')}</li>
          </ol>
          <TextInput
            label={t('settings.bridge.token.field')}
            hint={t('settings.bridge.token.fieldHint')}
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <div className="ss-actions">
            <Button disabled={act.busy || token.trim() === ''} onClick={save}>
              {t('settings.bridge.token.save')}
            </Button>
          </div>
        </>
      ) : (
        <p className="ss-sub">{t('settings.bridge.token.ownerOnlyPaste')}</p>
      )}
      <ActionStatus error={act.error} done={act.done} />
    </section>
  );
}

// ---- the channel mapping table

function MapCard({
  guildId,
  status,
  onStatus,
  reload,
}: {
  guildId: string;
  status: api.BridgeStatus;
  onStatus: (next: api.BridgeStatus) => void;
  reload: () => Promise<void>;
}) {
  const t = useT();
  const channels = useChannels(guildId).filter((c) => MAPPABLE.has(c.type));
  const discord = useResource(`bridge-discord-${guildId}`, () => api.listDiscordChannels(guildId));
  const act = useAction();
  const [ours, setOurs] = useState('');
  const [theirs, setTheirs] = useState('');
  const [direction, setDirection] = useState<api.BridgeDirection>('both');

  const mapped = new Set(status.maps.map((m) => m.channel_id));
  const free = channels.filter((c) => !mapped.has(c.id));
  const nameOf = (id: string) => channels.find((c) => c.id === id)?.name ?? id;
  const directionLabel = (d: api.BridgeDirection) =>
    d === 'both'
      ? t('settings.bridge.map.both')
      : d === 'to_discord'
        ? t('settings.bridge.map.toDiscord')
        : t('settings.bridge.map.fromDiscord');

  const add = () =>
    void act.run(async () => {
      await api.createMap(guildId, {
        channel_id: ours,
        discord_channel_id: theirs,
        direction,
      });
      setOurs('');
      setTheirs('');
      await reload();
    }, t('settings.bridge.map.added'));
  const remove = (m: api.BridgeMap) =>
    void act.run(async () => {
      await api.deleteMap(guildId, m.id);
      onStatus({ ...status, maps: status.maps.filter((x) => x.id !== m.id) });
    }, t('settings.bridge.map.removed'));
  const test = (m: api.BridgeMap) =>
    void act.run(
      async () => {
        await api.testMap(guildId, m.id);
      },
      t('settings.bridge.map.tested', { name: nameOf(m.channel_id) }),
    );

  return (
    <section className="ss-card" aria-labelledby="ss-bridge-maps">
      <h2 id="ss-bridge-maps">{t('settings.bridge.map.title')}</h2>
      <p className="ss-sub">{t('settings.bridge.map.encrypted')}</p>
      {status.maps.length === 0 ? (
        <p className="ss-na">{t('settings.bridge.map.none')}</p>
      ) : (
        <table className="ss-table" aria-label={t('settings.bridge.map.title')}>
          <thead>
            <tr>
              <th scope="col">{t('settings.bridge.map.colOurs')}</th>
              <th scope="col">{t('settings.bridge.map.colTheirs')}</th>
              <th scope="col">{t('settings.bridge.map.colDirection')}</th>
              <th scope="col">
                <span className="ss-bridge-sr">{t('settings.bridge.map.colActions')}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {status.maps.map((m) => (
              <tr key={m.id}>
                <td>#{nameOf(m.channel_id)}</td>
                <td>#{m.discord_channel_name ?? m.discord_channel_id}</td>
                <td>{directionLabel(m.direction)}</td>
                <td>
                  <div className="ss-actions">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={act.busy}
                      aria-label={t('settings.bridge.map.test', { name: nameOf(m.channel_id) })}
                      onClick={() => test(m)}
                    >
                      {t('settings.bridge.map.testShort')}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      disabled={act.busy}
                      aria-label={t('settings.bridge.map.remove', { name: nameOf(m.channel_id) })}
                      onClick={() => remove(m)}
                    >
                      {t('settings.bridge.map.removeShort')}
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <fieldset className="ss-bridge-add">
        <legend>{t('settings.bridge.map.addTitle')}</legend>
        <label className="ss-field-label">
          {t('settings.bridge.map.fieldOurs')}
          <select className="ss-select" value={ours} onChange={(e) => setOurs(e.target.value)}>
            <option value="">{t('settings.bridge.map.choose')}</option>
            {free.map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </label>
        <label className="ss-field-label">
          {t('settings.bridge.map.fieldTheirs')}
          <select className="ss-select" value={theirs} onChange={(e) => setTheirs(e.target.value)}>
            <option value="">{t('settings.bridge.map.choose')}</option>
            {(discord.data ?? []).map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </label>
        <label className="ss-field-label">
          {t('settings.bridge.map.fieldDirection')}
          <select
            className="ss-select"
            value={direction}
            onChange={(e) => setDirection(e.target.value as api.BridgeDirection)}
          >
            {DIRECTIONS.map((d) => (
              <option key={d} value={d}>
                {directionLabel(d)}
              </option>
            ))}
          </select>
        </label>
        {discord.error && <Notice variant="warning">{discord.error}</Notice>}
        <div className="ss-actions">
          <Button disabled={act.busy || !ours || !theirs} onClick={add}>
            {t('settings.bridge.map.add')}
          </Button>
        </div>
      </fieldset>
      <ActionStatus error={act.error} done={act.done} />
    </section>
  );
}

// ---- switch off

function SwitchOffCard({ guildId, isOwner }: { guildId: string; isOwner: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const setModule = useBridgeModule((s) => s.set);
  return (
    <section className="ss-card" aria-labelledby="ss-bridge-off">
      <h2 id="ss-bridge-off">{t('settings.bridge.off.title')}</h2>
      <p className="ss-sub">{t('settings.bridge.off.body')}</p>
      {isOwner ? (
        <div className="ss-actions">
          <Button variant="danger" onClick={() => setOpen(true)}>
            {t('settings.bridge.off.button')}
          </Button>
        </div>
      ) : (
        <p className="ss-sub">{t('settings.bridge.off.ownerOnly')}</p>
      )}
      {open && (
        <TeardownDialog
          guildId={guildId}
          onClose={(after) => {
            setOpen(false);
            // The entry goes with the module: Server Settings moves to the first section that is left.
            if (after) setModule(guildId, false);
          }}
        />
      )}
    </section>
  );
}
