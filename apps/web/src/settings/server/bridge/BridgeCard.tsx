import { Button, Notice, Switch } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { ActionStatus, useAction } from '../ui';
import * as api from './api';
import { useBridgeModule } from './moduleStore';
import { TeardownDialog } from './TeardownDialog';

/**
 * The Discord bridge's card under Modules > "Integrations, more" (BRG-UI-001, D-381). The privacy notice comes
 * before the switch. Only the server owner can use the switch (a manager sees it greyed with the reason); switching
 * off goes through the teardown dialog, never straight to the server.
 */
export function BridgeCard({
  guildId,
  feature,
  isOwner,
  onChanged,
}: {
  guildId: string;
  feature: api.BridgeFeature;
  isOwner: boolean;
  onChanged: (next: api.BridgeFeature) => void;
}) {
  const t = useT();
  const navigate = useNavigate();
  const act = useAction();
  const [tearing, setTearing] = useState(false);
  const setModule = useBridgeModule((s) => s.set);
  const whole = useBridgeModule((s) => s.wholeByGuild[guildId] === true);
  const locked = !isOwner || feature.platform_disabled;

  const switchOn = () =>
    void act.run(async () => {
      const next = await api.switchBridge(guildId, true);
      setModule(guildId, next.enabled && !next.platform_disabled);
      onChanged(next);
    });
  const onToggle = (on: boolean) => {
    if (on) switchOn();
    else setTearing(true);
  };

  return (
    <article className="ss-card" aria-labelledby="feature-bridge-h" data-testid="bridge-card">
      <h3 id="feature-bridge-h" className="ss-card-title">
        {feature.name}
      </h3>
      <p className="ss-desc">{feature.description}</p>
      <Notice variant="info" title={t('settings.bridge.card.noticeTitle')}>
        {t('settings.bridge.card.notice')}
      </Notice>
      <p className="ss-sub">{t('settings.bridgeSync.card.wholeNote')}</p>
      {feature.platform_disabled && (
        <Notice variant="warning">{t('settings.features.platformOff')}</Notice>
      )}
      <Switch
        label={t('settings.features.enable', { name: feature.name })}
        checked={feature.enabled}
        disabled={locked || act.busy}
        onChange={(e) => onToggle(e.target.checked)}
      />
      {!isOwner && (
        <p className="ss-sub" data-testid="bridge-owner-only">
          {feature.enabled
            ? t('settings.bridge.card.ownerOnlyOff')
            : t('settings.bridge.card.ownerOnlyOn')}
        </p>
      )}
      <ActionStatus error={act.error} />
      {feature.enabled && !feature.platform_disabled && (
        <Button
          size="sm"
          variant="secondary"
          onClick={() =>
            void navigate({
              to: '/settings/server/$guildId/$section',
              params: { guildId, section: 'bridge' },
            })
          }
        >
          {t('settings.bridge.card.open')}
        </Button>
      )}
      {tearing && (
        <TeardownDialog
          guildId={guildId}
          whole={whole}
          onClose={(after) => {
            setTearing(false);
            if (after) {
              setModule(guildId, false);
              onChanged(after);
            }
          }}
        />
      )}
    </article>
  );
}
