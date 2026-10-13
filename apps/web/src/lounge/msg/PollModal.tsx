import { Button, Modal } from '@sideroom/ui';
import { useState } from 'react';
import type { NewPoll } from '../data';

export const POLL_MAX_ANSWERS = 10;
export const POLL_MIN_ANSWERS = 2;
const DURATIONS: { hours: number; label: string }[] = [
  { hours: 1, label: '1 hour' },
  { hours: 4, label: '4 hours' },
  { hours: 8, label: '8 hours' },
  { hours: 24, label: '24 hours' },
  { hours: 72, label: '3 days' },
  { hours: 168, label: '1 week' },
];

/** Validation shared with the tests: question 1-300, 2-10 answers of 1-55 characters. */
export function validatePoll(p: NewPoll): string | null {
  if (!p.question.trim()) return 'Ask a question.';
  if (p.question.length > 300) return 'The question is too long (300 characters at most).';
  const answers = p.answers.filter((a) => a.text.trim());
  if (answers.length < POLL_MIN_ANSWERS) return 'Add at least two answers.';
  if (answers.some((a) => a.text.length > 55)) return 'Answers can be 55 characters at most.';
  return null;
}

/** Create Poll modal (LNG-CMP-008). */
export function PollModal({
  open,
  onClose,
  onCreate,
}: {
  open: boolean;
  onClose: () => void;
  onCreate: (poll: NewPoll) => void;
}) {
  const [question, setQuestion] = useState('');
  const [answers, setAnswers] = useState<{ text: string; emoji: string }[]>([
    { text: '', emoji: '' },
    { text: '', emoji: '' },
  ]);
  const [hours, setHours] = useState(24);
  const [multi, setMulti] = useState(false);
  const [touched, setTouched] = useState(false);

  const poll: NewPoll = {
    question: question.trim(),
    answers: answers
      .filter((a) => a.text.trim())
      .map((a) => ({ text: a.text.trim(), ...(a.emoji.trim() ? { emoji: a.emoji.trim() } : {}) })),
    durationHours: hours,
    multi,
  };
  const error = validatePoll({ ...poll });

  const reset = () => {
    setQuestion('');
    setAnswers([
      { text: '', emoji: '' },
      { text: '', emoji: '' },
    ]);
    setHours(24);
    setMulti(false);
    setTouched(false);
  };
  const close = () => {
    reset();
    onClose();
  };
  const set = (i: number, patch: Partial<{ text: string; emoji: string }>) =>
    setAnswers((list) => list.map((a, j) => (j === i ? { ...a, ...patch } : a)));

  return (
    <Modal
      open={open}
      onClose={close}
      title="Create Poll"
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              setTouched(true);
              if (error) return;
              onCreate(poll);
              reset();
            }}
          >
            Post
          </Button>
        </>
      }
    >
      <form
        className="lng-form"
        onSubmit={(e) => {
          e.preventDefault();
        }}
      >
        <label className="lng-field">
          <span>Question</span>
          <input
            type="text"
            value={question}
            maxLength={300}
            placeholder="What question do you want to ask?"
            onChange={(e) => setQuestion(e.target.value)}
            data-autofocus
          />
        </label>
        <fieldset className="lng-poll-fields">
          <legend>Answers</legend>
          {answers.map((a, i) => (
            <div key={i} className="lng-poll-field">
              <input
                type="text"
                className="lng-poll-emoji"
                aria-label={`Emoji for answer ${i + 1}`}
                value={a.emoji}
                maxLength={8}
                placeholder="🙂"
                onChange={(e) => set(i, { emoji: e.target.value })}
              />
              <input
                type="text"
                aria-label={`Answer ${i + 1}`}
                value={a.text}
                maxLength={55}
                placeholder="Type your answer"
                onChange={(e) => set(i, { text: e.target.value })}
              />
              {answers.length > POLL_MIN_ANSWERS && (
                <button
                  type="button"
                  className="lng-link-btn lng-danger"
                  aria-label={`Remove answer ${i + 1}`}
                  onClick={() => setAnswers((l) => l.filter((_, j) => j !== i))}
                >
                  Remove
                </button>
              )}
            </div>
          ))}
          {answers.length < POLL_MAX_ANSWERS && (
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => setAnswers((l) => [...l, { text: '', emoji: '' }])}
            >
              Add another answer
            </button>
          )}
        </fieldset>
        <label className="lng-field">
          <span>Duration</span>
          <select value={hours} onChange={(e) => setHours(Number(e.target.value))}>
            {DURATIONS.map((d) => (
              <option key={d.hours} value={d.hours}>
                {d.label}
              </option>
            ))}
          </select>
        </label>
        <label className="lng-check">
          <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
          <span>Allow multiple answers</span>
        </label>
        {touched && error && (
          <p className="lng-failed" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
