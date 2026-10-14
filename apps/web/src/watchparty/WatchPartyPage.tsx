// `/cinema/party/$id?code=`: opens (joins) a party and shows the theatre. The controller outlives this page: leaving
// it hands the picture to the mini player while the party keeps its clock, heartbeat and drift correction.
import { Button, EmptyState, Spinner } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useEffect } from 'react';
import { useT } from '../i18n';
import { useAgeGate } from '../lib/ageGate';
import { partyController, useParty } from './runtime';
import { Theatre } from './Theatre';
import './watchparty.css';

export interface WatchPartyPageProps {
  id: string;
  code?: string;
}

export function WatchPartyPage({ id, code }: WatchPartyPageProps) {
  const t = useT();
  const router = useRouter();
  const { ensureAdult } = useAgeGate();
  const c = partyController();
  const status = useParty((s) => s.status);
  const error = useParty((s) => s.error);
  const party = useParty((s) => s.party);

  const back = () => {
    if (router.history.canGoBack()) router.history.back();
    else router.history.push('/cinema');
  };

  useEffect(() => {
    let dead = false;
    void (async () => {
      await c.open(id, { code });
      // 18+ content and an unconfirmed account: ask once, then try again (D-184).
      if (!dead && c.state.error?.kind === 'age' && (await ensureAdult()) && !dead)
        await c.open(id, { code });
    })();
    return () => {
      dead = true;
    };
    // The controller and the age gate are stable for the page's life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, code]);

  if (status === 'active' && party) return <Theatre onExit={back} />;
  if (status === 'error' || status === 'ended') {
    const gone = error?.kind === 'gone';
    return (
      <div className="wp-root wp-msg">
        <EmptyState
          title={
            status === 'ended'
              ? error?.kind === 'removed'
                ? t('watchparty.page.removed')
                : t('watchparty.page.ended')
              : error?.kind === 'age'
                ? t('watchparty.page.ageTitle')
                : gone
                  ? t('watchparty.page.gone')
                  : t('watchparty.page.couldNotOpen')
          }
          description={
            error?.kind === 'age'
              ? t('watchparty.page.ageDescription')
              : (error?.message ?? t('watchparty.page.checkConnection'))
          }
          action={
            <Button onClick={back} variant="secondary">
              {t('watchparty.page.back')}
            </Button>
          }
        />
      </div>
    );
  }
  return (
    <div className="wp-root wp-msg">
      <Spinner label={t('watchparty.page.joining')} />
    </div>
  );
}
