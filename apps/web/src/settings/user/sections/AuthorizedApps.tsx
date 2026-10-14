import { Button, ConfirmDialog, Notice, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { isUnavailable, listGrants, revokeGrant } from '../../../developers/api';
import { scopeSentence } from '../../../developers/oauth';
import type { OAuthGrant } from '../../../developers/types';
import { useT } from '../../../i18n';
import { cdnUrl } from '../../../store';
import { errorText, useResource } from '../api';
import { Card, fmtDate, Loaded, SectionTitle, Unavailable } from '../parts';

/**
 * Authorized Apps (LNG-USET-021): the OAuth2 grants the account has given to apps, with their scopes and
 * Deauthorize. When the server has no OAuth2 provider the card says so instead of showing a fake list.
 */
export function AuthorizedAppsSection() {
  const t = useT();
  const toast = useToast();
  const res = useResource(listGrants);
  const [target, setTarget] = useState<OAuthGrant | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const deauthorize = async () => {
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      await revokeGrant(target.id);
      res.setData((d) => d?.filter((x) => x.id !== target.id));
      toast.show({
        title: t('settings.authorizedApps.revoked', { name: target.application.name }),
        duration: 3000,
      });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      setTarget(null);
    }
  };

  return (
    <>
      <SectionTitle>{t('settings.authorizedApps.title')}</SectionTitle>
      <Card
        id="us-apps"
        title={t('settings.authorizedApps.cardTitle')}
        description={t('settings.authorizedApps.cardDesc')}
      >
        {isUnavailable(res.error) ? (
          <Unavailable>{t('settings.authorizedApps.unavailable')}</Unavailable>
        ) : (
          <Loaded res={res} what={t('settings.authorizedApps.what')}>
            {(rows) =>
              rows.length === 0 ? (
                <p className="us-muted">{t('settings.authorizedApps.empty')}</p>
              ) : (
                <ul className="us-list">
                  {rows.map((g) => {
                    const icon = cdnUrl('avatars', g.application.id, g.application.icon, {
                      size: 64,
                    });
                    return (
                      <li key={g.id}>
                        <div className="us-row-main">
                          <strong>
                            {icon && (
                              <img
                                src={icon}
                                alt=""
                                width={24}
                                height={24}
                                style={{
                                  borderRadius: 6,
                                  marginInlineEnd: 8,
                                  verticalAlign: 'middle',
                                }}
                              />
                            )}
                            {g.application.name}
                          </strong>
                          <p className="us-muted">
                            {t('settings.authorizedApps.authorizedOn', {
                              date: fmtDate(g.created_at),
                            })}
                          </p>
                          <ul
                            className="us-scope-list"
                            aria-label={t('settings.authorizedApps.canAria', {
                              name: g.application.name,
                            })}
                          >
                            {g.scopes.map((s) => (
                              <li key={s}>{scopeSentence(s)}</li>
                            ))}
                          </ul>
                        </div>
                        <Button variant="danger" size="sm" onClick={() => setTarget(g)}>
                          {t('settings.authorizedApps.deauthorize')}
                          <span className="us-sr"> {g.application.name}</span>
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              )
            }
          </Loaded>
        )}
        {error && (
          <Notice variant="danger" title={t('settings.authorizedApps.removeError')}>
            {error}
          </Notice>
        )}
      </Card>
      <ConfirmDialog
        open={target !== null}
        danger
        title={t('settings.authorizedApps.confirmTitle', {
          name: target?.application.name ?? t('settings.authorizedApps.appFallback'),
        })}
        description={t('settings.authorizedApps.confirmDesc')}
        confirmLabel={t('settings.authorizedApps.deauthorize')}
        loading={busy}
        onCancel={() => setTarget(null)}
        onConfirm={() => void deauthorize()}
      />
    </>
  );
}
