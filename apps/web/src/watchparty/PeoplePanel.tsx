// People tab (WP-FE-005): host and controller badges, ready and buffering marks, speaking indicators from the
// voice call when the party is bound to a voice channel the viewer is connected to, host actions, and the
// host's party settings.
import { Avatar, Badge, Button, DropdownMenu, Icon, Switch, type MenuEntry } from '@sideroom/ui';
import { useT } from '../i18n';
import { useVoiceParticipants } from '../voice/hooks';
import { CONTROL_LABEL, httpPartyApi } from './api';
import { partyController, useParty } from './runtime';
import type { ControlMode, Participant } from './types';

const MODES: ControlMode[] = ['host', 'controllers', 'everyone'];

export function PeoplePanel() {
  const t = useT();
  const c = partyController();
  const party = useParty((s) => s.party);
  const hands = useParty((s) => s.hands);
  const me = c.meId;
  const voice = useVoiceParticipants(party?.channel_id ?? undefined);
  if (!party) return null;
  const manage = !!party.you?.can_manage;
  const speaking = new Set(voice.filter((v) => v.speaking).map((v) => v.userId));
  const inVoice = new Set(voice.map((v) => v.userId));

  const act = (fn: () => Promise<unknown>) => void fn().catch(() => undefined);

  const menu = (p: Participant): MenuEntry[] => [
    {
      label: p.is_controller
        ? t('watchparty.people.removeController')
        : t('watchparty.people.letControl'),
      onSelect: () =>
        act(async () =>
          c.adoptRest(await httpPartyApi.setController(party.id, p.user_id, !p.is_controller)),
        ),
    },
    {
      label: t('watchparty.people.makeHost'),
      onSelect: () =>
        act(async () => c.adoptRest(await httpPartyApi.transferHost(party.id, p.user_id))),
    },
    { type: 'separator' },
    {
      label: t('watchparty.people.removeFromParty'),
      danger: true,
      onSelect: () => act(() => httpPartyApi.kick(party.id, p.user_id)),
    },
  ];

  return (
    <div className="wp-people">
      <h3 className="wp-h">
        {t('watchparty.people.inThisParty')}{' '}
        <span className="wp-muted">({party.participants.length})</span>
      </h3>
      <ul className="wp-list" aria-label={t('watchparty.people.participants')}>
        {party.participants.map((p) => {
          const name = p.display_name || p.username;
          const isSpeaking = speaking.has(p.user_id);
          const raised = hands.includes(p.user_id);
          return (
            <li key={p.user_id} className="wp-row" data-speaking={isSpeaking || undefined}>
              <Avatar name={name} size={32} speaking={isSpeaking} />
              <span className="wp-row-main">
                <span className="wp-row-title">
                  {name}
                  {p.user_id === me ? t('watchparty.people.you') : ''}
                </span>
                <span className="wp-badges">
                  {p.is_host && <Badge variant="accent">{t('watchparty.people.host')}</Badge>}
                  {!p.is_host && p.is_controller && (
                    <Badge>{t('watchparty.people.controller')}</Badge>
                  )}
                  {p.buffering && (
                    <Badge variant="warning">{t('watchparty.people.buffering')}</Badge>
                  )}
                  {p.ready && <Badge variant="success">{t('watchparty.people.ready')}</Badge>}
                  {inVoice.has(p.user_id) && <Badge>{t('watchparty.people.inVoice')}</Badge>}
                  {isSpeaking && (
                    <span className="wp-sr-only">{t('watchparty.people.speaking')}</span>
                  )}
                  {raised && <Badge variant="accent">{t('watchparty.people.wantsControl')}</Badge>}
                </span>
              </span>
              {raised && manage && !p.is_controller && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    c.lowerHand(p.user_id);
                    act(async () =>
                      c.adoptRest(await httpPartyApi.setController(party.id, p.user_id, true)),
                    );
                  }}
                  aria-label={t('watchparty.people.letNameControl', { name })}
                >
                  {t('watchparty.people.grant')}
                </Button>
              )}
              {manage && p.user_id !== me && (
                <DropdownMenu
                  label={t('watchparty.people.actionsFor', { name })}
                  items={menu(p)}
                  trigger={
                    <Button
                      size="sm"
                      variant="ghost"
                      iconOnly
                      aria-label={t('watchparty.people.actionsFor', { name })}
                    >
                      <Icon name="menu" size={16} />
                    </Button>
                  }
                />
              )}
            </li>
          );
        })}
      </ul>

      {manage && <Settings />}
    </div>
  );
}

function Settings() {
  const t = useT();
  const c = partyController();
  const party = useParty((s) => s.party);
  if (!party) return null;
  const s = party.settings;
  const patch = (p: Parameters<typeof httpPartyApi.updateSettings>[1]) =>
    void httpPartyApi
      .updateSettings(party.id, p)
      .then((x) => c.adoptRest(x))
      .catch(() => undefined);
  return (
    <section className="wp-settings" aria-label={t('watchparty.people.settings')}>
      <h3 className="wp-h">{t('watchparty.people.settings')}</h3>
      <label className="wp-field">
        <span>{t('watchparty.people.whoControls')}</span>
        <select
          value={s.shared_control}
          onChange={(e) => patch({ shared_control: e.target.value as ControlMode })}
        >
          {MODES.map((m) => (
            <option key={m} value={m}>
              {CONTROL_LABEL[m]}
            </option>
          ))}
        </select>
      </label>
      <label className="wp-field">
        <span>{t('watchparty.people.whoQueues')}</span>
        <select
          value={s.queue_policy}
          onChange={(e) => patch({ queue_policy: e.target.value as ControlMode })}
        >
          {MODES.map((m) => (
            <option key={m} value={m}>
              {CONTROL_LABEL[m]}
            </option>
          ))}
        </select>
      </label>
      <Switch
        label={t('watchparty.people.autoPause')}
        checked={s.auto_pause_buffering}
        onChange={(e) => patch({ auto_pause_buffering: e.target.checked })}
      />
    </section>
  );
}
