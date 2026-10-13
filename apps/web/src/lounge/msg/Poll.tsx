import { Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import type { Poll } from '../data';
import { timeLeft } from './format';

export interface PollViewProps {
  poll: Poll;
  channelId: string;
  messageId: string;
  /** The viewer wrote the poll (may end it). */
  isOwn: boolean;
  onVote: (answerId: number, add: boolean) => void;
  onEnd?: (() => void) | undefined;
}

interface Voter {
  id: string;
  username: string;
  global_name?: string | null;
}

/** Poll widget (LNG-MSGUI-021): vote, results bars, voters list, time left, End Poll. */
export function PollView({ poll, channelId, messageId, isOwn, onVote, onEnd }: PollViewProps) {
  const total = poll.answers.reduce((n, a) => n + a.count, 0);
  const closed =
    poll.finalized || (!!poll.expiresAt && new Date(poll.expiresAt).getTime() <= Date.now());
  const [showResults, setShowResults] = useState(false);
  const [voters, setVoters] = useState<{ answer: number; users: Voter[] } | null>(null);
  const voted = poll.answers.some((a) => a.me);
  const reveal = closed || voted || showResults;

  useEffect(() => setShowResults(false), [messageId]);

  const openVoters = async (answerId: number) => {
    setVoters({ answer: answerId, users: [] });
    try {
      const res = await apiFetch<{ users: Voter[] }>(
        `/channels/${channelId}/polls/${messageId}/answers/${answerId}`,
      );
      setVoters({ answer: answerId, users: res.users });
    } catch {
      setVoters({ answer: answerId, users: [] });
    }
  };

  const left = closed ? 'Poll closed' : timeLeft(poll.expiresAt);
  const vlabel = voters ? (poll.answers.find((a) => a.id === voters.answer)?.text ?? '') : '';
  return (
    <section className="lng-poll" aria-label={`Poll: ${poll.question}`}>
      <h3 className="lng-poll-q">{poll.question}</h3>
      <p className="lng-muted lng-poll-hint">
        {poll.multi ? 'Select one or more answers' : 'Select one answer'}
      </p>
      <ul className="lng-poll-answers">
        {poll.answers.map((a) => {
          const pct = total > 0 ? Math.round((a.count / total) * 100) : 0;
          return (
            <li key={a.id} className="lng-poll-answer" data-me={a.me || undefined}>
              <button
                type="button"
                className="lng-poll-btn"
                role={poll.multi ? 'checkbox' : 'radio'}
                aria-checked={a.me}
                disabled={closed}
                onClick={() => onVote(a.id, !a.me)}
              >
                {reveal && (
                  <span className="lng-poll-bar" style={{ width: `${pct}%` }} aria-hidden="true" />
                )}
                <span className="lng-poll-text">
                  {a.emoji && <span aria-hidden="true">{a.emoji} </span>}
                  {a.text}
                </span>
                {reveal && (
                  <span className="lng-poll-count">
                    {pct}% <span className="lng-muted">({a.count})</span>
                  </span>
                )}
              </button>
              {reveal && a.count > 0 && (
                <button
                  type="button"
                  className="lng-link-btn lng-poll-voters"
                  onClick={() => void openVoters(a.id)}
                >
                  Voters<span className="lng-sr-only"> for {a.text}</span>
                </button>
              )}
            </li>
          );
        })}
      </ul>
      <footer className="lng-poll-foot">
        <span className="lng-muted">
          {total} {total === 1 ? 'vote' : 'votes'}
          {left ? ` • ${left}` : ''}
        </span>
        {!closed && !voted && (
          <button type="button" className="lng-link-btn" onClick={() => setShowResults((v) => !v)}>
            {showResults ? 'Hide results' : 'Show results'}
          </button>
        )}
        {!closed && isOwn && onEnd && (
          <button type="button" className="lng-link-btn lng-danger" onClick={onEnd}>
            End Poll
          </button>
        )}
      </footer>
      <Modal
        open={voters !== null}
        onClose={() => setVoters(null)}
        title={`Voters for "${vlabel}"`}
        size="sm"
      >
        {voters && voters.users.length === 0 ? (
          <p className="lng-muted">Nobody to show.</p>
        ) : (
          <ul className="lng-voter-list">
            {voters?.users.map((u) => (
              <li key={u.id}>{u.global_name || u.username}</li>
            ))}
          </ul>
        )}
      </Modal>
    </section>
  );
}
