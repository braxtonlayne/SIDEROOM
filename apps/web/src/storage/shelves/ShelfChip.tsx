import { useT } from '../../i18n';
import { useStore } from '../../store/store';
import type { ShelfSource } from './api';

/** The words of a source chip: "Your shelf", "Alex's shelf", "Server shelf: Side Room". */
export function useChipLabel(source: ShelfSource): string {
  const t = useT();
  const user = useStore((s) => (source.owner_id ? s.users[source.owner_id] : undefined));
  const guild = useStore((s) => (source.guild_id ? s.guilds[source.guild_id] : undefined));
  switch (source.scope) {
    case 'own':
      return t('shelves.chip.own');
    case 'friend': {
      const name = user?.global_name || user?.username;
      return name ? t('shelves.chip.friend', { name }) : t('shelves.chip.friendUnknown');
    }
    case 'guild':
      return guild?.name
        ? t('shelves.chip.guild', { name: guild.name })
        : t('shelves.chip.guildUnknown');
    default:
      return t('shelves.chip.staff');
  }
}

/** Where a card's media lives, so a member always knows whose bucket it is. */
export function ShelfChip({ source }: { source: ShelfSource }) {
  const label = useChipLabel(source);
  return (
    <span className="shv-chip" data-scope={source.scope}>
      {label}
    </span>
  );
}
