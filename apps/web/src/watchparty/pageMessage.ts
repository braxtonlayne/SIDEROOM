import type { MessageKey } from '../i18n';

export interface PartyError {
  kind?: string;
  message?: string;
}

/**
 * Title and description of the watch party page when it cannot show the party. A description that only repeats
 * the title (the "gone" error carries the title's own sentence as its message) is dropped, so the page does not
 * say the same thing twice (XC-VIS-001).
 */
export function partyMessage(
  status: 'error' | 'ended',
  error: PartyError | null | undefined,
  t: (key: MessageKey) => string,
): { title: string; description?: string } {
  const title =
    status === 'ended'
      ? error?.kind === 'removed'
        ? t('watchparty.page.removed')
        : t('watchparty.page.ended')
      : error?.kind === 'age'
        ? t('watchparty.page.ageTitle')
        : error?.kind === 'gone'
          ? t('watchparty.page.gone')
          : t('watchparty.page.couldNotOpen');
  // A party that ended (or that removed the viewer) is not a connection problem: no connection hint there.
  const description =
    status === 'ended'
      ? error?.message
      : error?.kind === 'age'
      ? t('watchparty.page.ageDescription')
      : error?.kind === 'gone'
        ? t('watchparty.page.goneDescription')
        : (error?.message ?? t('watchparty.page.checkConnection'));
  const norm = (s: string) =>
    s
      .trim()
      .replace(/[.!]+$/, '')
      .toLowerCase();
  return !description || norm(description) === norm(title) ? { title } : { title, description };
}
