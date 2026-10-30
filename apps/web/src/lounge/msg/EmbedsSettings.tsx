// Settings > Text & Images: the "Load embeds" setting of PRF-UNF-001 (default: ask on each card).
import { Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { Card } from '../../settings/user/parts';
import { errorText } from '../../settings/user/api';
import { useEnsureUserSettings } from './remoteImages';
import { setLoadEmbeds, useLoadEmbeds } from './EmbedsPrefs';

export function LoadEmbedsCard() {
  const t = useT();
  useEnsureUserSettings();
  const mode = useLoadEmbeds();
  const [error, setError] = useState<string | null>(null);
  return (
    <Card
      id="us-load-embeds"
      title={t('settings.loadEmbeds.title')}
      description={t('settings.loadEmbeds.desc')}
    >
      <Switch
        label={t('settings.loadEmbeds.always')}
        description={t('settings.loadEmbeds.alwaysDesc')}
        checked={mode === 'always'}
        onChange={(e) => {
          setError(null);
          setLoadEmbeds(e.target.checked ? 'always' : 'ask').catch((err: unknown) =>
            setError(errorText(err)),
          );
        }}
      />
      {error && (
        <Notice variant="danger" title={t('settings.loadEmbeds.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}
