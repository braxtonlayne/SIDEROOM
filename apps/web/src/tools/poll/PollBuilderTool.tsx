import { useState } from 'react';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import { RoomShell } from '../tierlist/RoomShell';
import type { LiveRoom } from '../tierlist/useRoom';
import {
  POLL_HOURS,
  POLL_MAX_OPTION,
  POLL_MAX_QUESTION,
  addOption,
  parsePoll,
  pollProblem,
  removeOption,
  setHead,
  setOption,
  starterPoll,
  toMessagePoll,
} from './model';

const PROBLEMS = {
  question: 'tools.poll.needQuestion',
  options: 'tools.poll.needOptions',
  long: 'tools.poll.tooLong',
} as const;

function PollEditor({ live, channelId }: { live: LiveRoom; channelId: string }) {
  const t = useT();
  const { view, edit } = live;
  const draft = parsePoll(view.state);
  const problem = pollProblem(draft);
  const [posted, setPosted] = useState(false);

  const post = () => {
    if (problem) return;
    useStore.getState().sendMessage(channelId, '', null, { poll: toMessagePoll(draft) });
    setPosted(true);
  };

  return (
    <div className="sr-poll">
      <label className="sr-room-field">
        <span>{t('tools.poll.question')}</span>
        <input
          value={draft.question}
          maxLength={POLL_MAX_QUESTION}
          onChange={(e) => edit(setHead(view.state, { question: e.target.value }))}
        />
      </label>
      <ul className="sr-poll-options" aria-label={t('tools.poll.options')}>
        {draft.options.map((o, i) => (
          <li key={o.id} className="sr-room-inline">
            <input
              className="sr-room-emoji"
              aria-label={t('tools.poll.optionEmoji', { n: i + 1 })}
              value={o.emoji}
              maxLength={8}
              onChange={(e) => {
                const set = setOption(view.state, o.id, { emoji: e.target.value });
                if (set) edit(set);
              }}
            />
            <input
              aria-label={t('tools.poll.option', { n: i + 1 })}
              value={o.text}
              maxLength={POLL_MAX_OPTION}
              onChange={(e) => {
                const set = setOption(view.state, o.id, { text: e.target.value });
                if (set) edit(set);
              }}
            />
            <button
              type="button"
              className="sr-room-x"
              aria-label={t('tools.poll.removeOption', { n: i + 1 })}
              disabled={removeOption(view.state, o.id) === null}
              onClick={() => {
                const set = removeOption(view.state, o.id);
                if (set) edit(set);
              }}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <div className="sr-room-inline">
        <button
          type="button"
          className="sr-room-btn"
          disabled={addOption(view.state) === null}
          onClick={() => {
            const set = addOption(view.state);
            if (set) edit(set);
          }}
        >
          {t('tools.poll.addOption')}
        </button>
      </div>
      <div className="sr-room-inline">
        <label className="sr-room-field">
          <span>{t('tools.poll.duration')}</span>
          <select
            value={draft.hours}
            onChange={(e) => edit(setHead(view.state, { hours: Number(e.target.value) }))}
          >
            {POLL_HOURS.map((h) => (
              <option key={h} value={h}>
                {h < 24 ? t('tools.poll.hours', { n: h }) : t('tools.poll.days', { n: h / 24 })}
              </option>
            ))}
          </select>
        </label>
        <label className="sr-room-check">
          <input
            type="checkbox"
            checked={draft.multi}
            onChange={(e) => edit(setHead(view.state, { multi: e.target.checked }))}
          />
          <span>{t('tools.poll.multi')}</span>
        </label>
      </div>
      <div className="sr-room-inline">
        <button
          type="button"
          className="sr-room-btn sr-room-primary"
          disabled={problem !== null}
          onClick={post}
        >
          {t('tools.poll.post')}
        </button>
        {problem && <span className="sr-tl-muted">{t(PROBLEMS[problem])}</span>}
        {posted && (
          <span role="status" className="sr-tl-muted">
            {t('tools.poll.posted')}
          </span>
        )}
      </div>
    </div>
  );
}

/** UTL-COL-001: the collaborative poll builder. "Post" creates the ordinary poll message. */
export default function PollBuilderTool() {
  const t = useT();
  return (
    <RoomShell kind="poll" defaultTitle={t('tools.poll.defaultTitle')} starter={starterPoll}>
      {(live, ctx) => <PollEditor live={live} channelId={ctx.channelId} />}
    </RoomShell>
  );
}
