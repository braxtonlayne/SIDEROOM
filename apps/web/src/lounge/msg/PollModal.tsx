import { Button, Modal } from '@sideroom/ui';
import { useState } from 'react';
import { t, useT } from '../../i18n';
import type { NewPoll } from '../data';
import { PollTemplates } from './PollTemplates';

export const POLL_MAX_ANSWERS = 10;
export const POLL_MIN_ANSWERS = 2;
/** `labelKey` is a catalog key, resolved when rendered. */
const DURATIONS: { hours: number; labelKey: string }[] = [
  { hours: 1, labelKey: 'lounge.pollModal.duration1h' },
  { hours: 4, labelKey: 'lounge.pollModal.duration4h' },
  { hours: 8, labelKey: 'lounge.pollModal.duration8h' },
  { hours: 24, labelKey: 'lounge.pollModal.duration24h' },
  { hours: 72, labelKey: 'lounge.pollModal.duration3d' },
  { hours: 168, labelKey: 'lounge.pollModal.duration1w' },
];

/** Validation shared with the tests: question 1-300, 2-10 answers of 1-55 characters. */
export function validatePoll(p: NewPoll): string | null {
  if (!p.question.trim()) return t('lounge.pollModal.askQuestion');
  if (p.question.length > 300) return t('lounge.pollModal.questionTooLong');
  const answers = p.answers.filter((a) => a.text.trim());
  if (answers.length < POLL_MIN_ANSWERS) return t('lounge.pollModal.addTwoAnswers');
  if (answers.some((a) => a.text.length > 55)) return t('lounge.pollModal.answerTooLong');
  return null;
}

/** Create Poll modal (LNG-CMP-008). */
export function PollModal({
  open,
  onClose,
  onCreate,
  guildId,
}: {
  open: boolean;
  onClose: () => void;
  onCreate: (poll: NewPoll) => void;
  /** In a server, the poll templates it saved are offered (FTR-POLLT-001). */
  guildId?: string | undefined;
}) {
  const t = useT();
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
      title={t('lounge.pollModal.title')}
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t('lounge.pollModal.cancel')}
          </Button>
          <Button
            onClick={() => {
              setTouched(true);
              if (error) return;
              onCreate(poll);
              reset();
            }}
          >
            {t('lounge.pollModal.post')}
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
        {guildId && (
          <PollTemplates
            guildId={guildId}
            current={{
              question,
              answers: answers.map((a) => a.text),
              hours,
              multi,
            }}
            onPick={(f) => {
              setQuestion(f.question);
              setAnswers(f.answers.map((text) => ({ text, emoji: '' })));
              setHours(f.hours);
              setMulti(f.multi);
            }}
          />
        )}
        <label className="lng-field">
          <span>{t('lounge.pollModal.question')}</span>
          <input
            type="text"
            value={question}
            maxLength={300}
            placeholder={t('lounge.pollModal.questionPlaceholder')}
            onChange={(e) => setQuestion(e.target.value)}
            data-autofocus
          />
        </label>
        <fieldset className="lng-poll-fields">
          <legend>{t('lounge.pollModal.answers')}</legend>
          {answers.map((a, i) => (
            <div key={i} className="lng-poll-field">
              <input
                type="text"
                className="lng-poll-emoji"
                aria-label={t('lounge.pollModal.emojiForAnswer', { n: i + 1 })}
                value={a.emoji}
                maxLength={8}
                placeholder="🙂"
                onChange={(e) => set(i, { emoji: e.target.value })}
              />
              <input
                type="text"
                aria-label={t('lounge.pollModal.answerLabel', { n: i + 1 })}
                value={a.text}
                maxLength={55}
                placeholder={t('lounge.pollModal.answerPlaceholder')}
                onChange={(e) => set(i, { text: e.target.value })}
              />
              {answers.length > POLL_MIN_ANSWERS && (
                <button
                  type="button"
                  className="lng-link-btn lng-danger"
                  aria-label={t('lounge.pollModal.removeAnswer', { n: i + 1 })}
                  onClick={() => setAnswers((l) => l.filter((_, j) => j !== i))}
                >
                  {t('lounge.pollModal.remove')}
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
              {t('lounge.pollModal.addAnswer')}
            </button>
          )}
        </fieldset>
        <label className="lng-field">
          <span>{t('lounge.pollModal.duration')}</span>
          <select value={hours} onChange={(e) => setHours(Number(e.target.value))}>
            {!DURATIONS.some((d) => d.hours === hours) && (
              <option value={hours}>{t('lounge.pollModal.durationCustom', { n: hours })}</option>
            )}
            {DURATIONS.map((d) => (
              <option key={d.hours} value={d.hours}>
                {t(d.labelKey)}
              </option>
            ))}
          </select>
        </label>
        <label className="lng-check">
          <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
          <span>{t('lounge.pollModal.allowMultiple')}</span>
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
