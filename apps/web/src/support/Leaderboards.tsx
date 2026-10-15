// /support/leaderboards (DON-LB-002): top donors and top supporting servers, last 30 days or lifetime.
// Names and amounts follow each donor's privacy choices; the server already left out what must not be shown.
import {
  Badge,
  Button,
  EmptyState,
  Notice,
  Tabs,
  ThemeProvider,
  ToastProvider,
} from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { useResource } from '../settings/server/hooks';
import { Async } from '../settings/server/ui';
import * as api from './api';
import { usd, type Board, type ContributorBoard, type ContributorKind } from './api';
import './support.css';
import '../settings/user/user-settings.css';

function BoardList({ board, mine }: { board: Board; mine: boolean }) {
  const t = useT();
  if (board.items.length === 0)
    return (
      <EmptyState
        title={t('support.leaderboards.emptyTitle')}
        description={
          board.board === 'donors'
            ? t('support.leaderboards.emptyDonors')
            : t('support.leaderboards.emptyServers')
        }
      />
    );
  return (
    <>
      <ol
        className="sup-board"
        aria-label={
          board.board === 'donors'
            ? t('support.leaderboards.topDonors')
            : t('support.leaderboards.topServers')
        }
      >
        {board.items.map((i) => (
          <li key={`${i.rank}-${i.user_id ?? i.guild_id ?? i.name}`}>
            <span className="sup-rank">#{i.rank}</span>
            <span className="sup-name">{i.name}</span>
            {i.anonymous && <Badge variant="neutral">{t('support.leaderboards.hidden')}</Badge>}
            <span className="sup-muted">
              {i.cents === null ? t('support.leaderboards.amountHidden') : usd(i.cents)}
            </span>
          </li>
        ))}
      </ol>
      {mine && board.me && (
        <p className="sup-muted" role="status">
          {t('support.leaderboards.yourRank', {
            rank: board.me.rank,
            amount: usd(board.me.cents),
          })}
        </p>
      )}
    </>
  );
}

function ContributorList({ board }: { board: ContributorBoard }) {
  const t = useT();
  if (board.items.length === 0)
    return (
      <EmptyState
        title={t('support.leaderboards.emptyTitle')}
        description={t('support.leaderboards.emptyContributors')}
      />
    );
  return (
    <>
      <ol
        className="sup-board"
        aria-label={t(`support.leaderboards.contrib.${board.kind}` as MessageKey)}
      >
        {board.items.map((i) => (
          <li key={i.user_id}>
            <span className="sup-rank">#{i.rank}</span>
            <span className="sup-name">{i.name}</span>
            <span className="sup-muted">
              {t(`support.leaderboards.count.${board.kind}` as MessageKey, { count: i.count })}
            </span>
          </li>
        ))}
      </ol>
      {board.me && (
        <p className="sup-muted" role="status">
          {t('support.leaderboards.yourContributorRank', {
            rank: board.me.rank,
            count: board.me.count,
          })}
        </p>
      )}
    </>
  );
}

function ContributorPane({ kind }: { kind: ContributorKind }) {
  const t = useT();
  const res = useResource(`contrib-${kind}`, () => api.getContributorBoard(kind));
  return (
    <Async res={res} label={t('support.leaderboards.loading')}>
      {(b) => <ContributorList board={b} />}
    </Async>
  );
}

const CONTRIBUTOR_TABS: ContributorKind[] = ['movies', 'episodes', 'chapters'];

/** `?tab=movies|episodes|chapters|donors|servers` opens that tab (the Cinema and Library footers link here). */
function initialTab(): string {
  try {
    const tab = new URLSearchParams(globalThis.location?.search ?? '').get('tab') ?? '';
    return ['donors', 'servers', ...CONTRIBUTOR_TABS].includes(tab) ? tab : 'donors';
  } catch {
    return 'donors';
  }
}

function Pane({ kind, window }: { kind: 'donors' | 'servers'; window: '30d' | 'lifetime' }) {
  const t = useT();
  const res = useResource(`${kind}-${window}`, () => api.getBoard(kind, window));
  return (
    <Async res={res} label={t('support.leaderboards.loading')}>
      {(b) => <BoardList board={b} mine={kind === 'donors'} />}
    </Async>
  );
}

export function LeaderboardsPage() {
  const t = useT();
  const [window, setWindow] = useState<'30d' | 'lifetime'>('30d');
  const [tab, setTab] = useState(initialTab);
  const money = tab === 'donors' || tab === 'servers';
  return (
    <ThemeProvider section="lounge" className="ss-root">
      <ToastProvider>
        <main className="sup-page">
          <h1 className="us-title">{t('support.leaderboards.title')}</h1>
          <p className="sup-muted">
            {t('support.leaderboards.thanks')}{' '}
            <Link to="/settings/user/$section" params={{ section: 'support' }}>
              {t('support.leaderboards.supportLink')}
            </Link>
          </p>
          {money && (
            <div className="sup-bar" role="group" aria-label={t('support.leaderboards.timeWindow')}>
              <Button
                variant={window === '30d' ? 'primary' : 'secondary'}
                aria-pressed={window === '30d'}
                onClick={() => setWindow('30d')}
              >
                {t('support.leaderboards.last30')}
              </Button>
              <Button
                variant={window === 'lifetime' ? 'primary' : 'secondary'}
                aria-pressed={window === 'lifetime'}
                onClick={() => setWindow('lifetime')}
              >
                {t('support.leaderboards.lifetime')}
              </Button>
            </div>
          )}
          <Tabs
            label={t('support.leaderboards.tabs')}
            value={tab}
            onValueChange={setTab}
            items={[
              {
                id: 'donors',
                label: t('support.leaderboards.topDonors'),
                content: <Pane kind="donors" window={window} />,
              },
              {
                id: 'servers',
                label: t('support.leaderboards.topServers'),
                content: <Pane kind="servers" window={window} />,
              },
              ...CONTRIBUTOR_TABS.map((kind) => ({
                id: kind,
                label: t(`support.leaderboards.contrib.${kind}` as MessageKey),
                content: <ContributorPane kind={kind} />,
              })),
            ]}
          />
          <Notice variant="info">{t('support.leaderboards.contributorsOptOut')}</Notice>
        </main>
      </ToastProvider>
    </ThemeProvider>
  );
}
