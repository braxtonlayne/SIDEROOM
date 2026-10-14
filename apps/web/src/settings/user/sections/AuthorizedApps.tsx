import { Button, ConfirmDialog, Notice, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { isUnavailable, listGrants, revokeGrant } from '../../../developers/api';
import { scopeSentence } from '../../../developers/oauth';
import type { OAuthGrant } from '../../../developers/types';
import { cdnUrl } from '../../../store';
import { errorText, useResource } from '../api';
import { Card, fmtDate, Loaded, SectionTitle, Unavailable } from '../parts';

/**
 * Authorized Apps (LNG-USET-021): the OAuth2 grants the account has given to apps, with their scopes and
 * Deauthorize. When the server has no OAuth2 provider the card says so instead of showing a fake list.
 */
export function AuthorizedAppsSection() {
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
      toast.show({ title: `${target.application.name} no longer has access`, duration: 3000 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      setTarget(null);
    }
  };

  return (
    <>
      <SectionTitle>Authorized Apps</SectionTitle>
      <Card
        id="us-apps"
        title="Apps with access to your account"
        description="Apps you signed in to with SIDE ROOM, with the access they have. Remove one and it stops working at once."
      >
        {isUnavailable(res.error) ? (
          <Unavailable>
            This server cannot authorize third-party apps yet, so no app can have access to your
            account. This list will appear once it can.
          </Unavailable>
        ) : (
          <Loaded res={res} what="authorized apps">
            {(rows) =>
              rows.length === 0 ? (
                <p className="us-muted">You have not authorized any apps.</p>
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
                                style={{ borderRadius: 6, marginRight: 8, verticalAlign: 'middle' }}
                              />
                            )}
                            {g.application.name}
                          </strong>
                          <p className="us-muted">Authorized {fmtDate(g.created_at)}</p>
                          <ul className="us-scope-list" aria-label={`${g.application.name} can`}>
                            {g.scopes.map((s) => (
                              <li key={s}>{scopeSentence(s)}</li>
                            ))}
                          </ul>
                        </div>
                        <Button variant="danger" size="sm" onClick={() => setTarget(g)}>
                          Deauthorize<span className="us-sr"> {g.application.name}</span>
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
          <Notice variant="danger" title="Couldn't remove the app">
            {error}
          </Notice>
        )}
      </Card>
      <ConfirmDialog
        open={target !== null}
        danger
        title={`Deauthorize ${target?.application.name ?? 'app'}?`}
        description="It loses the access you gave it and its tokens for your account stop working. You can authorize it again later."
        confirmLabel="Deauthorize"
        loading={busy}
        onCancel={() => setTarget(null)}
        onConfirm={() => void deauthorize()}
      />
    </>
  );
}
