/**
 * Age-restricted channel interstitial (LNG-CHN-022, D-032). The first time someone opens an age-restricted
 * channel they are asked "Are you 18 or older?". The answer is sent to `POST /sr/age-gate {over_18}` (the same
 * call the Library reader uses for restricted series) and remembered on this device, because the API has no
 * call that reads the stored answer back.
 *
 * This file is deliberately small and has no Lounge dependencies: `postAgeGate` and `AgeConfirmPrompt` are
 * what a shared `useAgeGate()` hook for the Library would wrap too.
 */
import { Button, Icon } from '@sideroom/ui';
import { useCallback, useState } from 'react';
import { create } from 'zustand';
import { apiFetch } from '../api/client';
import { useT } from '../i18n';
import { useStore } from '../store';

export type AgeAnswer = 'yes' | 'no' | null;

const KEY = 'sr.age.answer';

const storageKey = (userId: string | null) => `${KEY}:${userId ?? ''}`;

export function readAgeAnswer(userId: string | null): AgeAnswer {
  try {
    const v = localStorage.getItem(storageKey(userId));
    return v === 'yes' || v === 'no' ? v : null;
  } catch {
    return null;
  }
}

function saveAgeAnswer(userId: string | null, answer: Exclude<AgeAnswer, null>) {
  try {
    localStorage.setItem(storageKey(userId), answer);
  } catch {
    /* private mode: the question is simply asked again next time */
  }
}

/** Settings re-confirmed or reset the stored answer: keep this device's copy in step (LNG-USET-015). */
export function setLocalAgeAnswer(userId: string | null, answer: AgeAnswer) {
  try {
    if (answer === null) localStorage.removeItem(storageKey(userId));
    else localStorage.setItem(storageKey(userId), answer);
  } catch {
    /* private mode */
  }
  useAgeAnswers.setState((s) => {
    const answers = { ...s.answers };
    if (answer === null) delete answers[userId ?? ''];
    else answers[userId ?? ''] = answer;
    return { answers };
  });
}

/** Record the answer on the account (permanent: `true` confirms, `false` hides restricted content). */
export const postAgeGate = (over18: boolean) =>
  apiFetch<void>('/sr/age-gate', { method: 'POST', json: { over_18: over18 }, retries: 0 });

/** The question and its two buttons. */
export function AgeConfirmPrompt({
  title,
  busy,
  error,
  onAnswer,
}: {
  title?: string;
  busy?: boolean;
  error?: string | null;
  onAnswer: (over18: boolean) => void;
}) {
  const t = useT();
  return (
    <div className="lng-agegate" role="group" aria-labelledby="agegate-h">
      <Icon name="alert" size={24} />
      <h2 id="agegate-h">{title ?? t('lounge.ageGate.defaultTitle')}</h2>
      <p>{t('lounge.ageGate.question')}</p>
      {error && (
        <p role="alert" className="lng-failed">
          {error}
        </p>
      )}
      <div className="lng-agegate-actions">
        <Button variant="ghost" disabled={busy} onClick={() => onAnswer(false)}>
          {t('lounge.ageGate.no')}
        </Button>
        <Button loading={busy} onClick={() => onAnswer(true)}>
          {t('lounge.ageGate.yes')}
        </Button>
      </div>
    </div>
  );
}

/** Answers given in this session, shared by every place that gates (the channel and its threads). */
export const useAgeAnswers = create<{ answers: Record<string, AgeAnswer> }>(() => ({
  answers: {},
}));

/** The viewer said they are 18 or older (on this device or earlier in this session). */
export function useAgeCleared(): boolean {
  const userId = useStore((s) => s.currentUserId);
  return useAgeAnswers((s) => s.answers[userId ?? ''] ?? readAgeAnswer(userId)) === 'yes';
}

export type GateState = 'open' | 'ask' | 'blocked' | 'wait';

/**
 * Whether a restricted channel may be shown. `open` for channels that are not restricted or after a yes,
 * `ask` before any answer, `blocked` after a no.
 */
export function useAgeGate(restricted: boolean | undefined) {
  const t = useT();
  const userId = useStore((s) => s.currentUserId);
  const answer = useAgeAnswers((s) => s.answers[userId ?? ''] ?? readAgeAnswer(userId));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = useCallback(
    async (over18: boolean) => {
      setBusy(true);
      setError(null);
      try {
        await postAgeGate(over18);
        const a = over18 ? 'yes' : 'no';
        saveAgeAnswer(userId, a);
        useAgeAnswers.setState((s) => ({ answers: { ...s.answers, [userId ?? '']: a } }));
        return over18;
      } catch {
        setError(t('lounge.ageGate.saveFailed'));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [userId, t],
  );

  // The saved answer is keyed by account, so a restricted channel waits until the account is known (a page
  // reload shows the channel from REST before the gateway says who is signed in).
  const state: GateState = !restricted
    ? 'open'
    : userId === null
      ? 'wait'
      : answer === 'yes'
        ? 'open'
        : answer === 'no'
          ? 'blocked'
          : 'ask';
  return { state, busy, error, submit };
}
