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
import { useResource } from '../settings/server/hooks';
import { Async } from '../settings/server/ui';
import * as api from './api';
import { usd, type Board } from './api';
import './support.css';
import '../settings/user/user-settings.css';

function BoardList({ board, mine }: { board: Board; mine: boolean }) {
  if (board.items.length === 0)
    return (
      <EmptyState
        title="Nobody on this board yet"
        description={
          board.board === 'donors'
            ? 'Donations show up here once they are confirmed.'
            : 'Servers appear here when members support them.'
        }
      />
    );
  return (
    <>
      <ol
        className="sup-board"
        aria-label={board.board === 'donors' ? 'Top donors' : 'Top servers'}
      >
        {board.items.map((i) => (
          <li key={`${i.rank}-${i.user_id ?? i.guild_id ?? i.name}`}>
            <span className="sup-rank">#{i.rank}</span>
            <span className="sup-name">{i.name}</span>
            {i.anonymous && <Badge variant="neutral">Hidden</Badge>}
            <span className="sup-muted">{i.cents === null ? 'Amount hidden' : usd(i.cents)}</span>
          </li>
        ))}
      </ol>
      {mine && board.me && (
        <p className="sup-muted" role="status">
          Your rank: #{board.me.rank} ({usd(board.me.cents)})
        </p>
      )}
    </>
  );
}

function Pane({ kind, window }: { kind: 'donors' | 'servers'; window: '30d' | 'lifetime' }) {
  const res = useResource(`${kind}-${window}`, () => api.getBoard(kind, window));
  return (
    <Async res={res} label="Loading the leaderboard">
      {(b) => <BoardList board={b} mine={kind === 'donors'} />}
    </Async>
  );
}

export function LeaderboardsPage() {
  const [window, setWindow] = useState<'30d' | 'lifetime'>('30d');
  return (
    <ThemeProvider section="lounge" className="ss-root">
      <ToastProvider>
        <main className="sup-page">
          <h1 className="us-title">Leaderboards</h1>
          <p className="sup-muted">
            Thank you to everyone keeping the lights on. Donors can hide their name or amount in
            Settings.{' '}
            <Link to="/settings/user/$section" params={{ section: 'support' }}>
              Support
            </Link>
          </p>
          <div className="sup-bar" role="group" aria-label="Time window">
            <Button
              variant={window === '30d' ? 'primary' : 'secondary'}
              aria-pressed={window === '30d'}
              onClick={() => setWindow('30d')}
            >
              Last 30 days
            </Button>
            <Button
              variant={window === 'lifetime' ? 'primary' : 'secondary'}
              aria-pressed={window === 'lifetime'}
              onClick={() => setWindow('lifetime')}
            >
              Lifetime
            </Button>
          </div>
          <Tabs
            label="Leaderboard"
            items={[
              {
                id: 'donors',
                label: 'Top donors',
                content: <Pane kind="donors" window={window} />,
              },
              {
                id: 'servers',
                label: 'Top servers',
                content: <Pane kind="servers" window={window} />,
              },
            ]}
          />
          <Notice variant="info">
            Contributor boards for approved movies, episodes and chapters arrive with the review
            pipeline.
          </Notice>
        </main>
      </ToastProvider>
    </ThemeProvider>
  );
}
