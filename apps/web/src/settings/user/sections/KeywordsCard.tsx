// Notification keywords (GET/PUT /users/@me/notification-keywords): words that notify like a mention and
// highlight the message in the Lounge. The server cleans the list (trim, dedupe, 50 words of 100 characters).
import { Button, Notice, TextInput, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../../i18n';
import { useStore } from '../../../store';
import { errorText, getNotificationKeywords, putNotificationKeywords, useResource } from '../api';
import { Card, Loaded } from '../parts';

export const MAX_KEYWORDS = 50;
export const MAX_KEYWORD_LENGTH = 100;

export function KeywordsCard() {
  const t = useT();
  const toast = useToast();
  const res = useResource(getNotificationKeywords);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Another session saved a list: follow it.
  const live = useStore((s) => s.notificationKeywords);
  useEffect(() => {
    if (live) res.setData({ keywords: live });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [live]);

  const save = async (next: string[]) => {
    setBusy(true);
    setError(null);
    try {
      const saved = await putNotificationKeywords(next);
      res.setData(saved);
      useStore
        .getState()
        .applyDispatch('SR_NOTIFICATION_KEYWORDS_UPDATE', { keywords: saved.keywords });
      toast.show({ title: t('settings.keywords.saved'), duration: 1500 });
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const add = async (current: string[]) => {
    const word = draft.trim().replace(/\s+/g, ' ');
    if (!word) return;
    if (word.length > MAX_KEYWORD_LENGTH)
      return setError(t('settings.keywords.tooLong', { max: MAX_KEYWORD_LENGTH }));
    if (!/[\p{L}\p{N}]/u.test(word)) return setError(t('settings.keywords.needsLetter'));
    if (current.some((k) => k.toLowerCase() === word.toLowerCase()))
      return setError(t('settings.keywords.duplicate'));
    if (current.length >= MAX_KEYWORDS)
      return setError(t('settings.keywords.tooMany', { max: MAX_KEYWORDS }));
    if (await save([...current, word])) setDraft('');
  };
  return (
    <Card
      id="us-keywords"
      title={t('settings.keywords.title')}
      description={t('settings.keywords.desc')}
    >
      <Loaded res={res} what={t('settings.keywords.what')}>
        {({ keywords }) => (
          <div className="us-stack">
            <form
              className="us-actions"
              aria-label={t('settings.keywords.formAria')}
              onSubmit={(e) => {
                e.preventDefault();
                void add(keywords);
              }}
            >
              <TextInput
                label={t('settings.keywords.add')}
                value={draft}
                maxLength={MAX_KEYWORD_LENGTH}
                onChange={(e) => setDraft(e.target.value)}
              />
              <Button type="submit" loading={busy} disabled={!draft.trim()}>
                {t('settings.keywords.addButton')}
              </Button>
            </form>
            {keywords.length === 0 ? (
              <p className="us-muted">{t('settings.keywords.empty')}</p>
            ) : (
              <ul className="us-list">
                {keywords.map((k) => (
                  <li key={k}>
                    <span className="us-row-main">{k}</span>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={busy}
                      onClick={() => void save(keywords.filter((x) => x !== k))}
                    >
                      {t('settings.keywords.remove')}
                      <span className="us-sr"> {k}</span>
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            <p className="us-muted">
              {t('settings.keywords.count', { n: keywords.length, max: MAX_KEYWORDS })}
            </p>
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.keywords.error')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}
