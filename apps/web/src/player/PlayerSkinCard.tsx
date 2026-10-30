import { Select } from '@sideroom/ui';
import { useT } from '../i18n';
import { Card } from '../settings/user/parts';
import { setDock, setSkin, usePlayerStore } from './store';
import { PLAYER_DOCKS, PLAYER_SKINS, type PlayerDock, type PlayerSkin } from './types';

const PLACE = {
  'bottom-end': 'player.place.bottomEnd',
  'bottom-start': 'player.place.bottomStart',
  'top-end': 'player.place.topEnd',
  'top-start': 'player.place.topStart',
} as const;

/** Settings > Appearance: which skin the floating player wears and where it docks (MUS-PLY-001). */
export function PlayerSkinCard() {
  const t = useT();
  const skin = usePlayerStore((s) => s.skin);
  const dock = usePlayerStore((s) => s.dock);
  return (
    <Card id="us-player" title={t('player.settings.title')} description={t('player.settings.desc')}>
      <div className="us-stack">
        <div className="us-select-row">
          <label className="us-muted" id="us-player-skin">
            {t('player.skin')}
          </label>
          <Select
            aria-labelledby="us-player-skin"
            options={PLAYER_SKINS.map((k) => ({ value: k, label: t(`player.skins.${k}`) }))}
            value={skin}
            onValueChange={(v) => setSkin(v as PlayerSkin)}
          />
        </div>
        <p className="us-muted">{t(`player.skinDesc.${skin}`)}</p>
        <div className="us-select-row">
          <label className="us-muted" id="us-player-dock">
            {t('player.settings.dock')}
          </label>
          <Select
            aria-labelledby="us-player-dock"
            options={PLAYER_DOCKS.map((d) => ({ value: d, label: t(PLACE[d]) }))}
            value={dock}
            onValueChange={(v) => setDock(v as PlayerDock)}
          />
        </div>
      </div>
    </Card>
  );
}
