import { useNavigate } from '@tanstack/react-router';
import { Icon, type IconName } from '@sideroom/ui';
import { useT } from '../../i18n';
import { firstTextChannel } from '../channels';
import { useChannels, useGuild } from '../data';
import { useLoungeGoto } from '../nav';
import { useLoungeUi } from '../store';

interface Card {
  id: string;
  title: string;
  blurb: string;
  icon: IconName;
  done: boolean;
  disabled?: boolean;
  action(): void;
}

/**
 * Welcome screen for a server with no channel open (LNG-GLD-021): a short intro and a checklist of first steps.
 * Steps that cannot be detected from client data are ticked when the person does them from here.
 */
export function ServerWelcome({ guildId }: { guildId: string }) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const goto = useLoungeGoto();
  const navigate = useNavigate();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const done = useLoungeUi((s) => s.welcomeDone);
  const markDone = useLoungeUi((s) => s.markWelcomeDone);
  if (!guild) return null;

  const key = (id: string) => `${guildId}:${id}`;
  const first = firstTextChannel(channels);
  const p = guild.permissions;
  const cards: Card[] = [
    {
      id: 'invite',
      title: t('lounge.serverWelcome.inviteTitle'),
      blurb: t('lounge.serverWelcome.inviteBlurb'),
      icon: 'users',
      done: !!done[key('invite')],
      disabled: !p.createInvite,
      action: () => {
        markDone(key('invite'));
        openDialog({ kind: 'invite', guildId });
      },
    },
    {
      id: 'icon',
      title: t('lounge.serverWelcome.iconTitle'),
      blurb: t('lounge.serverWelcome.iconBlurb'),
      icon: 'palette',
      done: !!guild.iconUrl,
      disabled: !(p.serverSettings ?? p.manageGuild),
      action: () =>
        void navigate({
          to: '/settings/server/$guildId',
          params: { guildId },
        }),
    },
    {
      id: 'message',
      title: t('lounge.serverWelcome.messageTitle'),
      blurb: t('lounge.serverWelcome.messageBlurb'),
      icon: 'hash',
      done: !!done[key('message')],
      disabled: !first,
      action: () => {
        markDone(key('message'));
        if (first) goto(guildId, first.id);
      },
    },
  ];

  return (
    <section
      className="lng-channel-view lng-welcome"
      aria-label={t('lounge.serverWelcome.welcome', { name: guild.name })}
    >
      <div className="lng-welcome-body">
        <h1 className="lng-welcome-title">
          {t('lounge.serverWelcome.welcome', { name: guild.name })}
        </h1>
        <p className="lng-muted">{t('lounge.serverWelcome.intro')}</p>
        <ul className="lng-welcome-cards" aria-label={t('lounge.serverWelcome.gettingStarted')}>
          {cards.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                className="lng-welcome-card"
                data-done={c.done || undefined}
                disabled={c.disabled}
                onClick={c.action}
              >
                <Icon name={c.icon} size={24} />
                <span className="lng-welcome-text">
                  <strong>{c.title}</strong>
                  <small>{c.blurb}</small>
                </span>
                {c.done ? (
                  <Icon name="check" size={20} label={t('lounge.serverWelcome.done')} />
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
