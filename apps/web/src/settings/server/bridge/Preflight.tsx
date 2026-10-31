import { Badge, Button } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { useResource } from '../hooks';
import { Async } from '../ui';
import * as api from './api';

/**
 * The preflight checklist (BRG-SYN-004, design 9.2): what must be true on Discord before a whole-server plan is
 * worth building. "Check again" asks the server to run the checks now. `onBlocked` tells the plan whether a blocker
 * is open so it can keep its Build button off.
 */
export function Preflight({
  guildId,
  onResult,
}: {
  guildId: string;
  onResult?: (blocked: boolean) => void;
}) {
  const t = useT();
  const res = useResource(`bridge-preflight-${guildId}`, async () => {
    const out = await api.getPreflight(guildId);
    onResult?.(out.checks.some((c) => c.blocker && !c.ok));
    return out;
  });
  return (
    <section aria-labelledby="ss-bridge-preflight" data-testid="bridge-preflight">
      <h3 id="ss-bridge-preflight" className="ss-bridge-h">
        {t('settings.bridgeSync.preflight.title')}
      </h3>
      <Async res={res} label={t('settings.bridgeSync.preflight.checking')}>
        {(pre) => (
          <>
            <ul className="ss-bridge-checks" aria-label={t('settings.bridgeSync.preflight.title')}>
              {pre.checks.map((c) => (
                <li key={c.id}>
                  <Badge variant={c.ok ? 'success' : c.blocker ? 'danger' : 'warning'}>
                    {c.ok
                      ? t('settings.bridgeSync.preflight.ok')
                      : c.blocker
                        ? t('settings.bridgeSync.preflight.fail')
                        : t('settings.bridgeSync.preflight.warn')}
                  </Badge>{' '}
                  <span>{checkLabel(c.id, t)}</span>
                  {!c.ok && c.detail && <span className="ss-sub"> {c.detail}</span>}
                </li>
              ))}
            </ul>
            {pre.invite_url && pre.checks.some((c) => !c.ok && c.blocker) && (
              <p>
                <a href={pre.invite_url} target="_blank" rel="noopener noreferrer">
                  {t('settings.bridgeSync.preflight.invite')}
                </a>
              </p>
            )}
          </>
        )}
      </Async>
      <div className="ss-actions">
        <Button size="sm" variant="secondary" onClick={() => void res.reload()}>
          {t('settings.bridgeSync.preflight.recheck')}
        </Button>
      </div>
    </section>
  );
}

function checkLabel(id: api.PreflightId, t: ReturnType<typeof useT>): string {
  switch (id) {
    case 'bot_admin':
      return t('settings.bridgeSync.preflight.botAdmin');
    case 'bot_top_role':
      return t('settings.bridgeSync.preflight.botTopRole');
    case 'intents':
      return t('settings.bridgeSync.preflight.intents');
    case 'caps':
      return t('settings.bridgeSync.preflight.caps');
    case 'unique_guild':
      return t('settings.bridgeSync.preflight.uniqueGuild');
    case 'other_integration':
      return t('settings.bridgeSync.preflight.otherIntegration');
  }
}
