// People tab (WP-FE-005): host and controller badges, ready and buffering marks, speaking indicators from the
// voice call when the party is bound to a voice channel the viewer is connected to, host actions, and the
// host's party settings.
import { Avatar, Badge, Button, DropdownMenu, Icon, Switch, type MenuEntry } from '@sideroom/ui';
import { useVoiceParticipants } from '../voice/hooks';
import { CONTROL_LABEL, httpPartyApi } from './api';
import { partyController, useParty } from './runtime';
import type { ControlMode, Participant } from './types';

const MODES: ControlMode[] = ['host', 'controllers', 'everyone'];

export function PeoplePanel() {
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
      label: p.is_controller ? 'Remove controller' : 'Let them control playback',
      onSelect: () =>
        act(async () =>
          c.adoptRest(await httpPartyApi.setController(party.id, p.user_id, !p.is_controller)),
        ),
    },
    {
      label: 'Make host',
      onSelect: () =>
        act(async () => c.adoptRest(await httpPartyApi.transferHost(party.id, p.user_id))),
    },
    { type: 'separator' },
    {
      label: 'Remove from party',
      danger: true,
      onSelect: () => act(() => httpPartyApi.kick(party.id, p.user_id)),
    },
  ];

  return (
    <div className="wp-people">
      <h3 className="wp-h">
        In this party <span className="wp-muted">({party.participants.length})</span>
      </h3>
      <ul className="wp-list" aria-label="Participants">
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
                  {p.user_id === me ? ' (you)' : ''}
                </span>
                <span className="wp-badges">
                  {p.is_host && <Badge variant="accent">Host</Badge>}
                  {!p.is_host && p.is_controller && <Badge>Controller</Badge>}
                  {p.buffering && <Badge variant="warning">Buffering</Badge>}
                  {p.ready && <Badge variant="success">Ready</Badge>}
                  {inVoice.has(p.user_id) && <Badge>In voice</Badge>}
                  {isSpeaking && <span className="wp-sr-only">speaking</span>}
                  {raised && <Badge variant="accent">{'✋'} wants control</Badge>}
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
                  aria-label={`Let ${name} control playback`}
                >
                  Grant
                </Button>
              )}
              {manage && p.user_id !== me && (
                <DropdownMenu
                  label={`Actions for ${name}`}
                  items={menu(p)}
                  trigger={
                    <Button size="sm" variant="ghost" iconOnly aria-label={`Actions for ${name}`}>
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
    <section className="wp-settings" aria-label="Party settings">
      <h3 className="wp-h">Party settings</h3>
      <label className="wp-field">
        <span>Who can play, pause and seek</span>
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
        <span>Who can add to the queue</span>
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
        label="Pause everyone when someone is buffering"
        checked={s.auto_pause_buffering}
        onChange={(e) => patch({ auto_pause_buffering: e.target.checked })}
      />
    </section>
  );
}
