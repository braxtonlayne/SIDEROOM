// The smaller "Game servers: coming soon" tile on the voice Activities picker (GSV-PAGE-001). Cosmetic: the same two
// tiers as the settings page, no mock-up, nothing to press.
import { useT } from '../../../i18n';
import './gameservers.css';

export function GameServersTile() {
  const t = useT();
  return (
    <aside className="gsv-tile" aria-labelledby="gsv-tile-title">
      <h3 id="gsv-tile-title" className="gsv-tile-title">
        {t('activities.launcher.gameServersTitle')}
      </h3>
      <p className="gsv-tile-text">{t('activities.launcher.gameServersText')}</p>
      <ul className="gsv-tile-tiers">
        <li className="gsv-pill gsv-pill-soon">{t('settings.gameServers.byocTitle')}</li>
        <li className="gsv-pill gsv-pill-soon">{t('settings.gameServers.managedTitle')}</li>
      </ul>
    </aside>
  );
}
