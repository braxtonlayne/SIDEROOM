import { Avatar } from '@sideroom/ui';
import { useT } from '../../i18n';
import type { BridgedMember } from '../../settings/server/bridge/api';
import { openProfilePopout } from '../profile/popoutStore';
import { bridgedAvatar, bridgedColor } from './members';
import './bridge.css';

/** One Discord member in the sidebar group "On Discord": avatar, name in their role colour, presence, tag. */
export function BridgedMemberRow({
  member,
  guildId,
  color: roleColor,
}: {
  member: BridgedMember;
  guildId: string;
  /** Whole-server bridge: the colour of the top coloured SIDE ROOM role their Discord roles map to. */
  color?: string | null;
}) {
  const t = useT();
  const color = roleColor ?? bridgedColor(member);
  return (
    <button
      type="button"
      className="lng-member lng-bridged-member"
      aria-haspopup="dialog"
      data-offline={member.presence === 'offline' || undefined}
      aria-label={t('lounge.bridge.memberRowLabel', {
        name: member.display_name,
        status: member.presence ?? t('lounge.bridge.statusUnknown'),
      })}
      onClick={(e) => openProfilePopout({ userId: member.id, guildId }, e.currentTarget)}
    >
      <Avatar
        name={member.display_name}
        src={bridgedAvatar(member)}
        size={32}
        {...(member.presence ? { status: member.presence } : {})}
      />
      <span className="lng-member-text">
        <span className="lng-member-name" style={color ? { color } : undefined}>
          {member.display_name}
          <span className="lng-bot">
            {member.bot ? t('lounge.memberList.app') : t('lounge.messageItem.bridgeTag')}
          </span>
        </span>
      </span>
    </button>
  );
}

/** The popout of a Discord member: who they are over there, and nothing to do with them here. */
export function BridgedCard({ member }: { member: BridgedMember }) {
  const t = useT();
  const color = bridgedColor(member);
  return (
    <div className="lng-bridged-card" data-testid="bridged-card">
      <div className="lng-pop-head">
        <Avatar
          name={member.display_name}
          src={bridgedAvatar(member)}
          size={80}
          {...(member.presence ? { status: member.presence } : {})}
        />
      </div>
      <div className="lng-pop-body">
        <h2 className="lng-pop-name" style={color ? { color } : undefined}>
          {member.display_name} <span className="lng-bot">{t('lounge.messageItem.bridgeTag')}</span>
        </h2>
        <p className="lng-pop-handle">{t('lounge.bridge.popoutVia')}</p>
        {member.role_name && (
          <section aria-labelledby="bridged-role-h">
            <h3 id="bridged-role-h" className="lng-pop-h">
              {t('lounge.bridge.popoutRole')}
            </h3>
            <p>
              <span className="lng-role-chip" style={color ? { color } : undefined}>
                {member.role_name}
              </span>
            </p>
          </section>
        )}
      </div>
    </div>
  );
}
